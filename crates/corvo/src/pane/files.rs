use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::{Node, Tree, Workbench};
use corvo_ui::notice;
use corvo_ui::tree::{Event as TreeEvent, Item, Panel, Row, State};
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, IntoElement, Render, SharedString,
    Subscription, Window,
};

pub enum Event {
    Pick(PathBuf),
}

pub struct Files {
    bench: Entity<Workbench>,
    state: Entity<State>,
    dirs: Rc<HashSet<SharedString>>,
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
            _subs: subs,
        };
        files.refresh(cx);
        files
    }

    fn on_tree(
        &mut self,
        _: &Entity<State>,
        event: &TreeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = match event {
            TreeEvent::Expanded(id) => self
                .bench
                .update(cx, |bench, _| bench.expand(Path::new(id.as_ref()))),
            TreeEvent::Collapsed(id) => {
                self.bench
                    .update(cx, |bench, _| bench.collapse(Path::new(id.as_ref())));
                Ok(())
            }
        };
        if let Err(err) = result {
            notice::error(err.to_string(), window, cx);
        }
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let mut dirs = HashSet::new();
        let tree = self.bench.read(cx).tree();
        let items = items(tree, tree.root(), &mut dirs);
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

        Panel::new(&self.state, move |entry| {
            let id = &entry.item().id;
            if dirs.contains(id) {
                return Row::dir(entry);
            }
            let path = PathBuf::from(id.as_ref());
            let view = view.clone();
            Row::file(entry).on_click(move |_, _, cx| {
                let path = path.clone();
                view.update(cx, |_, cx| cx.emit(Event::Pick(path))).ok();
            })
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
    let id = SharedString::from(node.path().to_string_lossy().into_owned());
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

fn pending(id: &SharedString) -> Item {
    Item::new(format!("{id}/\u{0}"), "")
}
