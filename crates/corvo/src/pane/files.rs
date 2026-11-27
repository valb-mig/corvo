use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::Depth;
use corvo_domain::{
    Kind, Node, Renamed, Symbol, SymbolKind, Tree, Workbench, gather_impls, path_to,
};
use corvo_ui::git::tint;
use corvo_ui::keys::{RemovePicked, RenamePicked};
use corvo_ui::tree::{Event as TreeEvent, Item, Panel, Row, State};
use corvo_ui::{field, notice};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::input::Enter;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, ScrollStrategy, SharedString, Styled as _,
    Subscription, WeakEntity, Window, div,
};

use crate::repos::Repos;

pub enum Event {
    Pick(PathBuf),
    // A symbol of the active file was picked: its line and column, 0-based.
    Jump(PathBuf, u32, u32),
    // A file was made in the tree, to be opened.
    Made(PathBuf),
    // A file or folder took another name, and the tabs that followed it.
    Renamed(Renamed),
    // A file or folder went to the trash; the tabs of what it held, closed already.
    Trashed(Vec<PathBuf>),
    // A terminal asked for in a folder.
    Terminal(PathBuf),
}

// What a row of the tree has typed in it: the name of a file or folder to make in a folder,
// or the new name of one.
enum Typing {
    Make { dir: PathBuf, kind: Kind },
    Rename(PathBuf),
}

struct Naming {
    typing: Typing,
    field: Entity<field::State>,
    _subs: Vec<Subscription>,
}

impl Naming {
    // The row the field is in: a new one on top of the folder, or the one renamed.
    fn row(&self) -> SharedString {
        match &self.typing {
            Typing::Make { dir, .. } => making_id(dir),
            Typing::Rename(path) => id(path),
        }
    }

    fn is_dir(&self, dirs: &HashSet<SharedString>) -> bool {
        typing_dir(&self.typing, dirs)
    }
}

fn typing_dir(typing: &Typing, dirs: &HashSet<SharedString>) -> bool {
    match typing {
        Typing::Make { kind, .. } => *kind == Kind::Dir,
        Typing::Rename(path) => dirs.contains(&id(path)),
    }
}

// The outline of the active file, nested under its row in the tree: a type holds what its impls
// hold, its fields gathered in a row of their own. All is folded but the way to the cursor, and
// what was opened by hand.
#[derive(Default)]
struct Outline {
    file: Option<PathBuf>,
    symbols: Vec<Symbol>,
    // The rows from the file down to the symbol the cursor is in.
    way: Vec<SharedString>,
    open: HashSet<SharedString>,
    // The file's own row folded, its symbols put away.
    shut: bool,
}

// What a row of the outline stands for.
#[derive(Clone)]
enum Mark {
    Symbol {
        kind: SymbolKind,
        line: u32,
        column: u32,
        // How many fields and methods it holds, told while it is folded.
        holds: Vec<(SymbolKind, usize)>,
    },
    Group {
        kind: SymbolKind,
        count: usize,
    },
}

pub struct Files {
    bench: Entity<Workbench>,
    // How each file and folder stands against the last commit, in its row's color.
    repos: Entity<Repos>,
    state: Entity<State>,
    dirs: Rc<HashSet<SharedString>>,
    outline: Outline,
    marks: Rc<HashMap<SharedString, Mark>>,
    naming: Option<Naming>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for Files {}

impl Files {
    pub fn new(
        bench: Entity<Workbench>,
        repos: Entity<Repos>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.new(|cx| State::new(cx));
        let subs = vec![
            cx.subscribe_in(&state, window, Self::on_tree),
            cx.observe(&repos, |_, _, cx| cx.notify()),
        ];
        let mut files = Self {
            bench,
            repos,
            state,
            dirs: Rc::default(),
            outline: Outline::default(),
            marks: Rc::default(),
            naming: None,
            _subs: subs,
        };
        files.refresh(cx);
        files
    }

    pub fn reveal(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = self.bench.update(cx, |bench, _| bench.reveal(path)) {
            notice::error(err.to_string(), window, cx);
        }
        self.refresh(cx);
        self.select(path, cx);
    }

    // Nests the outline of the active file, or of none, under its row, open down to the cursor's
    // line; the same file again only follows the line.
    pub fn follow(
        &mut self,
        file: Option<&Path>,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.outline.file.as_deref() == file {
            if let (Some(file), Some(line)) = (file, line) {
                self.cursor(file, line, cx);
            }
            return;
        }
        self.outline = Outline {
            file: file.map(Path::to_path_buf),
            ..Outline::default()
        };
        self.trace(window, cx);
        if let Some(line) = line {
            self.walk(line);
        }
        self.refresh(cx);
    }

