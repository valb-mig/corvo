use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{Anchor, App};

pub fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::global_mut(cx).notification.placement = Anchor::TopCenter;
}
