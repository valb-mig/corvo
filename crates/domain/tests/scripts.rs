use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use corvo_domain::port::Fs;
use corvo_domain::{Node, Script, scripts};

struct Disk(HashMap<PathBuf, String>);

impl Fs for Disk {
    fn list(&self, _: &Path) -> io::Result<Vec<Node>> {
        Ok(Vec::new())
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.0
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, _: &Path, _: &str) -> io::Result<()> {
        Ok(())
    }
}

fn disk(files: &[(&str, &str)]) -> Disk {
    Disk(
        files
            .iter()
            .map(|(path, text)| (Path::new("/p").join(path), (*text).to_owned()))
            .collect(),
    )
}

fn found(files: &[(&str, &str)]) -> Vec<Script> {
    scripts(&disk(files), Path::new("/p"))
}

fn commands(scripts: &[Script]) -> Vec<&str> {
    scripts.iter().map(Script::command).collect()
}

#[test]
fn a_project_with_no_script_file_has_none() {
    assert!(found(&[("README.md", "# p")]).is_empty());
}

#[test]
fn justfile_recipes_run_with_just_in_their_order() {
    let justfile = "\
jobs := \"4\"
set dotenv-load
alias b := build

# Runs the app
dev:
    cargo run -j {{jobs}}

build: dev
    @cargo build

[private]
helper:
    echo hidden

_secret:
    echo hidden
";
    let scripts = found(&[("justfile", justfile)]);

    assert_eq!(commands(&scripts), ["just dev", "just build"]);
    assert_eq!(scripts[0].about(), Some("Runs the app"));
    assert_eq!(scripts[1].about(), Some("cargo build"));
    assert!(scripts.iter().all(|script| script.source() == "justfile"));
}

#[test]
fn a_recipe_that_needs_arguments_waits_for_them() {
    let justfile = "\
deploy env:
    ./deploy {{env}}

serve port='8080':
    ./serve {{port}}

test *flags:
    cargo test {{flags}}

each +files:
    echo {{files}}
";
    let scripts = found(&[("Justfile", justfile)]);

    let asks: Vec<_> = scripts.iter().map(|s| (s.name(), s.asks())).collect();
    assert_eq!(
        asks,
        [
            ("deploy", true),
            ("serve", false),
            ("test", false),
            ("each", true)
        ]
    );
    assert_eq!(scripts[0].command(), "just deploy ");
    assert_eq!(scripts[1].command(), "just serve");
}

#[test]
fn package_scripts_keep_the_order_of_the_file() {
    let package = r#"{
        "name": "web",
        "scripts": { "dev": "vite", "build": "vite build", "lint": "eslint ." }
    }"#;
    let scripts = found(&[("package.json", package)]);

    assert_eq!(
        commands(&scripts),
        ["npm run dev", "npm run build", "npm run lint"]
    );
    assert_eq!(scripts[1].about(), Some("vite build"));
}

#[test]
fn package_scripts_run_with_the_manager_of_the_lock_file() {
    let package = r#"{ "scripts": { "dev": "vite" } }"#;
    for (lock, command) in [
        ("pnpm-lock.yaml", "pnpm run dev"),
        ("yarn.lock", "yarn run dev"),
        ("bun.lock", "bun run dev"),
        ("bun.lockb", "bun run dev"),
        ("package-lock.json", "npm run dev"),
    ] {
        let scripts = found(&[("package.json", package), (lock, "")]);
        assert_eq!(commands(&scripts), [command], "with {lock}");
    }
}

#[test]
fn with_no_lock_file_the_named_package_manager_runs_them() {
    let package = r#"{ "packageManager": "pnpm@9.1.0", "scripts": { "dev": "vite" } }"#;
    assert_eq!(
        commands(&found(&[("package.json", package)])),
        ["pnpm run dev"]
    );
}

#[test]
fn hooks_of_another_script_are_left_out() {
    let package = r#"{ "scripts": {
        "prebuild": "rm -rf dist", "build": "tsc", "postbuild": "echo ok",
        "prepare": "husky", "start:prod": "node dist"
    } }"#;
    assert_eq!(
        commands(&found(&[("package.json", package)])),
        ["npm run build", "npm run prepare", "npm run start:prod"]
    );
}

#[test]
fn a_name_the_shell_would_read_goes_in_quotes() {
    let package = r#"{ "scripts": { "build all": "tsc", "it's": "echo" } }"#;
    assert_eq!(
        commands(&found(&[("package.json", package)])),
        ["npm run 'build all'", r"npm run 'it'\''s'"]
    );
}

#[test]
fn a_broken_package_json_has_no_script() {
    assert!(found(&[("package.json", "{ \"scripts\": ")]).is_empty());
}

#[test]
fn composer_scripts_run_with_composer_but_its_own_events() {
    let composer = r#"{
        "scripts": {
            "post-install-cmd": "@php artisan key:generate",
            "test": "phpunit",
            "check": ["@lint", "@test"],
            "lint": "pint"
        },
        "scripts-descriptions": { "lint": "Formats the code" }
    }"#;
    let scripts = found(&[("composer.json", composer)]);

    assert_eq!(
        commands(&scripts),
        [
            "composer run test",
            "composer run check",
            "composer run lint"
        ]
    );
    assert_eq!(scripts[0].about(), Some("phpunit"));
    assert_eq!(scripts[1].about(), Some("@lint && @test"));
    assert_eq!(scripts[2].about(), Some("Formats the code"));
}

#[test]
fn makefile_targets_run_with_make() {
    let makefile = "\
CC := gcc
FLAGS = -O2
.PHONY: build test
build: main.o ## Builds the binary
\t$(CC) -o app main.o
%.o: %.c
\t$(CC) -c $<
test install: build
$(OUT): build
";
    let scripts = found(&[("Makefile", makefile)]);

    assert_eq!(
        commands(&scripts),
        ["make build", "make test", "make install"]
    );
    assert_eq!(scripts[0].about(), Some("Builds the binary"));
    assert_eq!(scripts[1].about(), None);
}

#[test]
fn every_file_lists_its_scripts_one_after_another() {
    let scripts = found(&[
        ("Makefile", "all:\n\techo\n"),
        ("composer.json", r#"{ "scripts": { "test": "phpunit" } }"#),
        ("package.json", r#"{ "scripts": { "dev": "vite" } }"#),
        ("justfile", "check:\n    cargo check\n"),
    ]);

    let sources: Vec<_> = scripts.iter().map(Script::source).collect();
    assert_eq!(
        sources,
        ["justfile", "package.json", "composer.json", "Makefile"]
    );
}
