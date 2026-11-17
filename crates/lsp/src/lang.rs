use corvo_domain::Lang;

pub fn id(lang: Lang) -> &'static str {
    match lang {
        Lang::Bash => "shellscript",
        Lang::Css => "css",
        Lang::Html => "html",
        Lang::Java => "java",
        Lang::JavaScript => "javascript",
        Lang::Json => "json",
        Lang::Kotlin => "kotlin",
        Lang::Markdown => "markdown",
        Lang::Php => "php",
        Lang::Python => "python",
        Lang::Rust => "rust",
        Lang::Toml => "toml",
        Lang::Tsx => "typescriptreact",
        Lang::TypeScript => "typescript",
        Lang::Yaml => "yaml",
        Lang::Plain => "plaintext",
    }
}
