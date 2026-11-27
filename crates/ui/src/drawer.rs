use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnimationExt as _, AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, RenderOnce, SharedString, Styled as _, Window, px,
};

use crate::motion;

// The width of a drawer, and of one that needs more room, as one with lines of code in it.
pub const WIDTH: f32 = 280.;
pub const WIDE: f32 = 360.;
// The gap between a drawer and the edges of the window it floats in.
pub const INSET: f32 = 8.;

// A panel floating over the right of a window, over what the window holds instead of beside
// it, so nothing moves when it opens or closes. The window's bar opens and closes it, and so
// does ctrl-b.
#[derive(IntoElement)]
pub struct Drawer {
    id: ElementId,
    width: f32,
    children: Vec<AnyElement>,
}

impl Drawer {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            width: WIDTH,
            children: Vec::new(),
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

impl ParentElement for Drawer {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Drawer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        // It slides in from the right edge as it opens.
        v_flex()
            .id(self.id.clone())
            .absolute()
            .top(px(INSET))
            .bottom(px(INSET))
            .right(px(INSET))
            .w(px(self.width))
            .occlude()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .shadow_lg()
            .children(self.children)
            .with_animation(self.id, motion::opening(), |drawer, delta| {
                drawer.right(px(INSET - 16. * (1. - delta))).opacity(delta)
            })
    }
}

// The room a header under a drawer leaves on its right, so what is there stays in sight.
pub fn room() -> gpui_kit::Pixels {
    room_of(WIDTH)
}

// The room a drawer this wide takes on the right of what it floats over.
pub fn room_of(width: f32) -> gpui_kit::Pixels {
    px(width + 2. * INSET)
}

// The tabs along the top of a drawer that holds more than one panel.
pub fn tabs() -> Div {
    h_flex().gap_1().px_2().py_1().flex_shrink_0()
}

// A tab of a drawer: its icon and name, pressed while its panel is the one shown.
pub fn tab(
    id: impl Into<ElementId>,
    glyph: Glyph,
    label: impl Into<SharedString>,
    shown: bool,
    f: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .icon(glyph)
        .label(label.into())
        .selected(shown)
        .on_click(move |_, window, cx| f(window, cx))
}
