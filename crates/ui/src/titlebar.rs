use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, IconName, h_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, MouseButton, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use crate::button::Button;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

// The bar on top of the window: what it holds on the left, its title in the middle, and the
// window's buttons on the right.
#[derive(IntoElement)]
pub struct TitleBar {
    children: Vec<AnyElement>,
    title: Option<SharedString>,
    on_close: Option<Handler>,
}

impl TitleBar {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            title: None,
            on_close: None,
        }
    }

    // The words in the middle of the bar.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn on_close(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
}

impl Default for TitleBar {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for TitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let controls = window.window_controls();
        let pressed = window.use_keyed_state("titlebar:pressed", cx, |_, _| false);
        let down = pressed.clone();
        let up = pressed.clone();
        let close = self.on_close;

        // The sides take the same room, so the title sits in the window's middle.
        let mut buttons = h_flex().flex_1().h_full().justify_end().gap_1().px_2();
        if controls.minimize {
            buttons = buttons.child(
                Button::ghost("window:minimize")
                    .icon(IconName::WindowMinimize)
                    .on_click(|_, window, _| window.minimize_window()),
            );
        }
        if controls.maximize {
            let icon = if window.is_maximized() {
                IconName::WindowRestore
            } else {
                IconName::WindowMaximize
            };
            buttons = buttons.child(
                Button::ghost("window:maximize")
                    .icon(icon)
                    .on_click(|_, window, _| window.zoom_window()),
            );
        }
        buttons = buttons.child(
            Button::ghost("window:close")
                .icon(IconName::WindowClose)
                .on_click(move |_, window, cx| match &close {
                    Some(f) => f(window, cx),
                    None => window.remove_window(),
                }),
        );

        h_flex()
            .id("titlebar")
            .flex_shrink_0()
            .w_full()
            .h(px(34.))
            .justify_between()
            .bg(cx.theme().background)
            .border_b_1()
            .border_color(cx.theme().border)
            .text_sm()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                down.update(cx, |pressed, _| *pressed = true);
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                up.update(cx, |pressed, _| *pressed = false);
            })
            .on_mouse_move(move |_, window, cx| {
                if *pressed.read(cx) {
                    pressed.update(cx, |pressed, _| *pressed = false);
                    window.start_window_move();
                }
            })
            .on_click(move |event, window, _| {
                if controls.maximize && event.click_count() == 2 {
                    window.zoom_window();
                }
            })
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .gap_2()
                    .px_2()
                    .children(self.children),
            )
            .children(self.title.map(|title| {
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(title)
            }))
            .child(buttons)
    }
}
