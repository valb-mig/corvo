use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use corvo_domain::{Content, Doc, Lang, Workbench};
use corvo_lsp::Hub;
use corvo_ui::button::Button;
use corvo_ui::{editor, notice, tab, viewer};
use gpui_kit::component::{ActiveTheme as _, IconName, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use lsp_types::{Diagnostic, Position};

use crate::assist::Assist;
use crate::pane::overview::Overview;

const IDLE: Duration = Duration::from_secs(60);

pub enum Event {
    Active(PathBuf),
    Saved(PathBuf),
    // The last tab was closed; the code stays shown, empty.
    Emptied,
    // Put away, back to the home.
    Dismissed,
}

enum View {
    Text(Entity<editor::State>),
    Image,
    Binary(u64),
}

// The open files, each in a tab, over the home while shown; put away, they stay open for the
// next time.
pub struct Code {
    bench: Entity<Workbench>,
    hub: Entity<Hub>,
    home: Entity<Overview>,
    shown: bool,
    views: HashMap<PathBuf, View>,
    texts: HashMap<PathBuf, editor::Rope>,
    subs: HashMap<PathBuf, Vec<Subscription>>,
}

impl EventEmitter<Event> for Code {}

impl Code {
    pub fn new(bench: Entity<Workbench>, hub: Entity<Hub>, home: Entity<Overview>) -> Self {
        Self {
            bench,
            hub,
            home,
            shown: false,
            views: HashMap::new(),
            texts: HashMap::new(),
            subs: HashMap::new(),
        }
    }

    pub fn open(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.shown = true;
        match self.bench.update(cx, |bench, _| bench.open(path)) {
            Ok(Some(content)) => self.mount(path, content, window, cx),
            Ok(None) => {}
            Err(err) => notice::error(format!("{}: {err}", path.display()), window, cx),
        }
        self.focus(window, cx);
        self.announce(cx);
        cx.notify();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.shown {
            return;
        }
        let Some((path, state)) = self.active(cx) else {
            return;
        };
        let text = state.read(cx).value();
        match self.bench.update(cx, |bench, _| bench.save(&path, &text)) {
            Ok(()) => {
                self.hub.read(cx).save(&path);
                cx.emit(Event::Saved(path));
            }
            Err(err) => notice::error(format!("{}: {err}", path.display()), window, cx),
        }
        cx.notify();
    }

    pub fn diagnose(&mut self, path: &Path, items: Vec<Diagnostic>, cx: &mut Context<Self>) {
        if let Some(View::Text(state)) = self.views.get(path) {
            state.update(cx, |state, cx| editor::diagnose(state, items, cx));
        }
    }

    pub fn reveal(
        &mut self,
        path: &Path,
        at: Option<Position>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open(path, window, cx);
        if let (Some(at), Some(View::Text(state))) = (at, self.views.get(path)) {
            state.update(cx, |state, cx| state.set_cursor_position(at, window, cx));
        }
    }

    fn mount(
        &mut self,
        path: &Path,
        content: Content,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = match content {
            Content::Text(text) => View::Text(self.edit(path, text, window, cx)),
            Content::Image => View::Image,
            Content::Binary { size } => View::Binary(size),
        };
        self.views.insert(path.to_path_buf(), view);
    }

    fn edit(
        &mut self,
        path: &Path,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<editor::State> {
        let lang = self
            .bench
            .read(cx)
            .tabs()
            .get(path)
            .map_or(Lang::Plain, Doc::lang);
        let source = Rc::new(Assist::new(
            self.hub.clone(),
            cx.entity().downgrade(),
            path.to_path_buf(),
        ));
        let state = cx.new(|cx| editor::state(lang, text, Some(source), window, cx));
        let rope = state.read(cx).text().clone();
        self.hub
            .update(cx, |hub, _| hub.open(path, lang, &rope.to_string()));

        let key = path.to_path_buf();
        let touch = cx.subscribe(&state, move |this, _, event: &editor::Event, cx| {
            if matches!(event, editor::Event::Change) {
                this.touch(&key, cx);
            }
        });
        let key = path.to_path_buf();
        let sync = cx.subscribe(&state, move |this, _, event: &editor::Edits, cx| {
            this.sync(&key, event, cx);
        });

        self.texts.insert(path.to_path_buf(), rope);
        self.subs.insert(path.to_path_buf(), vec![touch, sync]);
        state
    }

    fn sync(&mut self, path: &Path, event: &editor::Edits, cx: &mut Context<Self>) {
        let Some((_, last)) = event.edits().last() else {
            return;
        };
        let last = last.clone();
        let prev = self
            .texts
            .insert(path.to_path_buf(), last.clone())
            .unwrap_or_default();
        let changes = editor::changes(&prev, event.edits());
        self.hub
            .update(cx, |hub, _| hub.change(path, changes, || last.to_string()));
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
        self.announce(cx);
        cx.notify();
    }

    // Back to the home, which reads its open folders and files again; the tabs stay.
    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shown = false;
        self.home.update(cx, |home, cx| home.refresh(window, cx));
        cx.emit(Event::Dismissed);
        cx.notify();
    }

    // Over the home again with the tabs left open, or with none.
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shown = true;
        self.focus(window, cx);
        self.announce(cx);
        cx.notify();
    }

    pub fn is_shown(&self) -> bool {
        self.shown
    }

    fn close(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.path_at(ix, cx) else {
            return;
        };
        self.bench.update(cx, |bench, _| bench.close(&path));
        self.hub.update(cx, |hub, _| hub.close(&path));
        self.reap(cx);
        self.views.remove(&path);
        self.texts.remove(&path);
        self.subs.remove(&path);
        if self.bench.read(cx).tabs().active().is_none() {
            cx.emit(Event::Emptied);
            cx.notify();
            return;
        }
        self.focus(window, cx);
        self.announce(cx);
        cx.notify();
    }

    fn reap(&self, cx: &mut Context<Self>) {
        let hub = self.hub.clone();
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(IDLE).await;
            hub.update(cx, |hub, _| hub.reap());
        })
        .detach();
    }

    fn announce(&self, cx: &mut Context<Self>) {
        if let Some(doc) = self.bench.read(cx).tabs().active() {
            cx.emit(Event::Active(doc.path().to_path_buf()));
        }
    }

    fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, state)) = self.active(cx) {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    fn active(&self, cx: &Context<Self>) -> Option<(PathBuf, Entity<editor::State>)> {
        let path = self.bench.read(cx).tabs().active()?.path().to_path_buf();
        match self.views.get(&path)? {
            View::Text(state) => Some((path, state.clone())),
            View::Image | View::Binary(_) => None,
        }
    }

    fn body(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let Some(doc) = self.bench.read(cx).tabs().active() else {
            return Some(empty(cx));
        };
        let path = doc.path();
        let body = match self.views.get(path)? {
            View::Text(state) => editor::view(state).into_any_element(),
            View::Image => viewer::Image::new(path).into_any_element(),
            View::Binary(size) => viewer::Binary::new(*size).into_any_element(),
        };
        Some(body)
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
        let body = self.body(cx);

        let pick = cx.entity().downgrade();
        let close = pick.clone();
        let dismiss = pick.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                h_flex()
                    .child(
                        div().flex_1().min_w_0().child(
                            tab::Bar::new("tabs", items)
                                .active(active)
                                .on_pick(move |ix, window, cx| {
                                    pick.update(cx, |this, cx| this.pick(ix, window, cx)).ok();
                                })
                                .on_close(move |ix, window, cx| {
                                    close.update(cx, |this, cx| this.close(ix, window, cx)).ok();
                                }),
                        ),
                    )
                    .child(Button::ghost("dismiss").icon(IconName::Close).on_click(
                        move |_, window, cx| {
                            dismiss.update(cx, |this, cx| this.dismiss(window, cx)).ok();
                        },
                    )),
            )
            .child(div().flex_1().min_h_0().children(body))
    }
}

// With no tab open, where the editor would be.
fn empty(cx: &App) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child("No files open. Pick one in the tree.")
        .into_any_element()
}
