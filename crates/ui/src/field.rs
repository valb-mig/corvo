use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{Context, Entity, IntoElement, SharedString, Window};

pub use gpui_kit::component::input::InputEvent as Event;

pub type State = InputState;

pub fn state(
    placeholder: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut Context<State>,
) -> State {
    InputState::new(window, cx).placeholder(placeholder)
}

pub fn view(state: &Entity<State>) -> impl IntoElement + use<> {
    Input::new(state)
}