    // The cursor of the active file went to another line: the rows it comes into open, and the
    // one it is in is marked. What it leaves stays as it was.
    pub fn cursor(&mut self, file: &Path, line: u32, cx: &mut Context<Self>) {
        if self.outline.file.as_deref() != Some(file) {
            return;
        }
        if self.walk(line) {
            self.refresh(cx);
        } else {
            cx.notify();
        }
    }

    // The file was written: its outline is read again, what was open stays open.
    pub fn saved(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self.outline.file.as_deref() == Some(path) {
            self.trace(window, cx);
            self.refresh(cx);
        }
    }

    fn trace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.outline.file.clone() else {
            self.outline.symbols.clear();
            return;
        };
        self.outline.symbols = match self.bench.read(cx).outline(&file, Depth::Items) {
            Ok(symbols) => gather_impls(symbols),
            Err(err) => {
                notice::error(format!("{}: {err}", file.display()), window, cx);
                Vec::new()
            }
        };
    }

    // Takes the way to `line`, opening the rows on it that were not on the last one; tells
    // whether any opened.
    fn walk(&mut self, line: u32) -> bool {
        let Some(file) = &self.outline.file else {
            return false;
        };
        let path = path_to(&self.outline.symbols, line);
        let way = way(&id(file), &self.outline.symbols, &path);
        let mut opened = false;
        for id in way.iter().filter(|id| !self.outline.way.contains(id)) {
            opened |= self.outline.open.insert(id.clone());
        }
        self.outline.way = way;
        opened
    }

    fn select(&mut self, path: &Path, cx: &mut Context<Self>) {
        let id = id(path);
        self.state.update(cx, |state, cx| {
            let ix = state.index_of(&id);
            state.set_selected_index(ix, cx);
            if let Some(ix) = ix {
                state.scroll_to_item(ix, ScrollStrategy::Nearest);
            }
        });
    }

