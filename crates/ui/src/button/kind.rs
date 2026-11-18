use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};

pub trait Kind {
    fn style(&self, button: Button) -> Button;
}

pub struct Ghost;

impl Kind for Ghost {
    fn style(&self, button: Button) -> Button {
        button.ghost().xsmall()
    }
}

pub struct Primary;

impl Kind for Primary {
    fn style(&self, button: Button) -> Button {
        button.primary()
    }
}
