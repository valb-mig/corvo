mod agents;
mod assist;
mod attach;
mod frame;
mod home;
mod palette;
mod pane;
mod repos;
mod shell;

use corvo_domain::Workspaces;
use corvo_domain::keymap::Keymap;
use corvo_fs::JsonStore;
use gpui_kit::{
    AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowDecorations, WindowOptions, px,
    size,
};

use frame::Frame;

fn main() -> anyhow::Result<()> {
    display()?;
    let spaces = Workspaces::load(Box::new(JsonStore::config()?))?;

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            corvo_ui::theme::apply(cx);
            corvo_ui::keys::bind(&Keymap::default(), cx);

            let options = WindowOptions {
                app_id: Some("corvo".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("corvo".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_decorations: Some(WindowDecorations::Client),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(800.)),
                    cx,
                ))),
                ..Default::default()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Frame::new(spaces, window, cx))
            })
            .expect("failed to open window");
        });

    Ok(())
}

fn display() -> anyhow::Result<()> {
    let found = ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|var| std::env::var_os(var).is_some_and(|value| !value.is_empty()));
    if cfg!(target_os = "linux") && !found {
        anyhow::bail!("no display: set WAYLAND_DISPLAY or DISPLAY");
    }
    Ok(())
}