    fn on_tree(
        &mut self,
        _: &Entity<State>,
        event: &TreeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, open) = match event {
            TreeEvent::Expanded(id) => (id, true),
            TreeEvent::Collapsed(id) => (id, false),
        };
        // A row of the outline, or the file's own, folds without a reading of the tree; the
        // file stays the one picked.
        let file = self.outline.file.clone();
        if self.marks.contains_key(id) || file.as_deref().is_some_and(|f| *id == self::id(f)) {
            if self.marks.contains_key(id) {
                if open {
                    self.outline.open.insert(id.clone());
                } else {
                    self.outline.open.remove(id);
                }
            } else {
                self.outline.shut = !open;
            }
            if let Some(file) = file {
                self.select(&file, cx);
            }
            return;
        }
        if !self.dirs.contains(id) {
            return;
        }
        let path = Path::new(id.as_ref());
        if open {
            if let Err(err) = self.bench.update(cx, |bench, _| bench.expand(path)) {
                notice::error(err.to_string(), window, cx);
            }
        } else {
            self.bench.update(cx, |bench, _| bench.collapse(path));
        }
        self.refresh(cx);
    }

    // A row on top of the folder, opened, to type the name of a file or folder to make in it.
    fn make(&mut self, dir: PathBuf, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = self.bench.update(cx, |bench, _| bench.expand(&dir)) {
            notice::error(format!("{}: {err}", dir.display()), window, cx);
            return;
        }
        let placeholder = match kind {
            Kind::File => "File name",
            Kind::Dir => "Folder name",
        };
        self.name(Typing::Make { dir, kind }, "", placeholder, window, cx);
    }

    // The name of a file or folder turned into a field, its stem selected.
    fn rename(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.name(Typing::Rename(path), &name, "New name", window, cx);
    }

    fn name(
        &mut self,
        typing: Typing,
        value: &str,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = cx.new(|cx| field::state(placeholder.to_owned(), window, cx));
        let stem = stem_len(value, typing_dir(&typing, &self.dirs));
        field.update(cx, |field, cx| {
            field.set_value(value.to_owned(), window, cx);
            field.focus(window, cx);
            field.set_selected_range(0..stem, cx);
        });
        // Enter keeps the field when the name is refused, so it can be fixed; clicking away
        // does not.
        let subs = vec![
            cx.subscribe_in(&field, window, |this, _, event, window, cx| {
                match event {
                    field::Event::PressEnter { .. } => this.commit(true, window, cx),
                    // Going to another window leaves the field as it is.
                    field::Event::Blur if window.is_window_active() => {
                        this.commit(false, window, cx)
                    }
                    _ => {}
                }
            }),
        ];
        let naming = Naming {
            typing,
            field,
            _subs: subs,
        };
        let row = naming.row();
        self.naming = Some(naming);
        self.refresh(cx);
        self.state.update(cx, |state, _| {
            if let Some(ix) = state.index_of(&row) {
                state.scroll_to_item(ix, ScrollStrategy::Nearest);
            }
        });
    }

    // Makes or renames what the field names. A name left empty, or as it was, changes nothing.
    fn commit(&mut self, keep: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(naming) = self.naming.take() else {
            return;
        };
        let name = naming.field.read(cx).value().to_string();
        let done = match &naming.typing {
            _ if name.trim().is_empty() => Ok(None),
            Typing::Rename(path) if path.file_name().is_some_and(|old| *old == *name) => Ok(None),
            Typing::Make { dir, kind } => self
                .bench
                .update(cx, |bench, _| bench.create(dir, &name, *kind))
                .map(|path| {
                    Some((
                        path.clone(),
                        (*kind == Kind::File).then_some(Event::Made(path)),
                    ))
                }),
            Typing::Rename(path) => self
                .bench
                .update(cx, |bench, _| bench.rename(path, &name))
                .map(|renamed| Some((renamed.path.clone(), Some(Event::Renamed(renamed))))),
        };
        match done {
            Ok(made) => {
                self.refresh(cx);
                self.focus_tree(window, cx);
                if let Some((path, event)) = made {
                    self.select(&path, cx);
                    if let Some(event) = event {
                        cx.emit(event);
                    }
                }
            }
            Err(err) => {
                notice::error(format!("{}: {err}", name.trim()), window, cx);
                if keep {
                    self.naming = Some(naming);
                } else {
                    self.refresh(cx);
                }
            }
        }
    }

    // A file or folder's name is being typed in a row.
    pub fn is_naming(&self) -> bool {
        self.naming.is_some()
    }

    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.naming.take().is_some() {
            self.refresh(cx);
            self.focus_tree(window, cx);
        }
    }

    fn focus_tree(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| state.focus(window, cx));
    }

    // Asks whether to send a file or folder to the trash, telling of unsaved changes it holds.
    fn ask_trash(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = self.dirs.contains(&id(&path));
        let unsaved = self
            .bench
            .read(cx)
            .tabs()
            .docs()
            .iter()
            .any(|doc| doc.path().starts_with(&path) && doc.is_dirty());
        let mut told = if dir {
            "The folder and all it holds can be brought back from the Trash.".to_owned()
        } else {
            "It can be brought back from the Trash.".to_owned()
        };
        if unsaved {
            told.push_str(" Changes not saved in it are lost.");
        }
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (view, path) = (view.clone(), path.clone());
            alert
                .title(format!("Move {name} to the Trash?"))
                .description(told.clone())
                .ok_text("Move to Trash")
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |this, cx| this.trash(&path, window, cx))
                        .ok();
                    true
                })
        });
    }

    fn trash(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match self.bench.update(cx, |bench, _| bench.trash(path)) {
            Ok(closed) => {
                self.refresh(cx);
                cx.emit(Event::Trashed(closed));
            }
            Err(err) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                notice::error(format!("{name}: {err}"), window, cx);
            }
        }
    }

    // The file or folder of the row picked in the tree.
    fn picked(&self, cx: &App) -> Option<PathBuf> {
        self.path_of(&self.state.read(cx).selected_item()?.id)
    }

    // The file or folder a row stands for: a symbol's row stands for its file, and a row still
    // being read, or named, for none.
    fn path_of(&self, id: &SharedString) -> Option<PathBuf> {
        if self.marks.contains_key(id) {
            return self.outline.file.clone();
        }
        if id.contains('\u{0}') || id.contains('\u{3}') {
            return None;
        }
        Some(PathBuf::from(id.as_ref()))
    }

    // What a right click offers on the row of a file or folder, or, with none, under the rows:
    // to make a file or folder there, to rename it or send it to the trash, to copy its path,
    // and to open it outside.
    fn fill_menu(
        &self,
        view: &WeakEntity<Self>,
        target: Option<PathBuf>,
        menu: PopupMenu,
        cx: &App,
    ) -> PopupMenu {
        let Some(root) = self.bench.read(cx).project().map(Path::to_path_buf) else {
            return menu;
        };
        let path = target.unwrap_or_else(|| root.clone());
        let is_dir = path == root || self.dirs.contains(&id(&path));
        let dir = if is_dir {
            path.clone()
        } else {
            path.parent().map_or(root.clone(), Path::to_path_buf)
        };
        let made = dir.clone();
        let mut menu = menu
            .item(entry(
                "New File…",
                Glyph::FilePlus,
                act(view, move |this, window, cx| {
                    this.make(made.clone(), Kind::File, window, cx)
                }),
            ))
            .item(entry(
                "New Folder…",
                Glyph::FolderPlus,
                act(view, move |this, window, cx| {
                    this.make(dir.clone(), Kind::Dir, window, cx)
                }),
            ));
        if path != root {
            let (renamed, trashed) = (path.clone(), path.clone());
            menu = menu
                .separator()
                .item(entry(
                    "Rename…",
                    Glyph::Pencil,
                    act(view, move |this, window, cx| {
                        this.rename(renamed.clone(), window, cx)
                    }),
                ))
                .item(entry(
                    "Move to Trash",
                    Glyph::Trash,
                    act(view, move |this, window, cx| {
                        this.ask_trash(trashed.clone(), window, cx)
                    }),
                ));
        }
        let full = path.display().to_string();
        menu = menu
            .separator()
            .item(entry("Copy Path", Glyph::Copy, move |window, cx| {
                copy(full.clone(), window, cx)
            }));
        if let Ok(rest) = path.strip_prefix(&root)
            && !rest.as_os_str().is_empty()
        {
            let rest = rest.display().to_string();
            menu = menu.item(entry(
                "Copy Relative Path",
                Glyph::Copy,
                move |window, cx| copy(rest.clone(), window, cx),
            ));
        }
        let shown = path.clone();
        menu = menu.separator();
        if is_dir {
            menu.item(entry(
                "Open in File Manager",
                Glyph::FolderOpen,
                move |_, cx| cx.open_with_system(&shown),
            ))
            .item(entry(
                "Open Terminal Here",
                Glyph::SquareTerminal,
                act(view, move |_, _, cx| cx.emit(Event::Terminal(path.clone()))),
            ))
        } else {
            menu.item(entry(
                "Show in File Manager",
                Glyph::FolderOpen,
                move |_, cx| cx.reveal_path(&shown),
            ))
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let bench = self.bench.read(cx);
        let making = match self.naming.as_ref().map(|naming| &naming.typing) {
            Some(Typing::Make { dir, .. }) => Some(dir.as_path()),
            _ => None,
        };
        let mut build = Build {
            outline: &self.outline,
            making,
            dirs: HashSet::new(),
            marks: HashMap::new(),
        };
        let items = match bench.tree() {
            Some(tree) => build.items(tree, tree.root()),
            None => Vec::new(),
        };
        self.dirs = Rc::new(build.dirs);
        self.marks = Rc::new(build.marks);

        self.state.update(cx, |state, cx| {
            let selected = state.selected_item().map(|item| item.id.clone());
            state.set_items(items, cx);
            let ix = selected.and_then(|id| state.index_of(&id));
            state.set_selected_index(ix, cx);
        });
        cx.notify();
    }
}

