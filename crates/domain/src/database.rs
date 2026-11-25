use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::port::Fs;
use crate::scan;

// The databases a project works with, and the code that talks to them.
//
// A database is known from evidence: a dependency in a manifest, a key in an env file (never its
// value), an image in a compose file, a Prisma provider, a database file. Code is then marked by
// what it mentions: a client by name (`rusqlite`, `supabase`) ties it to that database, and a
// plain SQL call (`.execute(`, `DB::`, `sqlx::`) to every SQL database the project has.

const SIZE: usize = 512 * 1024;
const EVIDENCE: usize = 3;

const CODE: &[&str] = &[
    "rs", "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "py", "java", "kt", "kts", "php",
    "go", "rb", "cs", "swift", "dart", "vue", "svelte", "sql",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Engine {
    Sqlite,
    Postgres,
    MySql,
    Supabase,
    MongoDb,
    Redis,
    Firebase,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Engine::Sqlite => "SQLite",
            Engine::Postgres => "PostgreSQL",
            Engine::MySql => "MySQL",
            Engine::Supabase => "Supabase",
            Engine::MongoDb => "MongoDB",
            Engine::Redis => "Redis",
            Engine::Firebase => "Firebase",
        }
    }

    fn speaks_sql(self) -> bool {
        matches!(self, Engine::Sqlite | Engine::Postgres | Engine::MySql)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Mark {
    Engine(Engine),
    Sql,
}

use Engine::*;

const DEPENDENCIES: &[(&str, Engine)] = &[
    ("rusqlite", Sqlite),
    ("sqlite", Sqlite),
    ("sqlite3", Sqlite),
    ("libsql", Sqlite),
    ("better-sqlite3", Sqlite),
    ("@libsql/client", Sqlite),
    ("sql.js", Sqlite),
    ("aiosqlite", Sqlite),
    ("sqlite-jdbc", Sqlite),
    ("postgres", Postgres),
    ("postgresql", Postgres),
    ("tokio-postgres", Postgres),
    ("pg", Postgres),
    ("pg-promise", Postgres),
    ("psycopg", Postgres),
    ("psycopg2", Postgres),
    ("psycopg2-binary", Postgres),
    ("asyncpg", Postgres),
    ("@neondatabase/serverless", Postgres),
    ("@vercel/postgres", Postgres),
    ("mysql", MySql),
    ("mysql2", MySql),
    ("mysql_async", MySql),
    ("mysqlclient", MySql),
    ("pymysql", MySql),
    ("mysql-connector-python", MySql),
    ("mysql-connector-j", MySql),
    ("mysql-connector-java", MySql),
    ("mariadb", MySql),
    ("@supabase/supabase-js", Supabase),
    ("@supabase/ssr", Supabase),
    ("supabase", Supabase),
    ("postgrest", Supabase),
    ("mongodb", MongoDb),
    ("mongoose", MongoDb),
    ("pymongo", MongoDb),
    ("motor", MongoDb),
    ("redis", Redis),
    ("ioredis", Redis),
    ("predis/predis", Redis),
    ("@upstash/redis", Redis),
    ("deadpool-redis", Redis),
    ("firebase", Firebase),
    ("firebase-admin", Firebase),
    ("@google-cloud/firestore", Firebase),
];

// Matched in lowercase code.
const MARKS: &[(&str, Mark)] = &[
    ("sqlite", Mark::Engine(Sqlite)),
    ("postgres", Mark::Engine(Postgres)),
    ("pgpool", Mark::Engine(Postgres)),
    ("psycopg", Mark::Engine(Postgres)),
    ("asyncpg", Mark::Engine(Postgres)),
    ("mysql", Mark::Engine(MySql)),
    ("mariadb", Mark::Engine(MySql)),
    ("supabase", Mark::Engine(Supabase)),
    ("mongo", Mark::Engine(MongoDb)),
    ("redis", Mark::Engine(Redis)),
    ("firestore", Mark::Engine(Firebase)),
    ("firebase", Mark::Engine(Firebase)),
    ("sqlx::", Mark::Sql),
    ("diesel::", Mark::Sql),
    ("sea_orm", Mark::Sql),
    ("params!", Mark::Sql),
    (".execute(", Mark::Sql),
    (".prepare(", Mark::Sql),
    (".query(", Mark::Sql),
    (".query_row(", Mark::Sql),
    (".query_map(", Mark::Sql),
    ("prisma.", Mark::Sql),
    ("knex", Mark::Sql),
    ("drizzle", Mark::Sql),
    ("db::", Mark::Sql),
    ("schema::create", Mark::Sql),
    ("::where(", Mark::Sql),
    ("->where(", Mark::Sql),
    ("::query(", Mark::Sql),
    ("session.query", Mark::Sql),
    ("cursor()", Mark::Sql),
    ("jdbctemplate", Mark::Sql),
    ("entitymanager", Mark::Sql),
    ("preparedstatement", Mark::Sql),
    ("sql`", Mark::Sql),
    ("insert into", Mark::Sql),
    ("delete from", Mark::Sql),
    ("create table", Mark::Sql),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    found: BTreeMap<Engine, Vec<String>>,
    files: HashMap<PathBuf, Vec<Engine>>,
}

impl Usage {
    pub fn engines(&self) -> Vec<Engine> {
        self.found.keys().copied().collect()
    }

    // Where each database was seen, a few places at most.
    pub fn evidence(&self, engine: Engine) -> &[String] {
        self.found.get(&engine).map_or(&[], Vec::as_slice)
    }

    // The databases a file talks to, or any file inside a folder.
    pub fn of(&self, path: &Path) -> Vec<Engine> {
        let mut engines = BTreeSet::new();
        for (file, used) in &self.files {
            if file.starts_with(path) {
                engines.extend(used.iter().copied());
            }
        }
        engines.into_iter().collect()
    }

    // The databases a piece of code talks to.
    pub fn code(&self, text: &str) -> Vec<Engine> {
        resolve(&marks(text), &self.found)
    }
}

pub fn survey(fs: &dyn Fs, root: &Path) -> Usage {
    let mut found: BTreeMap<Engine, Vec<String>> = BTreeMap::new();
    let mut marked: Vec<(PathBuf, BTreeSet<Mark>)> = Vec::new();
    scan::files(fs, root, |node| {
        let name = node.name();
        let path = node.path();
        let place = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        let mut note = |engine: Engine, why: Option<&str>| {
            let place = match why {
                Some(why) => format!("{place} · {why}"),
                None => place.clone(),
            };
            let places = found.entry(engine).or_default();
            if places.len() < EVIDENCE && !places.contains(&place) {
                places.push(place);
            }
        };
        let Some(kind) = kind(name) else {
            return;
        };
        if kind == Kind::Database {
            note(Sqlite, None);
            return;
        }
        let Some(text) = read(fs, path) else {
            return;
        };
        match kind {
            Kind::Database => {}
            Kind::Manifest => dependencies(&text, &mut note),
            Kind::Env => env(&text, &mut note),
            Kind::Compose => compose(&text, &mut note),
            Kind::Prisma => prisma(&text, &mut note),
            Kind::Code => {
                let marks = marks(&text);
                if stdlib_sqlite(&text) {
                    note(Sqlite, Some("sqlite import"));
                }
                if !marks.is_empty() {
                    marked.push((path.to_path_buf(), marks));
                }
            }
        }
    });
    let files = marked
        .into_iter()
        .filter_map(|(path, marks)| {
            let engines = resolve(&marks, &found);
            (!engines.is_empty()).then_some((path, engines))
        })
        .collect();
    Usage { found, files }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Database,
    Manifest,
    Env,
    Compose,
    Prisma,
    Code,
}

fn kind(name: &str) -> Option<Kind> {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, ext)| ext);
    let manifest = matches!(
        lower.as_str(),
        "cargo.toml"
            | "package.json"
            | "composer.json"
            | "pyproject.toml"
            | "pipfile"
            | "setup.py"
            | "setup.cfg"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "go.mod"
            | "gemfile"
    ) || (lower.starts_with("requirements") && lower.ends_with(".txt"));
    let compose = (lower.starts_with("docker-compose") || lower.starts_with("compose."))
        && matches!(ext, Some("yml" | "yaml"));
    let kind = if manifest {
        Kind::Manifest
    } else if lower == ".env" || lower.starts_with(".env.") {
        Kind::Env
    } else if compose {
        Kind::Compose
    } else if ext == Some("prisma") {
        Kind::Prisma
    } else if matches!(ext, Some("db" | "db3" | "sqlite" | "sqlite3")) && lower != "thumbs.db" {
        Kind::Database
    } else if ext.is_some_and(|ext| CODE.contains(&ext)) {
        Kind::Code
    } else {
        return None;
    };
    Some(kind)
}

