use std::path::{Path, PathBuf};

use corvo_domain::Script;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

pub enum Event {
    // A script picked, to run in a terminal.
    Run(Script),
}

// The scripts of the project in front, under the file each comes from, as the status bar opens
// them; a click runs one in a terminal of its own.
#[derive(Default)]
pub struct Scripts {
    root: Option<PathBuf>,
    scripts: Vec<Script>,
}

impl EventEmitter<Event> for Scripts {}

impl Scripts {
    pub fn set(&mut self, root: PathBuf, scripts: Vec<Script>, cx: &mut Context<Self>) {
        self.root = Some(root);
        self.scripts = scripts;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.root = None;
        self.scripts.clear();
        cx.notify();
    }

    // The project the scripts are of.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn scripts(&self) -> &[Script] {
        &self.scripts
    }
}

impl Render for Scripts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut source = "";
        for (at, script) in self.scripts.iter().enumerate() {
            if script.source() != source {
                source = script.source();
                rows.push(
                    div()
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(source)
                        .into_any_element(),
                );
            }
            let picked = script.clone();
            rows.push(
                h_flex()
                    .id(("script", at))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.muted))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(Event::Run(picked.clone()));
                    }))
                    .child(
                        Icon::new(Glyph::Play)
                            .xsmall()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(theme.foreground)
                            .child(script.name().to_owned()),
                    )
                    .children(script.about().map(|about| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(about.to_owned())
                    }))
                    .into_any_element(),
            );
        }
        if rows.is_empty() {
            return v_flex().w(px(340.)).child(
                div()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("No scripts in this project."),
            );
        }
        v_flex().w(px(380.)).child(
            div()
                .id("script-list")
                .max_h(px(420.))
                .overflow_y_scroll()
                .children(rows),
        )
    }
}
