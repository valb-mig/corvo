use std::rc::Rc;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

// The bar along the top of a window over the home: its icon and name on the left, its palette's
// field in the middle, and the way back to the home, by Esc or the ×, on the right.
#[derive(IntoElement)]
pub struct WindowBar {
    glyph: Glyph,
    title: SharedString,
    subtitle: Option<SharedString>,
    center: Option<AnyElement>,
    on_close: Option<Handler>,
}

impl WindowBar {
    pub fn new(glyph: Glyph, title: impl Into<SharedString>) -> Self {
        Self {
            glyph,
            title: title.into(),
            subtitle: None,
            center: None,
            on_close: None,
        }
    }

    // What sits in the middle of the bar.
    pub fn center(mut self, center: impl IntoElement) -> Self {
        self.center = Some(center.into_any_element());
        self
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
        // The sides take the same room, so the middle is the bar's middle.
        let left = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(Icon::new(self.glyph).small())
            .child(
                div()
                    .flex_shrink_0()
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
            }));
        let right = h_flex()
            .flex_1()
            .justify_end()
            .gap_2()
            .child(key("Esc", cx))
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
            );
        h_flex()
            .h(px(44.))
            .px_3()
            .gap_3()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .child(left)
            .children(self.center)
            .child(right)
    }
}

// The field that opens a window's palette: what it searches, and the key that opens it.
#[derive(IntoElement)]
pub struct PaletteField {
    placeholder: SharedString,
    on_click: Option<Handler>,
}

impl PaletteField {
    pub fn new(placeholder: impl Into<SharedString>) -> Self {
        Self {
            placeholder: placeholder.into(),
            on_click: None,
        }
    }

    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for PaletteField {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let click = self.on_click;
        h_flex()
            .id("palette-field")
            .w(px(420.))
            .flex_shrink(1.)
            .min_w(px(160.))
            .h(px(28.))
            .px_2()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .cursor_pointer()
            .hover(|field| field.border_color(theme.muted_foreground))
            .text_sm()
            .text_color(theme.muted_foreground)
            .on_click(move |_, window, cx| {
                if let Some(click) = &click {
                    click(window, cx);
                }
            })
            .child(Icon::new(Glyph::Search).xsmall())
            .child(div().flex_1().min_w_0().truncate().child(self.placeholder))
            .child(key("Ctrl K", cx))
    }
}

// A key, as a small framed label.
fn key(label: &'static str, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    div()
        .flex_shrink_0()
        .px_1()
        .rounded(px(4.))
        .border_1()
        .border_color(theme.border)
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(label)
}
