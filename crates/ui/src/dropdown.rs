use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::popover::Popover;
use gpui_kit::{Anchor, AnyView, App, ElementId, IntoElement, RenderOnce, SharedString, Window};

use crate::button::{Ghost, Kind as _};

type Toggle = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Dropdown {
    id: ElementId,
    label: SharedString,
    content: AnyView,
    open: bool,
    on_toggle: Option<Toggle>,
}

impl Dropdown {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        content: impl Into<AnyView>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            content: content.into(),
            open: false,
            on_toggle: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Dropdown {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let content = self.content;
        let trigger = Ghost
            .style(Button::new(self.id.clone()).label(self.label))
            .dropdown_caret(true);
        let mut popover = Popover::new(self.id)
            .anchor(Anchor::TopLeft)
            .open(self.open)
            .trigger(trigger)
            .content(move |_, _, _| content.clone());
        if let Some(f) = self.on_toggle {
            popover = popover.on_open_change(move |open, window, cx| f(*open, window, cx));
        }
        popover
    }
}
