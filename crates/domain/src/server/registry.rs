use crate::Lang;

use super::Spec;

#[derive(Clone, Debug)]
pub struct Registry {
    specs: Vec<Spec>,
}

impl Registry {
    pub fn new(specs: Vec<Spec>) -> Self {
        Self { specs }
    }

    pub fn specs(&self) -> &[Spec] {
        &self.specs
    }

    pub fn find(&self, lang: Lang) -> Option<&Spec> {
        self.specs.iter().find(|spec| spec.serves(lang))
    }

    pub fn add(&mut self, spec: Spec) {
        self.specs.retain(|s| s.name() != spec.name());
        self.specs.insert(0, spec);
    }
}

impl Default for Registry {
    fn default() -> Self {
        let gradle = [
            "settings.gradle.kts",
            "settings.gradle",
            "build.gradle.kts",
            "build.gradle",
            "pom.xml",
        ];
        Self::new(vec![
            Spec::new("rust-analyzer", "rust-analyzer")
                .langs([Lang::Rust])
                .markers(["Cargo.toml"])
                .hint("sudo pacman -S rust-analyzer"),
            Spec::new("typescript", "tsc")
                .args(["--lsp", "--stdio"])
                .langs([Lang::JavaScript, Lang::TypeScript, Lang::Tsx])
                .markers(["package.json", "tsconfig.json", "jsconfig.json"])
                .hint("npm i -g typescript"),
            Spec::new("phpactor", "phpactor")
                .args(["language-server"])
                .langs([Lang::Php])
                .markers(["composer.json"])
                .hint("https://github.com/phpactor/phpactor/releases"),
            Spec::new("jdtls", "jdtls")
                .langs([Lang::Java])
                .markers(gradle)
                .hint("yay -S jdtls"),
            Spec::new("kotlin-lsp", "kotlin-lsp")
                .args(["--stdio"])
                .langs([Lang::Kotlin])
                .markers(gradle)
                .hint("https://github.com/Kotlin/kotlin-lsp/releases"),
            Spec::new("pyright", "pyright-langserver")
                .args(["--stdio"])
                .langs([Lang::Python])
                .markers(["pyproject.toml", "setup.py", "requirements.txt"])
                .hint("npm i -g pyright"),
            Spec::new("taplo", "taplo")
                .args(["lsp", "stdio"])
                .langs([Lang::Toml])
                .hint("sudo pacman -S taplo-cli"),
            Spec::new("json", "vscode-json-language-server")
                .args(["--stdio"])
                .langs([Lang::Json])
                .hint("npm i -g vscode-langservers-extracted"),
            Spec::new("html", "vscode-html-language-server")
                .args(["--stdio"])
                .langs([Lang::Html])
                .hint("npm i -g vscode-langservers-extracted"),
            Spec::new("css", "vscode-css-language-server")
                .args(["--stdio"])
                .langs([Lang::Css])
                .hint("npm i -g vscode-langservers-extracted"),
            Spec::new("bash", "bash-language-server")
                .args(["start"])
                .langs([Lang::Bash])
                .hint("npm i -g bash-language-server"),
            Spec::new("yaml", "yaml-language-server")
                .args(["--stdio"])
                .langs([Lang::Yaml])
                .hint("npm i -g yaml-language-server"),
        ])
    }
}
