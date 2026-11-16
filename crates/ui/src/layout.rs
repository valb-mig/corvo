use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, RenderOnce, Window, px};

#[derive(IntoElement)]
pub struct Split {
    main: AnyElement,
    side: AnyElement,
}

impl Split {
    pub fn new(main: impl IntoElement, side: impl IntoElement) -> Self {
        Self {
            main: main.into_any_element(),
            side: side.into_any_element(),
        }
    }
}

impl RenderOnce for Split {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        h_resizable("split")
            .child(resizable_panel().child(self.main))
            .child(
                resizable_panel()
                    .size(px(280.))
                    .size_range(px(180.)..px(560.))
                    .fixed()
                    .child(self.side),
            )
    }
}
