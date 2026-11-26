use std::rc::Rc;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    App, FontWeight, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _,
    Window, div, px,
};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

// The bar along the top of a window over the home: its icon and name, and the way back to the
// home, by Esc or the ×.
#[derive(IntoElement)]
pub struct WindowBar {
    glyph: Glyph,
    title: SharedString,
    subtitle: Option<SharedString>,
    on_close: Option<Handler>,
}

impl WindowBar {
    pub fn new(glyph: Glyph, title: impl Into<SharedString>) -> Self {
        Self {
            glyph,
            title: title.into(),
            subtitle: None,
            on_close: None,
        }
    }

    // Muted words beside the name, about what the window holds.
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn on_close(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for WindowBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let close = self.on_close;
        h_flex()
            .h(px(44.))
            .px_3()
            .gap_2()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .child(Icon::new(self.glyph).small())
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.title),
            )
            .children(self.subtitle.map(|subtitle| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(subtitle)
            }))
            .child(div().flex_1())
            .child(
                div()
                    .px_1()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Esc"),
            )
            .child(
                Button::new("close-window")
                    .ghost()
                    .small()
                    .icon(Glyph::Close)
                    .on_click(move |_, window, cx| {
                        if let Some(close) = &close {
                            close(window, cx);
                        }
                    }),
            )
    }
}
