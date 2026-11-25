mod mention;
mod rules;
mod walk;

use std::ops::Range;

use corvo_domain::port::{Depth, Syntax};
use corvo_domain::{Lang, Symbol};
use tree_sitter::{Language, Parser};

use walk::{Locals, Rule, Scope, Walk};

#[derive(Clone, Copy, Debug, Default)]
pub struct TreeSitter;

impl Syntax for TreeSitter {
    fn outlines(&self, lang: Lang) -> bool {
        grammar(lang).is_some()
    }

    fn outline(&self, lang: Lang, text: &str, depth: Depth) -> Vec<Symbol> {
        let Some((language, rule, locals)) = grammar(lang) else {
            return Vec::new();
        };
        let mut parser = Parser::new();
        if parser.set_language(&language).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(text, None) else {
            return Vec::new();
        };
        let walk = match depth {
            Depth::Items => Walk::new(rule, text),
            Depth::Locals => Walk::new(rule, text).locals(locals),
        };
        walk.within(tree.root_node(), Scope::Top)
    }

    fn mentions(
        &self,
        lang: Lang,
        text: &str,
        name: &str,
        within: Range<usize>,
    ) -> Vec<Range<usize>> {
        let Some((language, _, _)) = grammar(lang) else {
            return Vec::new();
        };
        let mut parser = Parser::new();
        if parser.set_language(&language).is_err() {
            return Vec::new();
        }
        let Some(tree) = parser.parse(text, None) else {
            return Vec::new();
        };
        mention::find(tree.root_node(), text, name, within)
    }
}

fn grammar(lang: Lang) -> Option<(Language, Rule, Locals)> {
    let found: (Language, Rule, Locals) = match lang {
        Lang::Rust => (
            tree_sitter_rust::LANGUAGE.into(),
            rules::rust,
            rules::rust_local,
        ),
        Lang::JavaScript => (
            tree_sitter_javascript::LANGUAGE.into(),
            rules::script,
            rules::script_local,
        ),
        Lang::TypeScript => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            rules::script,
            rules::script_local,
        ),
        Lang::Tsx => (
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            rules::script,
            rules::script_local,
        ),
        Lang::Python => (
            tree_sitter_python::LANGUAGE.into(),
            rules::python,
            rules::python_local,
        ),
        Lang::Java => (
            tree_sitter_java::LANGUAGE.into(),
            rules::java,
            rules::java_local,
        ),
        Lang::Php => (
            tree_sitter_php::LANGUAGE_PHP.into(),
            rules::php,
            rules::php_local,
        ),
        Lang::Kotlin => (
            tree_sitter_kotlin_sg::LANGUAGE.into(),
            rules::kotlin,
            rules::kotlin_local,
        ),
        _ => return None,
    };
    Some(found)
}