impl Render for Files {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dirs = self.dirs.clone();
        let marks = self.marks.clone();
        let here = self.outline.way.last().cloned();
        let file = self.outline.file.clone().unwrap_or_default();
        let view = cx.entity().downgrade();
        let repos = self.repos.clone();
        let naming = self
            .naming
            .as_ref()
            .map(|naming| (naming.row(), naming.is_dir(&dirs), naming.field.clone()));

        let panel = Panel::new(&self.state, move |entry, cx| {
            let id = &entry.item().id;
            if let Some((row, dir, field)) = &naming
                && row == id
            {
                // Enter in the field ends the naming, through the field's event, and goes no
                // further, where it would open the row picked. Esc is the window's to tell.
                let field = div()
                    .on_action(|_: &Enter, _, _| {})
                    .child(field::compact(field));
                return Row::naming(entry, *dir, field, cx);
            }
            match marks.get(id) {
                Some(Mark::Group { kind, count }) => return Row::group(entry, *kind, *count, cx),
                Some(Mark::Symbol {
                    kind,
                    line,
                    column,
                    holds,
                }) => {
                    let folds = entry.is_folder();
                    let mut row = Row::symbol(entry, *kind, cx);
                    if folds && !entry.is_expanded() {
                        row = Row::counts(row, holds.clone(), cx);
                    }
                    if here.as_ref() == Some(id) {
                        row = Row::here(row, cx);
                    }
                    // A click goes to the symbol, but one that folds it away stays put.
                    if folds && entry.is_expanded() {
                        return row;
                    }
                    let (view, file, at) = (view.clone(), file.clone(), (*line, *column));
                    return row.on_click(move |_, _, cx| {
                        let file = file.clone();
                        view.update(cx, |_, cx| cx.emit(Event::Jump(file, at.0, at.1)))
                            .ok();
                    });
                }
                None => {}
            }
            let path = PathBuf::from(id.as_ref());
            let change = repos.read(cx).change_of(&path);
            let dir = dirs.contains(id);
            let row = if dir {
                Row::dir(entry, cx)
            } else {
                let view = view.clone();
                let path = path.clone();
                Row::file(entry, cx).on_click(move |_, _, cx| {
                    let path = path.clone();
                    view.update(cx, |_, cx| cx.emit(Event::Pick(path))).ok();
                })
            };
            match change {
                Some(change) => Row::tint(row, tint(change, cx), !dir),
                None => row,
            }
        });

