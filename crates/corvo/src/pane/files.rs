use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::Depth;
use corvo_domain::{Node, Symbol, SymbolKind, Tree, Workbench};
use corvo_ui::notice;
use corvo_ui::tree::{Event as TreeEvent, Item, Panel, Row, State};
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, IntoElement, Render, ScrollStrategy,
    SharedString, Subscription, Window,
};

pub enum Event {
    Pick(PathBuf),
    Jump(PathBuf, u32, u32),
}

struct Outline {
    path: PathBuf,
    symbols: Vec<Symbol>,
    folded: HashSet<SharedString>,
}

#[derive(Clone, Copy)]
struct Spot {
    kind: SymbolKind,
    line: u32,
    column: u32,
}

pub struct Files {
    bench: Entity<Workbench>,
    state: Entity<State>,
    dirs: Rc<HashSet<SharedString>>,
    spots: Rc<HashMap<SharedString, Spot>>,
    outline: Option<Outline>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for Files {}

impl Files {
    pub fn new(bench: Entity<Workbench>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| State::new(cx));
        let subs = vec![cx.subscribe_in(&state, window, Self::on_tree)];
        let mut files = Self {
            bench,
            state,
            dirs: Rc::default(),
            spots: Rc::default(),
            outline: None,
            _subs: subs,
        };
        files.refresh(cx);
        files
    }

    pub fn reveal(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = self.bench.update(cx, |bench, _| bench.reveal(path)) {
            notice::error(err.to_string(), window, cx);
        }
        if self
            .outline
            .as_ref()
            .is_none_or(|outline| outline.path != path)
        {
            self.trace(path, window, cx);
        }
        self.refresh(cx);
        self.select(path, cx);
    }

    pub fn saved(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .outline
            .as_ref()
            .is_some_and(|outline| outline.path == path)
        {
            self.trace(path, window, cx);
            self.refresh(cx);
        }
    }

    fn trace(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let symbols = match self.bench.read(cx).outline(path, Depth::Items) {
            Ok(symbols) => symbols,
            Err(err) => {
                notice::error(format!("{}: {err}", path.display()), window, cx);
                Vec::new()
            }
        };
        let folded = match self.outline.take() {
            Some(outline) if outline.path == path => outline.folded,
            _ => HashSet::new(),
        };
        self.outline = Some(Outline {
            path: path.to_path_buf(),
            symbols,
            folded,
        });
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
        let path = Path::new(id.as_ref());
        if !self.dirs.contains(id) {
            if let Some(outline) = &mut self.outline {
                if open {
                    outline.folded.remove(id);
                } else {
                    outline.folded.insert(id.clone());
                }
            }
        } else if open {
            if let Err(err) = self.bench.update(cx, |bench, _| bench.expand(path)) {
                notice::error(err.to_string(), window, cx);
            }
        } else {
            self.bench.update(cx, |bench, _| bench.collapse(path));
        }
        self.refresh(cx);
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let mut walk = Walk {
            outline: self.outline.as_ref(),
            dirs: HashSet::new(),
            spots: HashMap::new(),
        };
        let items = match self.bench.read(cx).tree() {
            Some(tree) => walk.items(tree, tree.root()),
            None => Vec::new(),
        };
        self.dirs = Rc::new(walk.dirs);
        self.spots = Rc::new(walk.spots);

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
        let spots = self.spots.clone();
        let outlined = self.outline.as_ref().map(|outline| outline.path.clone());
        let view = cx.entity().downgrade();

        Panel::new(&self.state, move |entry| {
            let id = &entry.item().id;
            if dirs.contains(id) {
                return Row::dir(entry);
            }
            let view = view.clone();
            if let (Some(spot), Some(file)) = (spots.get(id).copied(), outlined.clone()) {
                return Row::symbol(entry, spot.kind).on_click(move |_, _, cx| {
                    let file = file.clone();
                    view.update(cx, |_, cx| {
                        cx.emit(Event::Jump(file, spot.line, spot.column));
                    })
                    .ok();
                });
            }
            let path = PathBuf::from(id.as_ref());
            Row::file(entry).on_click(move |_, _, cx| {
                let path = path.clone();
                view.update(cx, |_, cx| cx.emit(Event::Pick(path))).ok();
            })
        })
    }
}

struct Walk<'a> {
    outline: Option<&'a Outline>,
    dirs: HashSet<SharedString>,
    spots: HashMap<SharedString, Spot>,
}

impl Walk<'_> {
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
            return match self.outline {
                Some(outline) if outline.path == node.path() => item
                    .expanded(!outline.folded.contains(&id))
                    .children(self.symbols(outline, &outline.symbols)),
                _ => item,
            };
        }
        self.dirs.insert(id.clone());
        if !tree.is_loaded(node.path()) {
            return item.child(pending(&id));
        }
        item.expanded(tree.is_open(node.path()))
            .children(self.items(tree, node.path()))
    }

    fn symbols(&mut self, outline: &Outline, symbols: &[Symbol]) -> Vec<Item> {
        symbols
            .iter()
            .map(|symbol| {
                let (line, column) = (symbol.line(), symbol.column());
                let id = SharedString::from(format!("{}#{line}:{column}", outline.path.display()));
                let kind = symbol.kind();
                self.spots.insert(id.clone(), Spot { kind, line, column });
                Item::new(id.clone(), symbol.name().to_owned())
                    .expanded(!outline.folded.contains(&id))
                    .children(self.symbols(outline, symbol.nested()))
            })
            .collect()
    }
}

fn id(path: &Path) -> SharedString {
    SharedString::from(path.to_string_lossy().into_owned())
}

fn pending(id: &SharedString) -> Item {
    Item::new(format!("{id}/\u{0}"), "")
}
