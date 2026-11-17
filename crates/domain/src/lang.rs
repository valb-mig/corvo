use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Bash,
    Css,
    Html,
    Java,
    JavaScript,
    Json,
    Kotlin,
    Markdown,
    Php,
    Python,
    Rust,
    Toml,
    Tsx,
    TypeScript,
    Yaml,
    Plain,
}

impl Lang {
    pub fn of(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase);

        match ext.as_deref() {
            Some("sh" | "bash" | "zsh") => Self::Bash,
            Some("css" | "scss") => Self::Css,
            Some("html" | "htm") => Self::Html,
            Some("java") => Self::Java,
            Some("js" | "mjs" | "cjs" | "jsx") => Self::JavaScript,
            Some("json" | "jsonc") => Self::Json,
            Some("kt" | "kts") => Self::Kotlin,
            Some("md" | "markdown") => Self::Markdown,
            Some("php" | "phtml") => Self::Php,
            Some("py" | "pyi") => Self::Python,
            Some("rs") => Self::Rust,
            Some("toml") => Self::Toml,
            Some("tsx") => Self::Tsx,
            Some("ts" | "mts" | "cts") => Self::TypeScript,
            Some("yml" | "yaml") => Self::Yaml,
            _ => Self::Plain,
        }
    }
}
