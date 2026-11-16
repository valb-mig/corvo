use gpui_kit::App;
use gpui_kit::component::{Theme, ThemeMode};

pub fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
}
