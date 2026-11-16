use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, SharedString, Window};

pub fn error(message: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::error(message), cx);
}
