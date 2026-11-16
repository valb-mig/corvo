mod pane;
mod shell;

use std::path::PathBuf;

use corvo_domain::Workbench;
use corvo_domain::keymap::Keymap;
use corvo_fs::LocalFs;
use gpui_kit::{AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use shell::Shell;

fn main() -> anyhow::Result<()> {
    let bench = Workbench::new(root()?, Box::new(LocalFs))?;

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            corvo_ui::theme::apply(cx);
            corvo_ui::keys::bind(&Keymap::default(), cx);

            let options = WindowOptions {
                app_id: Some("corvo".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("corvo".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(800.)),
                    cx,
                ))),
                ..Default::default()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Shell::new(bench, window, cx))
            })
            .expect("failed to open window");
        });

    Ok(())
}

fn root() -> std::io::Result<PathBuf> {
    let path = match std::env::args_os().nth(1) {
        Some(arg) => PathBuf::from(arg),
        None => std::env::current_dir()?,
    };
    path.canonicalize()
}
