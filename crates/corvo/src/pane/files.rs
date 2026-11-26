use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::{Node, Tree, Workbench};
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
}

pub struct Files {
    bench: Entity<Workbench>,
    // How each file and folder stands against the last commit, in its row's color.
    repos: Entity<Repos>,
    state: Entity<State>,
    dirs: Rc<HashSet<SharedString>>,
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
        let mut dirs = HashSet::new();
        let items = match self.bench.read(cx).tree() {
            Some(tree) => items(tree, tree.root(), &mut dirs),
            None => Vec::new(),
        };
        self.dirs = Rc::new(dirs);

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
        let view = cx.entity().downgrade();
        let repos = self.repos.clone();

        Panel::new(&self.state, move |entry, cx| {
            let id = &entry.item().id;
            let path = PathBuf::from(id.as_ref());
            let change = repos.read(cx).change_of(&path);
            let dir = dirs.contains(id);
            let row = if dir {
                Row::dir(entry)
            } else {
                let view = view.clone();
                let path = path.clone();
                Row::file(entry).on_click(move |_, _, cx| {
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

fn items(tree: &Tree, dir: &Path, dirs: &mut HashSet<SharedString>) -> Vec<Item> {
    tree.children(dir)
        .unwrap_or_default()
        .iter()
        .map(|node| item(tree, node, dirs))
        .collect()
}

fn item(tree: &Tree, node: &Node, dirs: &mut HashSet<SharedString>) -> Item {
    let id = id(node.path());
    let item = Item::new(id.clone(), node.name().to_owned());
    if !node.is_dir() {
        return item;
    }
    dirs.insert(id.clone());
    if !tree.is_loaded(node.path()) {
        return item.child(pending(&id));
    }
    item.expanded(tree.is_open(node.path()))
        .children(items(tree, node.path(), dirs))
}

fn id(path: &Path) -> SharedString {
    SharedString::from(path.to_string_lossy().into_owned())
}

fn pending(id: &SharedString) -> Item {
    Item::new(format!("{id}/\u{0}"), "")
}