fn read(fs: &dyn Fs, path: &Path) -> Option<String> {
    let bytes = fs.read(path).ok()?;
    if bytes.len() > SIZE {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn dependencies(text: &str, note: &mut impl FnMut(Engine, Option<&str>)) {
    let words = text.split(|c: char| !(c.is_ascii_alphanumeric() || "_@/.-".contains(c)));
    let mut seen = BTreeSet::new();
    for word in words {
        let word = word.to_ascii_lowercase();
        if let Some(&(dep, engine)) = DEPENDENCIES.iter().find(|(dep, _)| *dep == word)
            && seen.insert(dep)
        {
            note(engine, Some(dep));
        }
    }
}

// Keys only: a value can hold a password.
fn env(text: &str, note: &mut impl FnMut(Engine, Option<&str>)) {
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_start_matches("export ").trim();
        if key.starts_with('#') {
            continue;
        }
        let upper = key.to_ascii_uppercase();
        let value = value.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
        let engine = if upper == "DB_CONNECTION" {
            match value.as_str() {
                "mysql" | "mariadb" => Some(MySql),
                "pgsql" | "postgres" | "postgresql" => Some(Postgres),
                "sqlite" => Some(Sqlite),
                _ => None,
            }
        } else if value.starts_with("postgres://") || value.starts_with("postgresql://") {
            Some(Postgres)
        } else if value.starts_with("mysql://") || value.starts_with("mariadb://") {
            Some(MySql)
        } else if value.starts_with("mongodb://") || value.starts_with("mongodb+srv://") {
            Some(MongoDb)
        } else if value.starts_with("redis://") || value.starts_with("rediss://") {
            Some(Redis)
        } else if value.starts_with("sqlite:") {
            Some(Sqlite)
        } else if upper.contains("SUPABASE") {
            Some(Supabase)
        } else if upper.contains("FIREBASE") || upper.contains("FIRESTORE") {
            Some(Firebase)
        } else if upper.starts_with("REDIS") {
            Some(Redis)
        } else if upper.starts_with("MONGO") {
            Some(MongoDb)
        } else {
            None
        };
        if let Some(engine) = engine {
            note(engine, Some(key));
        }
    }
}

fn compose(text: &str, note: &mut impl FnMut(Engine, Option<&str>)) {
    for line in text.lines() {
        let Some((_, image)) = line.trim().split_once("image:") else {
            continue;
        };
        let image = image.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
        let base = image.split(':').next().unwrap_or_default();
        let engine = if base.contains("supabase") {
            Some(Supabase)
        } else if base.contains("postgres") || base.contains("postgis") {
            Some(Postgres)
        } else if base.contains("mysql") || base.contains("mariadb") {
            Some(MySql)
        } else if base.contains("mongo") {
            Some(MongoDb)
        } else if base.contains("redis") || base.contains("valkey") {
            Some(Redis)
        } else {
            None
        };
        if let Some(engine) = engine {
            note(engine, Some(base));
        }
    }
}

fn prisma(text: &str, note: &mut impl FnMut(Engine, Option<&str>)) {
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "provider" {
            continue;
        }
        let engine = match value.trim().trim_matches('"') {
            "postgresql" | "postgres" | "cockroachdb" => Some(Postgres),
            "mysql" => Some(MySql),
            "sqlite" => Some(Sqlite),
            "mongodb" => Some(MongoDb),
            _ => None,
        };
        if let Some(engine) = engine {
            note(engine, Some("provider"));
        }
    }
}

