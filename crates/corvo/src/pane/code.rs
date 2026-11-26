use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use corvo_domain::{Change, Content, Doc, Hunk, HunkMark, Lang, Workbench, hunks};
use corvo_lsp::Hub;
use corvo_ui::{editor, notice, tab, viewer};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Task, Window, div,
};
use lsp_types::{Diagnostic, Position};

use crate::assist::Assist;
use crate::pane::overview::Overview;
use crate::repos::Repos;

const IDLE: Duration = Duration::from_secs(60);
// How long typing rests before the changes against the last commit are marked again.
const REMARK: Duration = Duration::from_millis(150);

pub enum Event {
    Active(PathBuf),
    Saved(PathBuf),
    // The cursor of the active file went to another line (0-based).
    Cursor(PathBuf, u32),
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

// The changes marked in a file's editor, against the file as the commit it was read at has it.
struct Marked {
    marks: editor::Marks,
    head: Option<String>,
    // The file in that commit, once read; none when it is not there.
    base: Option<String>,
    read: bool,
    // The next marking, a moment after the last keystroke.
    _soon: Option<Task<()>>,
}

// The open files, each in a tab, over the home while shown; put away, they stay open for the
// next time.
pub struct Code {
    bench: Entity<Workbench>,
    hub: Entity<Hub>,
    home: Entity<Overview>,
    repos: Entity<Repos>,
    shown: bool,
    views: HashMap<PathBuf, View>,
    texts: HashMap<PathBuf, editor::Rope>,
    subs: HashMap<PathBuf, Vec<Subscription>>,
    marked: HashMap<PathBuf, Marked>,
    // The line each file's cursor was last seen on.
    lines: HashMap<PathBuf, u32>,
    _repos: Subscription,
}

impl EventEmitter<Event> for Code {}

impl Code {
    pub fn new(
        bench: Entity<Workbench>,
        hub: Entity<Hub>,
        home: Entity<Overview>,
        repos: Entity<Repos>,
        cx: &mut Context<Self>,
    ) -> Self {
        // A new commit, or a file that came into git or left it, marks the open files anew.
        let watch = cx.observe(&repos, |this, _, cx| this.rebase_all(cx));
        Self {
            bench,
            hub,
            home,
            repos,
            shown: false,
            views: HashMap::new(),
            texts: HashMap::new(),
            subs: HashMap::new(),
            marked: HashMap::new(),
            lines: HashMap::new(),
            _repos: watch,
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
        let text = matches!(view, View::Text(_));
        self.views.insert(path.to_path_buf(), view);
        if text {
            self.rebase(path, cx);
        }
    }

    fn rebase_all(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<PathBuf> = self
            .views
            .iter()
            .filter(|(_, view)| matches!(view, View::Text(_)))
            .map(|(path, _)| path.clone())
            .collect();
        for path in paths {
            self.rebase(&path, cx);
            self.remark(&path, cx);
        }
    }

    // Reads the file as the last commit has it, unless it was read at that commit already.
    fn rebase(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(View::Text(state)) = self.views.get(path) else {
            return;
        };
        let repos = self.repos.read(cx);
        let Some(repo) = repos.repo_of(path) else {
            // Out of every repository, or out of git now: nothing to mark.
            if let Some(marked) = self.marked.remove(path) {
                marked.marks.clear(cx);
            }
            return;
        };
        let head = repo.head().map(str::to_owned);
        let root = repo.root().to_path_buf();
        let git = repos.git();
        if self
            .marked
            .get(path)
            .is_some_and(|marked| marked.head == head)
        {
            return;
        }
        let state = state.clone();
        let marked = self
            .marked
            .entry(path.to_path_buf())
            .or_insert_with(|| Marked {
                marks: state.update(cx, editor::Marks::new),
                head: None,
                base: None,
                read: false,
                _soon: None,
            });
        marked.head = head.clone();
        marked.read = false;
        let file = path.to_path_buf();
        let task = cx
            .background_executor()
            .spawn(async move { git.base(&root, &file).ok().flatten() });
        let key = path.to_path_buf();
        cx.spawn(async move |this, cx| {
            let base = task.await;
            this.update(cx, |this, cx| {
                // A newer commit came while this one was read.
                let Some(marked) = this.marked.get_mut(&key).filter(|m| m.head == head) else {
                    return;
                };
                marked.base = base;
                marked.read = true;
                this.remark(&key, cx);
            })
            .ok();
        })
        .detach();
    }

    fn remark_soon(&mut self, path: &Path, cx: &mut Context<Self>) {
        let key = path.to_path_buf();
        let soon = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REMARK).await;
            this.update(cx, |this, cx| this.remark(&key, cx)).ok();
        });
        if let Some(marked) = self.marked.get_mut(path) {
            marked._soon = Some(soon);
        }
    }

    // Marks what the text in the editor changed from the last commit. A file the commit does
    // not have is all new, and only its bars show; one git leaves out has none.
    fn remark(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(View::Text(state)) = self.views.get(path) else {
            return;
        };
        let Some(marked) = self.marked.get(path).filter(|marked| marked.read) else {
            return;
        };
        let text = state.read(cx).text().to_string();
        let change = self.repos.read(cx).change_of(path);
        let found = match (&marked.base, change) {
            (Some(base), _) => hunks(base, &text),
            (None, Some(Change::Added | Change::Untracked)) => vec![Hunk {
                mark: HunkMark::Added,
                lines: 0..text.lines().count(),
            }],
            (None, _) => Vec::new(),
        };
        marked.marks.set(&text, &found, marked.base.is_some(), cx);
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
                this.remark_soon(&key, cx);
            }
        });
        let key = path.to_path_buf();
        let sync = cx.subscribe(&state, move |this, _, event: &editor::Edits, cx| {
            this.sync(&key, event, cx);
        });

        // The editor is told of every change of it, the cursor's blink too: only a new line
        // is told on.
        let key = path.to_path_buf();
        let follow = cx.observe(&state, move |this, state, cx| {
            let line = state.read(cx).cursor_position().line;
            if this.lines.insert(key.clone(), line) == Some(line) {
                return;
            }
            let tabs = this.bench.read(cx).tabs();
            if tabs.active().is_some_and(|doc| doc.path() == key) {
                cx.emit(Event::Cursor(key.clone(), line));
            }
        });

        self.texts.insert(path.to_path_buf(), rope);
        self.subs
            .insert(path.to_path_buf(), vec![touch, sync, follow]);
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

    // The line (0-based) of a file's cursor, when it is open as text.
    pub fn line(&self, path: &Path, cx: &App) -> Option<u32> {
        match self.views.get(path)? {
            View::Text(state) => Some(state.read(cx).cursor_position().line),
            View::Image | View::Binary(_) => None,
        }
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
        self.marked.remove(&path);
        self.lines.remove(&path);
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

    // The keys go to the editor of the active tab, when it has one.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
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
        let repos = self.repos.read(cx);
        let items = tabs
            .docs()
            .iter()
            .map(|doc| {
                tab::Item::new(
                    doc.path().to_string_lossy().into_owned(),
                    doc.name().to_owned(),
                    doc.is_dirty(),
                )
                .change(repos.change_of(doc.path()))
            })
            .collect();
        let active = tabs.active_ix();
        let body = self.body(cx);

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
        .child("No files open. Pick one in the tree (Ctrl+B) or with Ctrl+K.")
        .into_any_element()
}
