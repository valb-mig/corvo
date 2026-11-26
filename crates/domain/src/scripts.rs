use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use serde::Deserialize;
use serde::de::{MapAccess, Visitor};

use crate::port::Fs;

// The scripts a project offers to run in a terminal, from the files that keep them at its root:
// the recipes of a justfile, the scripts of package.json and composer.json, the targets of a
// Makefile. Each comes as the command that runs it.

const SIZE: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    name: String,
    command: String,
    source: &'static str,
    detail: Option<String>,
    asks: bool,
}

impl Script {
    fn new(name: impl Into<String>, command: String, source: &'static str) -> Self {
        Self {
            name: name.into(),
            command,
            source,
            detail: None,
            asks: false,
        }
    }

    fn detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail.filter(|detail| !detail.is_empty());
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    // What runs it, as typed in a shell.
    pub fn command(&self) -> &str {
        &self.command
    }

    // The file it comes from: `justfile`, `package.json`, `composer.json` or `Makefile`.
    pub fn source(&self) -> &'static str {
        self.source
    }

    // What it does: its description, or what it runs.
    pub fn about(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    // It needs arguments the dev types after the command, so the command is not run on its own.
    pub fn asks(&self) -> bool {
        self.asks
    }
}

// The scripts at the root of a project, a file at a time: justfile, package.json, composer.json,
// Makefile, each in the order the file has them.
pub fn scripts(fs: &dyn Fs, root: &Path) -> Vec<Script> {
    let mut found = Vec::new();
    if let Some(text) = first(fs, root, &["justfile", "Justfile", ".justfile"]) {
        found.extend(just(&text));
    }
    if let Some(text) = read(fs, &root.join("package.json")) {
        let manager = manager(fs, root, &text);
        found.extend(package(&text, manager));
    }
    if let Some(text) = read(fs, &root.join("composer.json")) {
        found.extend(composer(&text));
    }
    if let Some(text) = first(fs, root, &["GNUmakefile", "makefile", "Makefile"]) {
        found.extend(make(&text));
    }
    found
}

fn first(fs: &dyn Fs, root: &Path, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| read(fs, &root.join(name)))
}

fn read(fs: &dyn Fs, path: &Path) -> Option<String> {
    let bytes = fs.read(path).ok()?;
    if bytes.len() > SIZE {
        return None;
    }
    String::from_utf8(bytes).ok()
}

// A name as a shell takes it: as it is when it holds nothing the shell reads, in quotes otherwise.
fn quote(name: &str) -> String {
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.:/@+=,%".contains(c));
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', r"'\''"))
    }
}

// The recipes of a justfile, but the private ones: those named with a leading `_` or marked
// `[private]`. The comment right above a recipe tells what it does, else its first line.
fn just(text: &str) -> Vec<Script> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut private = false;
    let mut doc: Option<String> = None;
    for (at, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if line.starts_with([' ', '\t']) || trimmed.is_empty() {
            if trimmed.is_empty() {
                private = false;
                doc = None;
            }
            continue;
        }
        if let Some(comment) = trimmed.strip_prefix('#') {
            doc = Some(comment.trim().to_owned());
            continue;
        }
        if trimmed.starts_with('[') {
            private |= trimmed
                .trim_matches(['[', ']'])
                .split(',')
                .any(|attr| attr.trim() == "private");
            continue;
        }
        let recipe = recipe(trimmed);
        let (is_private, about) = (private, doc.take());
        private = false;
        let Some((name, params)) = recipe else {
            continue;
        };
        if is_private || name.starts_with('_') {
            continue;
        }
        let asks = params.iter().any(|param| asked(param));
        let body = lines[at + 1..]
            .iter()
            .take_while(|line| line.starts_with([' ', '\t']))
            .map(|line| line.trim().trim_start_matches(['@', '-']).trim())
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned);
        let mut command = format!("just {}", quote(name));
        if asks {
            command.push(' ');
        }
        let mut script = Script::new(name, command, "justfile").detail(about.or(body));
        script.asks = asks;
        found.push(script);
    }
    found
}

// A recipe's name and its parameters, from the line that opens it; none for a setting, a
// variable, an alias, an import or a module.
fn recipe(line: &str) -> Option<(&str, Vec<&str>)> {
    const WORDS: &[&str] = &["set", "alias", "export", "import", "mod", "unexport"];
    let line = line.strip_prefix('@').unwrap_or(line);
    let colon = colon(line)?;
    let head = &line[..colon];
    let mut words = head.split_whitespace();
    let name = words.next()?;
    if WORDS.contains(&name)
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((name, params(&head[name.len()..])))
}

// Where the colon that ends a recipe's head is, outside quotes; none when it starts `:=`, as in
// a variable, a setting or an alias.
fn colon(line: &str) -> Option<usize> {
    let mut quote = None;
    for (at, c) in line.char_indices() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, ':') => {
                return (!line[at + 1..].starts_with('=')).then_some(at);
            }
            _ => {}
        }
    }
    None
}