        let menu = cx.entity().downgrade();
        let panel = panel.context_menu(move |entry, popup, _, cx| {
            let id = &entry.item().id;
            let Some(files) = menu.upgrade() else {
                return popup;
            };
            let files = files.read(cx);
            match files.path_of(id) {
                Some(path) => files.fill_menu(&menu, Some(path), popup, cx),
                None => popup,
            }
        });
        // Under the rows, a right click offers what can be made in the project's folder.
        let under = cx.entity().downgrade();
        div()
            .id("file-tree")
            .size_full()
            .on_action(cx.listener(|this, _: &RenamePicked, window, cx| {
                if let Some(path) = this.picked(cx) {
                    this.rename(path, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &RemovePicked, window, cx| {
                if let Some(path) = this.picked(cx) {
                    this.ask_trash(path, window, cx);
                }
            }))
            .child(panel)
            .context_menu(move |popup, _, cx| match under.upgrade() {
                Some(files) => files.read(cx).fill_menu(&under, None, popup, cx),
                None => popup,
            })
    }
}

// The items of the tree, read from the workbench's, with the outline under the active file and
// the row of a file or folder being made on top of its folder.
struct Build<'a> {
    outline: &'a Outline,
    making: Option<&'a Path>,
    dirs: HashSet<SharedString>,
    marks: HashMap<SharedString, Mark>,
}

impl Build<'_> {
    fn items(&mut self, tree: &Tree, dir: &Path) -> Vec<Item> {
        let made = (self.making == Some(dir)).then(|| Item::new(making_id(dir), ""));
        made.into_iter()
            .chain(
                tree.children(dir)
                    .unwrap_or_default()
                    .iter()
                    .map(|node| self.item(tree, node)),
            )
            .collect()
    }

    fn item(&mut self, tree: &Tree, node: &Node) -> Item {
        let id = id(node.path());
        let item = Item::new(id.clone(), node.name().to_owned());
        if !node.is_dir() {
            let outline = self.outline;
            if outline.file.as_deref() != Some(node.path()) || outline.symbols.is_empty() {
                return item;
            }
            return item
                .expanded(!outline.shut)
                .children(self.symbols(&id, &outline.symbols));
        }
        self.dirs.insert(id.clone());
        if !tree.is_loaded(node.path()) {
            return item.child(pending(&id));
        }
        item.expanded(tree.is_open(node.path()))
            .children(self.items(tree, node.path()))
    }

    fn symbols(&mut self, parent: &SharedString, symbols: &[Symbol]) -> Vec<Item> {
        (0..symbols.len())
            .map(|ix| self.symbol(child_id(parent, symbols, ix), &symbols[ix]))
            .collect()
    }

