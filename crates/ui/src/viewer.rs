use std::path::PathBuf;

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    App, IntoElement, ObjectFit, ParentElement as _, RenderOnce, Styled as _, StyledImage as _,
    Window, div, img,
};

#[derive(IntoElement)]
pub struct Image {
    path: PathBuf,
}

impl Image {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl RenderOnce for Image {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex().size_full().p_4().child(
            img(self.path)
                .size_full()
                .object_fit(ObjectFit::ScaleDown)
                .with_fallback(move || {
                    message("Image can't be loaded")
                        .text_color(muted)
                        .into_any_element()
                }),
        )
    }
}

#[derive(IntoElement)]
pub struct Binary {
    size: u64,
}

impl Binary {
    pub fn new(size: u64) -> Self {
        Self { size }
    }
}

impl RenderOnce for Binary {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        message(format!(
            "Binary file · {} · can't be shown",
            size(self.size)
        ))
        .text_color(cx.theme().muted_foreground)
    }
}

pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.;
    let mut unit = 0;
    while value >= 1024. && unit < UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn message(text: impl Into<gpui_kit::SharedString>) -> gpui_kit::Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .child(text.into())
}
