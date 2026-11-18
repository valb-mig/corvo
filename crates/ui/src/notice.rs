use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, ClipboardItem, SharedString, Window};

use crate::button::{Kind as _, Primary};

pub fn error(message: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::error(message), cx);
}

pub fn warn(message: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::warning(message), cx);
}

pub fn warn_with_copy(
    message: impl Into<SharedString>,
    label: impl Into<SharedString>,
    text: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) {
    let label = label.into();
    let text = text.into();
    let notification = Notification::warning(message).action(move |_, _, cx| {
        let text = text.clone();
        let this = cx.entity().downgrade();
        Primary
            .style(Button::new("copy").label(label.clone()))
            .on_click(move |_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                this.update(cx, |this, cx| this.dismiss(window, cx)).ok();
                window.push_notification(Notification::info("Copied"), cx);
            })
    });
    window.push_notification(notification, cx);
}
