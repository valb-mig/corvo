use std::collections::HashMap;
use std::path::{Path, PathBuf};

use corvo_domain::{Doc, Lang, Workbench};
use corvo_ui::{editor, notice, tab};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Window, div,
};

pub struct Code {
    bench: Entity<Workbench>,
    editors: HashMap<PathBuf, Entity<editor::State>>,
    subs: HashMap<PathBuf, Subscription>,
}

impl Code {
    pub fn new(bench: Entity<Workbench>) -> Self {
        Self {
            bench,
            editors: HashMap::new(),
            subs: HashMap::new(),
        }
    }

    pub fn open(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match self.bench.update(cx, |bench, _| bench.open(path)) {
            Ok(Some(text)) => self.mount(path, text, window, cx),
            Ok(None) => {}
            Err(err) => notice::error(format!("{}: {err}", path.display()), window, cx),
        }
        self.focus(window, cx);
        cx.notify();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, state)) = self.active(cx) else {
            return;
        };
        let text = state.read(cx).value();
        if let Err(err) = self.bench.update(cx, |bench, _| bench.save(&path, &text)) {
            notice::error(format!("{}: {err}", path.display()), window, cx);
        }
        cx.notify();
    }

    fn mount(&mut self, path: &Path, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let lang = self
            .bench
            .read(cx)
            .tabs()
            .get(path)
            .map_or(Lang::Plain, Doc::lang);
        let state = cx.new(|cx| editor::state(lang, text, window, cx));

        let key = path.to_path_buf();
        let sub = cx.subscribe(&state, move |this, _, event: &editor::Event, cx| {
            if matches!(event, editor::Event::Change) {
                this.touch(&key, cx);
            }
        });

        self.editors.insert(path.to_path_buf(), state);
        self.subs.insert(path.to_path_buf(), sub);
    }

    fn touch(&mut self, path: &Path, cx: &mut Context<Self>) {
        let dirty = self
            .bench
            .read(cx)
            .tabs()
            .get(path)
            .is_some_and(Doc::is_dirty);
        if !dirty {
            self.bench.update(cx, |bench, _| bench.touch(path));
            cx.notify();
        }
    }

    fn pick(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.path_at(ix, cx) else {
            return;
        };
        self.bench.update(cx, |bench, _| bench.focus(&path));
        self.focus(window, cx);
        cx.notify();
    }

    fn close(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.path_at(ix, cx) else {
            return;
        };
        self.bench.update(cx, |bench, _| bench.close(&path));
        self.editors.remove(&path);
        self.subs.remove(&path);
        self.focus(window, cx);
        cx.notify();
    }

    fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, state)) = self.active(cx) {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    fn active(&self, cx: &Context<Self>) -> Option<(PathBuf, Entity<editor::State>)> {
        let path = self.bench.read(cx).tabs().active()?.path().to_path_buf();
        let state = self.editors.get(&path)?.clone();
        Some((path, state))
    }

    fn path_at(&self, ix: usize, cx: &Context<Self>) -> Option<PathBuf> {
        let docs = self.bench.read(cx).tabs().docs();
        docs.get(ix).map(|doc| doc.path().to_path_buf())
    }
}

impl Render for Code {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = self.bench.read(cx).tabs();
        let items = tabs
            .docs()
            .iter()
            .map(|doc| {
                tab::Item::new(
                    doc.path().to_string_lossy().into_owned(),
                    doc.name().to_owned(),
                    doc.is_dirty(),
                )
            })
            .collect();
        let active = tabs.active_ix();
        let editor = self.active(cx).map(|(_, state)| editor::view(&state));

        let pick = cx.entity().downgrade();
        let close = pick.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                tab::Bar::new("tabs", items)
                    .active(active)
                    .on_pick(move |ix, window, cx| {
                        pick.update(cx, |this, cx| this.pick(ix, window, cx)).ok();
                    })
                    .on_close(move |ix, window, cx| {
                        close.update(cx, |this, cx| this.close(ix, window, cx)).ok();
                    }),
            )
            .child(div().flex_1().min_h_0().children(editor))
    }
}
