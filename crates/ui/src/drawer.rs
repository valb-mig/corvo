use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnimationExt as _, AnyElement, App, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, RenderOnce, Styled as _, Window, px,
};

use crate::motion;

// The width of a drawer.
pub const WIDTH: f32 = 280.;
// The gap between a drawer and the edges of the window it floats in.
pub const INSET: f32 = 8.;

// A panel floating over the right of a window, over what the window holds instead of beside
// it, so nothing moves when it opens or closes. The window's bar opens and closes it, and so
// does ctrl-b.
#[derive(IntoElement)]
pub struct Drawer {
    id: ElementId,
    children: Vec<AnyElement>,
}

impl Drawer {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            children: Vec::new(),
        }
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
            .w(px(WIDTH))
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
    px(WIDTH + 2. * INSET)
}
