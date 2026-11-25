use std::rc::Rc;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::Button;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    Anchor, AnyElement, AnyView, App, ElementId, IntoElement, ParentElement as _, RenderOnce,
    SharedString, Styled as _, Window,
};

use crate::button::{Ghost, Kind as _};

type Toggle = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

// The strip along the bottom of the window, its items from the left.
#[derive(IntoElement, Default)]
pub struct StatusBar {
    items: Vec<AnyElement>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn item(mut self, item: impl IntoElement) -> Self {
        self.items.push(item.into_any_element());
        self
    }
}

impl RenderOnce for StatusBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .flex_shrink_0()
            .h_7()
            .px_1()
            .gap_1()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .text_xs()
            .text_color(theme.muted_foreground)
            .children(self.items)
    }
}

// An item of the status bar that opens its content above it.
#[derive(IntoElement)]
pub struct Popup {
    id: ElementId,
    label: SharedString,
    glyph: Option<Glyph>,
    children: Vec<AnyElement>,
    content: AnyView,
    anchor: Anchor,
    open: bool,
    on_toggle: Option<Toggle>,
}

impl Popup {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        content: impl Into<AnyView>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            glyph: None,
            children: Vec::new(),
            content: content.into(),
            anchor: Anchor::BottomLeft,
            open: false,
            on_toggle: None,
        }
    }

    pub fn glyph(mut self, glyph: Glyph) -> Self {
        self.glyph = Some(glyph);
        self
    }

    // More to show in the item, after its label.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    // The corner of the content that meets the item: bottom left by default, bottom right for an
    // item at the right end of the bar.
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
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

impl RenderOnce for Popup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let content = self.content;
        let mut trigger = Button::new(self.id.clone());
        if !self.label.is_empty() {
            trigger = trigger.label(self.label);
        }
        if let Some(glyph) = self.glyph {
            trigger = trigger.icon(glyph);
        }
        let trigger = trigger.children(self.children);
        let mut popover = Popover::new(self.id)
            .anchor(self.anchor)
            .open(self.open)
            .trigger(Ghost.style(trigger))
            .content(move |_, _, _| content.clone());
        if let Some(f) = self.on_toggle {
            popover = popover.on_open_change(move |open, window, cx| f(*open, window, cx));
        }
        popover
    }
}

// An item of the status bar that does one thing, shown as its icon.
#[derive(IntoElement)]
pub struct Action {
    id: ElementId,
    glyph: Glyph,
    tip: SharedString,
    on_click: Option<Handler>,
}

impl Action {
    pub fn new(id: impl Into<ElementId>, glyph: Glyph, tip: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            glyph,
            tip: tip.into(),
            on_click: None,
        }
    }

    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Action {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut button = Ghost.style(Button::new(self.id).icon(self.glyph).tooltip(self.tip));
        if let Some(f) = self.on_click {
            button = button.on_click(move |_, window, cx| f(window, cx));
        }
        button
    }
}