// The parameters of a recipe, each with its default when it has one, quotes kept whole.
fn params(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = None;
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if let Some(from) = start.take() {
                    found.push(&text[from..at]);
                }
            }
            _ => {
                start.get_or_insert(at);
            }
        }
    }
    if let Some(from) = start {
        found.push(&text[from..]);
    }
    found
}

// A parameter the dev must give: one with no default, `+` taking one or more; `*` takes none.
fn asked(param: &str) -> bool {
    let param = param.trim_start_matches('$');
    !param.starts_with('*') && !param.contains('=')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Manager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl Manager {
    fn command(self) -> &'static str {
        match self {
            Manager::Npm => "npm",
            Manager::Pnpm => "pnpm",
            Manager::Yarn => "yarn",
            Manager::Bun => "bun",
        }
    }
}

// The package manager of a Node project: by its lock file, then by the `packageManager` it
// names, npm otherwise.
fn manager(fs: &dyn Fs, root: &Path, text: &str) -> Manager {
    const LOCKS: &[(&str, Manager)] = &[
        ("pnpm-lock.yaml", Manager::Pnpm),
        ("yarn.lock", Manager::Yarn),
        ("bun.lock", Manager::Bun),
        ("bun.lockb", Manager::Bun),
        ("package-lock.json", Manager::Npm),
    ];
    if let Some(&(_, manager)) = LOCKS
        .iter()
        .find(|(lock, _)| fs.read(&root.join(lock)).is_ok())
    {
        return manager;
    }
    let named = serde_json::from_str::<Manifest>(text)
        .ok()
        .and_then(|manifest| manifest.manager);
    match named.as_deref().and_then(|named| named.split('@').next()) {
        Some("pnpm") => Manager::Pnpm,
        Some("yarn") => Manager::Yarn,
        Some("bun") => Manager::Bun,
        _ => Manager::Npm,
    }
}

// The scripts of package.json, run with the project's package manager. A `pre` or `post` hook
// of another script runs with it, so it is not listed.
fn package(text: &str, manager: Manager) -> Vec<Script> {
    let Ok(manifest) = serde_json::from_str::<Manifest>(text) else {
        return Vec::new();
    };
    let names: Vec<&str> = manifest.scripts.0.iter().map(|(n, _)| n.as_str()).collect();
    let hook = |name: &str| {
        ["pre", "post"].iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| names.contains(&rest))
        })
    };
    manifest
        .scripts
        .0
        .iter()
        .filter(|(name, _)| !hook(name))
        .map(|(name, body)| {
            let command = format!("{} run {}", manager.command(), quote(name));
            Script::new(name, command, "package.json").detail(body.as_str().map(str::to_owned))
        })
        .collect()
}

// The scripts of composer.json, but the ones composer runs on its own events (`post-install-cmd`
// and the like). Its `scripts-descriptions` tell what they do, else what they run.
fn composer(text: &str) -> Vec<Script> {
    let Ok(manifest) = serde_json::from_str::<Manifest>(text) else {
        return Vec::new();
    };
    manifest
        .scripts
        .0
        .iter()
        .filter(|(name, _)| !(name.starts_with("pre-") || name.starts_with("post-")))
        .map(|(name, body)| {
            let runs = match body {
                serde_json::Value::String(line) => Some(line.clone()),
                serde_json::Value::Array(lines) => Some(
                    lines
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" && "),
                ),
                _ => None,
            };
            let about = manifest.descriptions.get(name).cloned().or(runs);
            let command = format!("composer run {}", quote(name));
            Script::new(name, command, "composer.json").detail(about)
        })
        .collect()
}

// The targets of a Makefile, but special ones (`.PHONY`), patterns and variables. A comment
// after `##` on a target's line tells what it does.
fn make(text: &str) -> Vec<Script> {
    let mut found: Vec<Script> = Vec::new();
    for line in text.lines() {
        if line.starts_with([' ', '\t', '#']) {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        let (head, rest) = (&line[..colon], &line[colon + 1..]);
        if head.contains('=') || rest.starts_with([':', '=']) {
            continue;
        }
        let about = rest
            .split_once("##")
            .map(|(_, about)| about.trim().to_owned());
        for name in head.split_whitespace() {
            let target = !name.starts_with('.')
                && !name.contains(['%', '$', '(', ')'])
                && !found.iter().any(|script| script.name == name);
            if target {
                let command = format!("make {}", quote(name));
                found.push(Script::new(name, command, "Makefile").detail(about.clone()));
            }
        }
    }
    found
}

#[derive(Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    scripts: Entries,
    #[serde(default, rename = "scripts-descriptions")]
    descriptions: HashMap<String, String>,
    #[serde(default, rename = "packageManager")]
    manager: Option<String>,
}

// The entries of a JSON object in the order the file has them.
#[derive(Default)]
struct Entries(Vec<(String, serde_json::Value)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Ordered;

        impl<'de> Visitor<'de> for Ordered {
            type Value = Entries;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Entries, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, serde_json::Value>()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }

        deserializer.deserialize_map(Ordered)
    }
}
