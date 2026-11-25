use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use corvo_domain::port::Fs;
use corvo_domain::{Engine, Node, survey};

#[derive(Default)]
struct Disk {
    files: HashMap<PathBuf, String>,
}

impl Disk {
    fn with(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
                .collect(),
        }
    }
}

impl Fs for Disk {
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>> {
        let mut nodes: Vec<Node> = Vec::new();
        for path in self.files.keys() {
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
        self.files
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, _: &Path, _: &str) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_rust_store_with_rusqlite() {
    let disk = Disk::with(&[
        ("/p/Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]"),
        (
            "/p/crates/store/Cargo.toml",
            "[dependencies]\nrusqlite = { version = \"0.42\" }",
        ),
        (
            "/p/crates/store/src/sqlite.rs",
            "use rusqlite::Connection;\nfn put(t: &T) { t.execute(\"x\", []); }",
        ),
        ("/p/crates/web/src/lib.rs", "fn page() {}"),
    ]);

    let usage = survey(&disk, Path::new("/p"));

    assert_eq!(usage.engines(), [Engine::Sqlite]);
    assert_eq!(
        usage.evidence(Engine::Sqlite),
        ["crates/store/Cargo.toml · rusqlite"]
    );
    assert_eq!(usage.of(Path::new("/p/crates/store")), [Engine::Sqlite]);
    assert_eq!(usage.of(Path::new("/p/crates")), [Engine::Sqlite]);
    assert!(usage.of(Path::new("/p/crates/web")).is_empty());
}

#[test]
fn code_mentioning_a_database_the_project_lacks_is_not_tied() {
    let disk = Disk::with(&[
        ("/p/package.json", "{ \"dependencies\": { \"pg\": \"8\" } }"),
        ("/p/src/cache.ts", "// TODO redis someday\nconst n = 1;"),
        (
            "/p/src/db.ts",
            "export const run = () => pool.query('select 1');",
        ),
    ]);

    let usage = survey(&disk, Path::new("/p"));

    assert_eq!(usage.engines(), [Engine::Postgres]);
    assert!(usage.of(Path::new("/p/src/cache.ts")).is_empty());
    assert_eq!(usage.of(Path::new("/p/src/db.ts")), [Engine::Postgres]);
}

#[test]
fn env_keys_compose_images_and_prisma_providers_count() {
    let disk = Disk::with(&[
        (
            "/p/.env.example",
            "DB_CONNECTION=mysql\nSUPABASE_URL=https://x.supabase.co\n# REDIS_HOST=x\n",
        ),
        (
            "/p/docker-compose.yml",
            "services:\n  cache:\n    image: redis:7\n",
        ),
        (
            "/p/prisma/schema.prisma",
            "generator client {\n  provider = \"prisma-client-js\"\n}\ndatasource db {\n  provider = \"mongodb\"\n}\n",
        ),
        ("/p/data/app.sqlite", ""),
    ]);

    let usage = survey(&disk, Path::new("/p"));

    assert_eq!(
        usage.engines(),
        [
            Engine::Sqlite,
            Engine::MySql,
            Engine::Supabase,
            Engine::MongoDb,
            Engine::Redis
        ]
    );
    assert_eq!(
        usage.evidence(Engine::MySql),
        [".env.example · DB_CONNECTION"]
    );
    assert_eq!(usage.evidence(Engine::Sqlite), ["data/app.sqlite"]);
}

#[test]
fn env_values_never_reach_the_evidence() {
    let disk = Disk::with(&[(
        "/p/.env",
        "DATABASE_URL=postgres://admin:hunter2@db:5432/app\n",
    )]);

    let usage = survey(&disk, Path::new("/p"));

    assert_eq!(usage.evidence(Engine::Postgres), [".env · DATABASE_URL"]);
}

#[test]
fn plain_sql_ties_to_every_sql_database() {
    let disk = Disk::with(&[
        (
            "/p/composer.json",
            "{ \"require\": { \"predis/predis\": \"2\" } }",
        ),
        ("/p/.env", "DB_CONNECTION=pgsql\n"),
        (
            "/p/app/Repo.php",
            "<?php function all() { return DB::table('users')->get(); }",
        ),
    ]);

    let usage = survey(&disk, Path::new("/p"));

    assert_eq!(usage.of(Path::new("/p/app")), [Engine::Postgres]);
    assert_eq!(
        usage.code("Cache::put(); Redis::get('k');"),
        [Engine::Redis]
    );
    assert!(usage.code("fn idle() {}").is_empty());
}

#[test]
fn skipped_folders_are_not_read() {
    let disk = Disk::with(&[
        ("/p/package.json", "{}"),
        ("/p/node_modules/pg/package.json", "{ \"name\": \"pg\" }"),
    ]);

    assert!(survey(&disk, Path::new("/p")).engines().is_empty());
}

#[test]
fn sqlite_from_the_language_counts_only_as_an_import() {
    let quoted = Disk::with(&[(
        "/p/src/notes.ts",
        "const s = 'import sqlite3 or bun:sqlite';",
    )]);
    assert!(survey(&quoted, Path::new("/p")).engines().is_empty());

    let imported = Disk::with(&[
        (
            "/p/src/history.ts",
            "import { DatabaseSync } from 'node:sqlite';",
        ),
        ("/p/tool.py", "import sqlite3\n"),
    ]);
    let usage = survey(&imported, Path::new("/p"));
    assert_eq!(usage.engines(), [Engine::Sqlite]);
    assert_eq!(usage.of(Path::new("/p/src/history.ts")), [Engine::Sqlite]);
}
