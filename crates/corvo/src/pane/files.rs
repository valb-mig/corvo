use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::Depth;
use corvo_domain::{Node, Symbol, SymbolKind, Tree, Workbench, gather_impls, path_to};
use corvo_ui::git::tint;
use corvo_ui::notice;
use corvo_ui::tree::{Event as TreeEvent, Item, Panel, Row, State};
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, IntoElement, Render, ScrollStrategy,
    SharedString, Subscription, Window,
};

use crate::repos::Repos;

pub enum Event {
    Pick(PathBuf),
    // A symbol of the active file was picked: its line and column, 0-based.
    Jump(PathBuf, u32, u32),
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

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let bench = self.bench.read(cx);
        let mut build = Build {
            outline: &self.outline,
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

        Panel::new(&self.state, move |entry, cx| {
            let id = &entry.item().id;
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
        })
    }
}

// The items of the tree, read from the workbench's, with the outline under the active file.
struct Build<'a> {
    outline: &'a Outline,
    dirs: HashSet<SharedString>,
    marks: HashMap<SharedString, Mark>,
}

impl Build<'_> {
    fn items(&mut self, tree: &Tree, dir: &Path) -> Vec<Item> {
        tree.children(dir)
            .unwrap_or_default()
            .iter()
            .map(|node| self.item(tree, node))
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

fn pending(id: &SharedString) -> Item {
    Item::new(format!("{id}/\u{0}"), "")
}