    fn symbol(&mut self, id: SharedString, symbol: &Symbol) -> Item {
        let nested = symbol.nested();
        let ids: Vec<_> = (0..nested.len())
            .map(|ix| child_id(&id, nested, ix))
            .collect();
        let mut children = Vec::new();
        let data: Vec<_> = (0..nested.len())
            .filter(|&ix| is_data(nested[ix].kind()))
            .collect();
        if grouped(symbol) {
            let group = group_id(&id);
            let kind = nested[data[0]].kind();
            let fields = data
                .iter()
                .map(|&ix| self.symbol(ids[ix].clone(), &nested[ix]))
                .collect::<Vec<_>>();
            let label = if kind == SymbolKind::Variant {
                "variants"
            } else {
                "fields"
            };
            self.marks.insert(
                group.clone(),
                Mark::Group {
                    kind,
                    count: data.len(),
                },
            );
            children.push(
                Item::new(group.clone(), label)
                    .expanded(self.outline.open.contains(&group))
                    .children(fields),
            );
            for ix in (0..nested.len()).filter(|ix| !data.contains(ix)) {
                children.push(self.symbol(ids[ix].clone(), &nested[ix]));
            }
        } else {
            for (ix, child) in nested.iter().enumerate() {
                children.push(self.symbol(ids[ix].clone(), child));
            }
        }

        let mut holds = Vec::new();
        if let Some(&first) = data.first() {
            holds.push((nested[first].kind(), data.len()));
        }
        let calls = nested
            .iter()
            .filter(|s| matches!(s.kind(), SymbolKind::Method | SymbolKind::Function))
            .count();
        if calls > 0 {
            holds.push((SymbolKind::Method, calls));
        }
        self.marks.insert(
            id.clone(),
            Mark::Symbol {
                kind: symbol.kind(),
                line: symbol.line(),
                column: symbol.column(),
                holds,
            },
        );
        Item::new(id.clone(), symbol.name().to_owned())
            .expanded(self.outline.open.contains(&id))
            .children(children)
    }
}

// The ids of the rows from the file down to the symbol at the end of `path`, through the row
// of fields when the way goes into one.
fn way(file: &SharedString, symbols: &[Symbol], path: &[usize]) -> Vec<SharedString> {
    let mut ids = Vec::new();
    let mut parent = file.clone();
    let mut level = symbols;
    let mut holder: Option<&Symbol> = None;
    for &ix in path {
        let Some(symbol) = level.get(ix) else {
            break;
        };
        if holder.is_some_and(grouped) && is_data(symbol.kind()) {
            ids.push(group_id(&parent));
        }
        let id = child_id(&parent, level, ix);
        ids.push(id.clone());
        holder = Some(symbol);
        level = symbol.nested();
        parent = id;
    }
    ids
}

// A type's fields go in a row of their own when it holds more than fields.
fn grouped(symbol: &Symbol) -> bool {
    let nested = symbol.nested();
    is_type(symbol.kind())
        && nested.iter().any(|s| is_data(s.kind()))
        && nested.iter().any(|s| !is_data(s.kind()))
}

fn is_type(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Class | SymbolKind::Interface
    )
}

fn is_data(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::Field | SymbolKind::Variant)
}

// A symbol's row by the names that lead to it, which hold while its lines move; a name that
// siblings before it share takes `~n`.
fn child_id(parent: &SharedString, siblings: &[Symbol], ix: usize) -> SharedString {
    let name = siblings[ix].name();
    let before = siblings[..ix].iter().filter(|s| s.name() == name).count();
    let step = match before {
        0 => name.to_owned(),
        n => format!("{name}~{n}"),
    };
    SharedString::from(format!("{parent}\u{1}{step}"))
}

fn group_id(parent: &SharedString) -> SharedString {
    SharedString::from(format!("{parent}\u{1}\u{2}"))
}

fn id(path: &Path) -> SharedString {
    SharedString::from(path.to_string_lossy().into_owned())
}

fn making_id(dir: &Path) -> SharedString {
    SharedString::from(format!("{}/\u{3}", dir.display()))
}

// The name of a file without its extension, as a rename selects it; all of it for a folder or a
// name that starts with its only dot.
fn stem_len(name: &str, dir: bool) -> usize {
    match name.rfind('.') {
        Some(dot) if dot > 0 && !dir => dot,
        _ => name.len(),
    }
}

fn entry(
    label: &'static str,
    glyph: Glyph,
    run: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .icon(glyph)
        .on_click(move |_, window, cx| run(window, cx))
}

// What an entry of the menu does to the tree, while it is still there.
fn act(
    view: &WeakEntity<Files>,
    f: impl Fn(&mut Files, &mut Window, &mut Context<Files>) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let view = view.clone();
    move |window, cx| {
        view.update(cx, |this, cx| f(this, window, cx)).ok();
    }
}

fn copy(text: String, window: &mut Window, cx: &mut App) {
    notice::info(format!("Copied {text}"), window, cx);
    cx.write_to_clipboard(ClipboardItem::new_string(text));
}

fn pending(id: &SharedString) -> Item {
    Item::new(format!("{id}/\u{0}"), "")
}
