use corvo_domain::Lang;
use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::{Context, Entity, IntoElement, Styled as _, Window};

pub use gpui_kit::component::input::InputEvent as Event;

pub type State = EditorState;

pub fn state(lang: Lang, text: String, window: &mut Window, cx: &mut Context<State>) -> State {
    EditorState::new(window, cx)
        .language(name(lang))
        .line_number(true)
        .folding(true)
        .default_value(text)
}

pub fn view(state: &Entity<State>) -> impl IntoElement + use<> {
    Editor::new(state).size_full().bordered(false)
}

fn name(lang: Lang) -> &'static str {
    match lang {
        Lang::Bash => "bash",
        Lang::Css => "css",
        Lang::Html => "html",
        Lang::JavaScript => "javascript",
        Lang::Json => "json",
        Lang::Markdown => "markdown",
        Lang::Python => "python",
        Lang::Rust => "rust",
        Lang::Toml => "toml",
        Lang::Tsx => "tsx",
        Lang::TypeScript => "typescript",
        Lang::Yaml => "yaml",
        Lang::Plain => "text",
    }
}
