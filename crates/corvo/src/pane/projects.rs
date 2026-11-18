use std::path::{Path, PathBuf};

use corvo_domain::{Workbench, Workspaces};
use corvo_ui::button::Button;
use corvo_ui::list::Row;
use corvo_ui::notice;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    Context, Entity, EventEmitter, IntoElement, ParentElement as _, PathPromptOptions, Render,
    Styled as _, Window, div, px,
};

pub enum Event {
    Show(PathBuf),
    Linked(PathBuf),
    Unlinked(PathBuf),
}

pub struct Projects {
    spaces: Entity<Workspaces>,
    id: u64,
    bench: Entity<Workbench>,
}

impl EventEmitter<Event> for Projects {}

impl Projects {
    pub fn new(spaces: Entity<Workspaces>, id: u64, bench: Entity<Workbench>) -> Self {
        Self { spaces, id, bench }
    }

    fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Link".into()),
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = picked.await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.link(path, window, cx);
                    }
                }
                Ok(Ok(None)) | Err(_) => {}
                Ok(Err(err)) => notice::error(err.to_string(), window, cx),
            })
            .ok();
        })
        .detach();
    }

    fn link(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.id;
        match self.spaces.update(cx, |spaces, _| spaces.link(id, &path)) {
            Ok(()) => cx.emit(Event::Linked(path)),
            Err(err) => notice::error(err.to_string(), window, cx),
        }
        cx.notify();
    }

    fn unlink(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.id;
        match self.spaces.update(cx, |spaces, _| spaces.unlink(id, path)) {
            Ok(()) => cx.emit(Event::Unlinked(path.to_path_buf())),
            Err(err) => notice::error(err.to_string(), window, cx),
        }
        cx.notify();
    }
}

impl Render for Projects {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let active = self.bench.read(cx).project().map(Path::to_path_buf);
        let projects = self
            .spaces
            .read(cx)
            .get(self.id)
            .map(|space| space.projects().to_vec())
            .unwrap_or_default();

        let empty = projects.is_empty();
        let rows = projects.into_iter().map(|path| {
            let name = path
                .file_name()
                .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
                .into_owned();
            let show = view.clone();
            let remove = view.clone();
            let shown = path.clone();
            let removed = path.clone();
            Row::new(path.to_string_lossy().into_owned(), name)
                .selected(active.as_ref() == Some(&path))
                .on_click(move |_, cx| {
                    let path = shown.clone();
                    show.update(cx, |_, cx| cx.emit(Event::Show(path))).ok();
                })
                .on_remove(move |window, cx| {
                    remove
                        .update(cx, |this, cx| this.unlink(&removed, window, cx))
                        .ok();
                })
        });
        let add = view.clone();

        v_flex()
            .w(px(280.))
            .gap_1()
            .text_sm()
            .children(empty.then(|| {
                div()
                    .px(px(8.))
                    .py_1()
                    .text_color(cx.theme().muted_foreground)
                    .child("No projects linked")
            }))
            .children(rows)
            .child(
                Button::ghost("link")
                    .label("Link folder…")
                    .on_click(move |_, window, cx| {
                        add.update(cx, |this, cx| this.browse(window, cx)).ok();
                    }),
            )
    }
}
