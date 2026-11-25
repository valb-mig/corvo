use std::collections::HashMap;
use std::path::Path;

use crate::port::Fs;
use crate::scan;

// The languages a project is written in, by how many of its files each takes. Data, docs and
// settings (JSON, YAML, TOML, Markdown) are not counted as languages.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Share {
    name: &'static str,
    files: usize,
}

impl Share {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn files(&self) -> usize {
        self.files
    }
}

// Extensions and name.
const LANGUAGES: &[(&[&str], &str)] = &[
    (&["rs"], "Rust"),
    (&["ts", "tsx", "mts", "cts"], "TypeScript"),
    (&["js", "jsx", "mjs", "cjs"], "JavaScript"),
    (&["py", "pyi"], "Python"),
    (&["java"], "Java"),
    (&["kt", "kts"], "Kotlin"),
    (&["php"], "PHP"),
    (&["go"], "Go"),
    (&["rb"], "Ruby"),
    (&["swift"], "Swift"),
    (&["dart"], "Dart"),
    (&["c", "h"], "C"),
    (&["cpp", "cc", "cxx", "hpp", "hh", "hxx"], "C++"),
    (&["cs"], "C#"),
    (&["vue"], "Vue"),
    (&["svelte"], "Svelte"),
    (&["astro"], "Astro"),
    (&["html", "htm"], "HTML"),
    (&["css", "scss", "sass", "less"], "CSS"),
    (&["sh", "bash", "zsh", "fish"], "Shell"),
    (&["lua"], "Lua"),
    (&["sql"], "SQL"),
    (&["ex", "exs"], "Elixir"),
    (&["zig"], "Zig"),
    (&["scala"], "Scala"),
    (&["hs"], "Haskell"),
];

// Most files first; a tie goes by name.
pub fn languages(fs: &dyn Fs, root: &Path) -> Vec<Share> {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    scan::files(fs, root, |node| {
        let Some((_, ext)) = node.name().rsplit_once('.') else {
            return;
        };
        let ext = ext.to_ascii_lowercase();
        if let Some(ix) = LANGUAGES
            .iter()
            .position(|(exts, _)| exts.contains(&ext.as_str()))
        {
            *counts.entry(ix).or_default() += 1;
        }
    });
    let mut shares: Vec<Share> = counts
        .into_iter()
        .map(|(ix, files)| Share {
            name: LANGUAGES[ix].1,
            files,
        })
        .collect();
    shares.sort_by(|a, b| b.files.cmp(&a.files).then(a.name.cmp(b.name)));
    shares
}
