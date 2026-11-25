use corvo_domain::Share;
use corvo_ui::language_icon::LanguageIcon;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div, px};

// The languages of the shown project, each with its share of the files, as the status bar
// opens them.
#[derive(Default)]
pub struct Stack {
    shares: Vec<Share>,
}

impl Stack {
    pub fn set(&mut self, shares: Vec<Share>, cx: &mut Context<Self>) {
        self.shares = shares;
        cx.notify();
    }
}

impl Render for Stack {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let total: usize = self.shares.iter().map(Share::files).sum();
        let mut list = v_flex().w(px(280.)).gap_1().text_sm();
        if self.shares.is_empty() {
            return list.child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(theme.muted_foreground)
                    .child("No code found in this project."),
            );
        }
        for share in &self.shares {
            let part = share.files() as f32 / total.max(1) as f32;
            let files = match share.files() {
                1 => "1 file".to_owned(),
                n => format!("{n} files"),
            };
            list = list.child(
                v_flex()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(LanguageIcon::new(share.name()).size(px(16.)))
                            .child(div().flex_1().child(share.name()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{files} · {:.0}%", part * 100.)),
                            ),
                    )
                    .child(
                        div()
                            .h(px(3.))
                            .w_full()
                            .rounded_full()
                            .bg(theme.border)
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui_kit::relative(part))
                                    .rounded_full()
                                    .bg(theme.muted_foreground),
                            ),
                    ),
            );
        }
        list
    }
}
