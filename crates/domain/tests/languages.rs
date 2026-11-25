use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use corvo_domain::port::Fs;
use corvo_domain::{Node, languages};

struct Disk(HashMap<PathBuf, String>);

impl Fs for Disk {
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>> {
        let mut nodes: Vec<Node> = Vec::new();
        for path in self.0.keys() {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let child = dir.join(first);
            let node = if parts.next().is_some() {
                Node::dir(child)
            } else {
                Node::file(child)
            };
            if !nodes.contains(&node) {
                nodes.push(node);
            }
        }
        Ok(nodes)
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

fn disk(paths: &[&str]) -> Disk {
    Disk(
        paths
            .iter()
            .map(|path| (PathBuf::from(path), String::new()))
            .collect(),
    )
}

#[test]
fn languages_count_files_most_first() {
    let disk = disk(&[
        "/p/crates/a/src/lib.rs",
        "/p/crates/a/src/main.rs",
        "/p/crates/b/src/lib.rs",
        "/p/web/src/app.ts",
        "/p/web/src/view.TSX",
        "/p/web/src/App.svelte",
        "/p/web/package.json",
        "/p/README.md",
        "/p/Cargo.toml",
    ]);

    let found = languages(&disk, Path::new("/p"));

    let summary: Vec<_> = found
        .iter()
        .map(|share| (share.name(), share.files()))
        .collect();
    assert_eq!(summary, [("Rust", 3), ("TypeScript", 2), ("Svelte", 1)]);
}

#[test]
fn built_and_vendored_folders_do_not_count() {
    let disk = disk(&[
        "/p/src/main.rs",
        "/p/node_modules/pkg/index.js",
        "/p/target/debug/build/x.rs",
    ]);

    let found = languages(&disk, Path::new("/p"));

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name(), "Rust");
}

#[test]
fn a_project_of_only_data_has_no_language() {
    let disk = disk(&["/p/data.json", "/p/notes.md", "/p/Makefile", "/p/.bashrc"]);

    assert!(languages(&disk, Path::new("/p")).is_empty());
}
