mod marks;
mod source;
mod sync;

use std::rc::Rc;

use corvo_domain::Lang;
use gpui_kit::component::highlighter::LanguageRegistry;
use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::{Context, Entity, IntoElement, Styled as _, Window};
use lsp_types::Diagnostic;

pub use gpui_kit::component::input::{InputEditsEvent as Edits, InputEvent as Event, Rope};
pub use marks::Marks;
pub use source::Source;
pub use sync::changes;

pub type State = EditorState;

pub fn state(
    lang: Lang,
    text: String,
    source: Option<Rc<dyn Source>>,
    window: &mut Window,
    cx: &mut Context<State>,
) -> State {
    // The search of the code is a bar of corvo's own over the editor, not the one it brings.
    let mut state = EditorState::new(window, cx)
        .language(highlighted_as(lang, text.len()))
        .line_number(true)
        .folding(true)
        .searchable(false)
        .default_value(text);
    state.set_soft_wrap(false, window, cx);
    if let Some(source) = source {
        source::attach(&mut state, source);
    }
    state
}

// A piece of a file on its own: no line numbers or folding, since its lines start mid-file.
pub fn snippet(lang: Lang, text: String, window: &mut Window, cx: &mut Context<State>) -> State {
    let mut state = EditorState::new(window, cx)
        .language(name(lang))
        .line_number(false)
        .folding(false)
        .default_value(text);
    state.set_soft_wrap(false, window, cx);
    state
}

pub fn view(state: &Entity<State>) -> impl IntoElement + use<> {
    Editor::new(state).size_full().bordered(false)
}

pub fn diagnose(state: &mut State, items: Vec<Diagnostic>, cx: &mut Context<State>) {
    let text = state.text().clone();
    if let Some(set) = state.diagnostics_mut() {
        set.reset(&text);
        set.extend(items);
    }
    cx.notify();
}

// Rust reads the tokens of every macro again as Rust (the injections of gpui-component's grammar),
// and its highlighter runs that query over the whole file, on the main thread, on each key typed:
// about 40% of a key in a file of 2300 lines. From this size on a file of Rust is highlighted
// without them, under a name of its own: inside a macro's brackets its strings and numbers keep
// their colours, its calls and types don't. The grammar, and what tells code from strings and
// comments as it is typed, are Rust's still.
const LIGHT_FROM: usize = 32 * 1024;
const RUST_LIGHT: &str = "rust-light";

// Registers what files past `LIGHT_FROM` are highlighted as; once, before any editor opens.
pub fn init() {
    let registry = LanguageRegistry::singleton();
    if let Some(mut rust) = registry.language("rust") {
        rust.name = RUST_LIGHT.into();
        rust.injections = Default::default();
        rust.injection_languages.clear();
        registry.register(RUST_LIGHT, &rust);
    }
}

// The language a file of `len` bytes is highlighted as.
fn highlighted_as(lang: Lang, len: usize) -> &'static str {
    match lang {
        Lang::Rust if len >= LIGHT_FROM => RUST_LIGHT,
        lang => name(lang),
    }
}

fn name(lang: Lang) -> &'static str {
    match lang {
        Lang::Bash => "bash",
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
        Lang::Tsx => "tsx",
        Lang::TypeScript => "typescript",
        Lang::Yaml => "yaml",
        Lang::Plain => "text",
    }
}

#[cfg(test)]
mod tests {
    use corvo_domain::Lang;

    use super::{LIGHT_FROM, RUST_LIGHT, highlighted_as};

    #[test]
    fn long_rust_is_highlighted_without_injections() {
        assert_eq!(highlighted_as(Lang::Rust, LIGHT_FROM - 1), "rust");
        assert_eq!(highlighted_as(Lang::Rust, LIGHT_FROM), RUST_LIGHT);
    }

    #[test]
    fn other_languages_keep_their_injections_whatever_their_length() {
        assert_eq!(highlighted_as(Lang::Php, 10 * LIGHT_FROM), "php");
        assert_eq!(highlighted_as(Lang::Markdown, 10 * LIGHT_FROM), "markdown");
    }
}
