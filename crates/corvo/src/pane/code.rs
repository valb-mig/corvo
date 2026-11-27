use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use corvo_domain::{Change, Content, Doc, Finder, Hunk, HunkMark, Lang, Workbench, hunks};
use corvo_lsp::Hub;
use corvo_ui::find::{Count, FindBar};
use corvo_ui::{drawer, editor, field, notice, tab, viewer};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::{Replace, RopeExt as _, Search};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, Task, Window, div,
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

// The bar that finds in the active file: what is looked for and what replaces it, kept while
// the bar is closed for the next time it opens.
struct Find {
    query: Entity<field::State>,
    replace: Entity<field::State>,
    case: bool,
    replacing: bool,
    _subs: Vec<Subscription>,
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
    find: Option<Find>,
    finding: bool,
    // How wide the drawer open over the right of the code is; the find bar keeps clear of it.
    beside_panel: Option<f32>,
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
            find: None,
            finding: false,
            beside_panel: None,
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
        self.seek(cx);
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
        self.seek(cx);
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
        self.forget(&[path], window, cx);
    }

    // Lets go of the editors of tabs closed already, as with files gone to the trash; the tab
    // in front takes the keys, or the code stays empty.
    pub fn forget(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        for path in paths {
            self.drop_view(path, cx);
        }
        self.reap(cx);
        if self.bench.read(cx).tabs().active().is_none() {
            cx.emit(Event::Emptied);
            cx.notify();
            return;
        }
        self.focus(window, cx);
        self.announce(cx);
        self.seek(cx);
        cx.notify();
    }

    // Opens the bar that finds in the active file, with what is selected in it as the query
    // when that is one line; again, it takes the keys back. With `replace`, its line to replace
    // opens too, and takes them.
    pub fn find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.shown {
            return;
        }
        let Some((_, state)) = self.active(cx) else {
            return;
        };
        let find = self.find_bar(window, cx);
        find.replacing |= replace;
        let (query, with) = (find.query.clone(), find.replace.clone());
        let selected = state.read(cx).selected_value();
        if !selected.is_empty() && !selected.contains('\n') {
            query.update(cx, |query, cx| query.set_value(selected, window, cx));
        }
        let field = if replace { with } else { query };
        field.update(cx, |field, cx| {
            field.focus(window, cx);
            field.select_all(window, cx);
        });
        self.finding = true;
        self.seek(cx);
        cx.notify();
    }

    // The bar is open over the code in sight.
    pub fn is_finding(&self) -> bool {
        self.shown && self.finding
    }

    // Closes the bar: no file shows its matches any more, and the keys go back to the editor.
    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finding = false;
        for view in self.views.values() {
            if let View::Text(state) = view {
                state.update(cx, |state, cx| state.close_search(cx));
            }
        }
        self.focus(window, cx);
        cx.notify();
    }

    pub fn beside_panel(&mut self, width: Option<f32>, cx: &mut Context<Self>) {
        if self.beside_panel != width {
            self.beside_panel = width;
            cx.notify();
        }
    }

    // What is selected in the file in front, when it is on one line.
    pub fn selected_line(&self, cx: &App) -> Option<String> {
        let (_, state) = self.active_in(cx)?;
        let selected = state.read(cx).selected_value();
        (!selected.is_empty() && !selected.contains('\n')).then(|| selected.to_string())
    }

    // The text of each file open with changes not saved, as it is in its editor.
    pub fn unsaved(&self, cx: &App) -> HashMap<PathBuf, String> {
        let tabs = self.bench.read(cx).tabs();
        self.views
            .iter()
            .filter(|(path, _)| tabs.get(path).is_some_and(Doc::is_dirty))
            .filter_map(|(path, view)| match view {
                View::Text(state) => Some((path.clone(), state.read(cx).value().to_string())),
                View::Image | View::Binary(_) => None,
            })
            .collect()
    }

    // Replaces what the finder finds in a file open as text, in its editor, where it can be
    // undone and is left to save; how many it replaced. None for a file not open so.
    pub fn replace_in(
        &mut self,
        path: &Path,
        finder: &Finder,
        with: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let Some(View::Text(state)) = self.views.get(path) else {
            return None;
        };
        let text = state.read(cx).value().to_string();
        let (text, count) = finder.replace(&text, with);
        if count > 0 {
            state.update(cx, |state, cx| state.replace_all(text, window, cx));
        }
        Some(count)
    }

    // Opens a file with the bytes in `range` selected and in sight.
    pub fn reveal_range(
        &mut self,
        path: &Path,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open(path, window, cx);
        let Some(View::Text(state)) = self.views.get(path) else {
            return;
        };
        state.update(cx, |state, cx| {
            let end = state.text().len();
            let range = range.start.min(end)..range.end.min(end);
            let at = state.text().offset_to_position(range.start);
            state.set_cursor_position(at, window, cx);
            state.set_selected_range(range, cx);
        });
    }

    fn find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> &mut Find {
        if self.find.is_none() {
            let query = cx.new(|cx| field::state("Find", window, cx));
            let replace = cx.new(|cx| field::state("Replace", window, cx));
            let subs = vec![
                cx.subscribe(&query, |this, _, event: &field::Event, cx| match event {
                    field::Event::Change => this.seek(cx),
                    field::Event::PressEnter { shift, .. } => this.step(*shift, cx),
                    _ => {}
                }),
                cx.subscribe_in(&replace, window, |this, _, event, window, cx| {
                    if let field::Event::PressEnter { secondary, .. } = event {
                        this.replace(*secondary, window, cx);
                    }
                }),
            ];
            self.find = Some(Find {
                query,
                replace,
                case: false,
                replacing: false,
                _subs: subs,
            });
        }
        self.find.as_mut().expect("made above")
    }

    // Marks the matches of the query in the active file, from where it is scrolled to.
    fn seek(&mut self, cx: &mut Context<Self>) {
        let (Some(find), true) = (&self.find, self.finding) else {
            return;
        };
        let Some((_, state)) = self.active(cx) else {
            return;
        };
        let query = find.query.read(cx).value().to_string();
        let insensitive = !find.case;
        state.update(cx, |state, cx| {
            state.set_search_query(query, insensitive, cx);
        });
        cx.notify();
    }

    // To the match after the current one, or before it.
    fn step(&mut self, back: bool, cx: &mut Context<Self>) {
        let Some((_, state)) = self.active(cx) else {
            return;
        };
        state.update(cx, |state, cx| {
            if back {
                state.previous_search_match(cx);
            } else {
                state.next_search_match(cx);
            }
        });
        cx.notify();
    }

    // Replaces the current match and goes to the next, or replaces them all.
    fn replace(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(find), Some((_, state))) = (&self.find, self.active(cx)) else {
            return;
        };
        let with = find.replace.read(cx).value().to_string();
        state.update(cx, |state, cx| {
            if all {
                state.replace_all_search_matches(&with, window, cx);
            } else {
                state.replace_current_search_match(&with, window, cx);
            }
        });
        cx.notify();
    }

    fn toggle_case(&mut self, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.find {
            find.case = !find.case;
        }
        self.seek(cx);
    }

    fn toggle_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = &mut self.find else {
            return;
        };
        find.replacing = !find.replacing;
        let field = if find.replacing {
            find.replace.clone()
        } else {
            find.query.clone()
        };
        field.update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }

    // What the bar tells of the matches in the active file.
    fn count(&self, cx: &App) -> Count {
        let Some((find, (_, state))) = self.find.as_ref().zip(self.active_in(cx)) else {
            return Count::Idle;
        };
        if find.query.read(cx).value().is_empty() {
            return Count::Idle;
        }
        let matcher = &state.read(cx).search_session().matcher;
        match matcher.current() {
            Some(ix) => Count::At(ix + 1, matcher.len()),
            None => Count::None,
        }
    }

    fn find_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.as_ref().filter(|_| self.finding)?;
        let view = cx.entity().downgrade();
        let on = |f: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            let view = view.clone();
            move |window: &mut Window, cx: &mut App| {
                view.update(cx, |this, cx| f(this, window, cx)).ok();
            }
        };
        let mut bar = FindBar::new(&find.query, &find.replace)
            .case(find.case)
            .replacing(find.replacing)
            .count(self.count(cx))
            .on_case(on(|this, _, cx| this.toggle_case(cx)))
            .on_toggle(on(Self::toggle_replace))
            .on_prev(on(|this, _, cx| this.step(true, cx)))
            .on_next(on(|this, _, cx| this.step(false, cx)))
            .on_replace(on(|this, window, cx| this.replace(false, window, cx)))
            .on_replace_all(on(|this, window, cx| this.replace(true, window, cx)))
            .on_close(on(Self::close_find));
        if let Some(width) = self.beside_panel {
            bar = bar.clear_of(drawer::room_of(width));
        }
        Some(bar.into_any_element())
    }

    fn on_search(&mut self, _: &Search, window: &mut Window, cx: &mut Context<Self>) {
        self.find(false, window, cx);
    }

    fn on_replace(&mut self, _: &Replace, window: &mut Window, cx: &mut Context<Self>) {
        self.find(true, window, cx);
    }

    // Files that moved, each from its path before to the one after: their editors open again
    // where they went, with what was typed in them, so the language and the language server
    // follow the new name. The cursor of the file in front stays where it was.
    pub fn moved(
        &mut self,
        moves: &[(PathBuf, PathBuf)],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if moves.is_empty() {
            return;
        }
        for (old, new) in moves {
            let Some(view) = self.drop_view(old, cx) else {
                continue;
            };
            match view {
                View::Text(state) => {
                    let (text, at) = {
                        let state = state.read(cx);
                        (state.value().to_string(), state.cursor_position())
                    };
                    let state = self.edit(new, text, window, cx);
                    state.update(cx, |state, cx| state.set_cursor_position(at, window, cx));
                    self.views.insert(new.clone(), View::Text(state));
                    self.rebase(new, cx);
                }
                other => {
                    self.views.insert(new.clone(), other);
                }
            }
        }
        self.focus(window, cx);
        self.announce(cx);
        self.seek(cx);
        cx.notify();
    }

    // Takes a file's editor away, with all that was kept of it, and tells the language server
    // it closed.
    fn drop_view(&mut self, path: &Path, cx: &mut Context<Self>) -> Option<View> {
        self.hub.update(cx, |hub, _| hub.close(path));
        self.texts.remove(path);
        self.subs.remove(path);
        self.marked.remove(path);
        self.lines.remove(path);
        self.views.remove(path)
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

    // The tab in front is a file open as text, in an editor to take the keys.
    pub fn has_editor(&self, cx: &App) -> bool {
        self.active_in(cx).is_some()
    }

    // The keys go to the editor of the active tab, when it has one.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, state)) = self.active(cx) {
            state.update(cx, |state, cx| state.focus(window, cx));
        }
    }

    fn active(&self, cx: &Context<Self>) -> Option<(PathBuf, Entity<editor::State>)> {
        self.active_in(cx)
    }

    fn active_in(&self, cx: &App) -> Option<(PathBuf, Entity<editor::State>)> {
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
        let find = self.find_view(cx);

        let pick = cx.entity().downgrade();
        let close = pick.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .on_action(cx.listener(Self::on_search))
            .on_action(cx.listener(Self::on_replace))
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
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .children(body)
                    .children(find),
            )
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