// SQLite that ships with the language: an import line, not a word in a string.
fn stdlib_sqlite(text: &str) -> bool {
    text.lines().map(str::trim_start).any(|line| {
        let module = line.contains("\"bun:sqlite\"")
            || line.contains("'bun:sqlite'")
            || line.contains("\"node:sqlite\"")
            || line.contains("'node:sqlite'");
        line.starts_with("import sqlite3")
            || line.starts_with("from sqlite3 ")
            || (module && (line.starts_with("import ") || line.contains("require(")))
    })
}

fn marks(text: &str) -> BTreeSet<Mark> {
    let lower = text.to_ascii_lowercase();
    MARKS
        .iter()
        .filter(|(mark, _)| lower.contains(mark))
        .map(|&(_, mark)| mark)
        .collect()
}

fn resolve(marks: &BTreeSet<Mark>, found: &BTreeMap<Engine, Vec<String>>) -> Vec<Engine> {
    let mut engines = BTreeSet::new();
    for mark in marks {
        match *mark {
            Mark::Engine(engine) if found.contains_key(&engine) => {
                engines.insert(engine);
            }
            Mark::Engine(_) => {}
            Mark::Sql => engines.extend(found.keys().copied().filter(|e| e.speaks_sql())),
        }
    }
    engines.into_iter().collect()
}
