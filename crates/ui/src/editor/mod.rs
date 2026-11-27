mod marks;
mod source;
mod sync;

use std::rc::Rc;

use corvo_domain::Lang;
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
        .language(name(lang))
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
