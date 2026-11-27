use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use corvo_domain::{
    Atlas, Group, Item, Moved, Node, Pile, Place, Weights, Workbench, bytes, column, owner, weigh,
};
use corvo_fs::LocalFs;
use corvo_ui::file_icon::{FileIcon, FolderIcon};
use corvo_ui::git::tint;
use corvo_ui::graph::{self, Branch, Camera, Card, Graph, Submenu};
use corvo_ui::{field, notice, path};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::Icon;
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, IntoElement,
    Render, SharedString, Styled as _, Subscription, WeakEntity, Window,
};

use crate::home::tilde;
use crate::repos::Repos;

pub enum Event {
    // A project stands alone as the root now, or none does.
    Isolated(Option<PathBuf>),
    // Asked from a card's menu: link a folder to the workspace, or unlink a project.
    Link,
    Unlink(PathBuf),
    // A file's card was clicked: open its code.
    Open(PathBuf),
    // Asked from a folder's menu: a terminal in it.
    Terminal(PathBuf),
    // The home changed in a way worth keeping: what is open or hidden, a card moved, a group.
    Arranged,
}

// What a click on a card does.
#[derive(Clone)]
enum Act {
    Isolate(Option<PathBuf>),
    Toggle(Node),
    // A crowded folder shows all its files, or piles them back.
    Spread(PathBuf),
    Pile(PathBuf),
    Hide(Place, String),
    // One hidden card back, or all of them.
    Show(Option<Place>),
    HideMany(Vec<(Place, String)>),
    // Cards dragged, by their keys, to how far each is moved now.
    Move(Vec<(String, (f32, f32))>),
    Untangle,
    Group(Vec<String>),
    Ungroup(u64),
    Rename(u64, String),
    Fold(u64, bool),
    HideGroup(u64),
    ShowGroup(u64),
    Leave(String),
}

// The home of the workspace: its projects, or one project as the root with any number of its
// folders open, as a tree read left to right. A click on a file opens its
// code. Cards are known by a key that holds between runs: a folder or file by its path.
pub struct Overview {
    bench: Entity<Workbench>,
    repos: Entity<Repos>,
    // How much the folders of each project hold, by project, read off the main thread; a project
    // read again while it is being read is read once more after.
    weights: HashMap<PathBuf, Weights>,
    weighing: HashSet<PathBuf>,
    again: HashSet<PathBuf>,
    camera: Camera,
    // Where the keys go while the home is in front, so Space held moves the graph.
    focus: FocusHandle,
    // The card that can be hidden under each key, as last rendered.
    places: RefCell<HashMap<SharedString, (Place, String)>>,
    // The group whose title is being typed in.
    naming: Option<(u64, Entity<field::State>, Subscription)>,
    // How far the wheel turned each pile, by its key: 0 is its cover, then each of its files.
    turned: HashMap<SharedString, usize>,
}

impl EventEmitter<Event> for Overview {}

impl Overview {
    pub fn new(bench: Entity<Workbench>, repos: Entity<Repos>, cx: &mut Context<Self>) -> Self {
        Self {
            bench,
            repos,
            weights: HashMap::new(),
            weighing: HashSet::new(),
            again: HashSet::new(),
            camera: Camera::default(),
            focus: cx.focus_handle(),
            places: RefCell::default(),
            naming: None,
            turned: HashMap::new(),
        }
    }

    // The keys go to the home, the window's own keys still reaching the window around it.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    pub fn chart(&mut self, projects: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let charted = self.bench.update(cx, |bench, _| bench.chart(projects));
        self.report(charted, window, cx);
        self.weigh(projects.to_vec(), cx);
    }

    // Reads again how much the folders of these projects hold.
    fn weigh(&mut self, projects: Vec<PathBuf>, cx: &mut Context<Self>) {
        for project in projects {
            if !self.weighing.insert(project.clone()) {
                self.again.insert(project);
                continue;
            }
            let root = project.clone();
            let task = cx
                .background_executor()
                .spawn(async move { weigh(&LocalFs, &root) });
            cx.spawn(async move |this, cx| {
                let weights = task.await;
                this.update(cx, |this, cx| {
                    this.weighing.remove(&project);
                    this.weights.insert(project.clone(), weights);
                    if this.again.remove(&project) {
                        this.weigh(vec![project], cx);
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    // Files moved on disk: the folders above them are read again, and those down to a file
    // that came or changed open, so it shows.
    pub fn uncover(&mut self, moved: &[Moved], cx: &mut Context<Self>) {
        let opened = self.bench.update(cx, |bench, _| bench.uncover(moved));
        if opened {
            cx.emit(Event::Arranged);
        }
        // The projects where files moved hold more or less now.
        let projects = self.bench.read(cx).atlas().projects().to_vec();
        let touched: Vec<PathBuf> = projects
            .iter()
            .filter(|project| moved.iter().any(|moved| moved.path.starts_with(project)))
            .cloned()
            .collect();
        self.weigh(touched, cx);
        cx.notify();
    }

    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let charted = self.bench.update(cx, |bench, _| bench.rechart());
        self.report(charted, window, cx);
        let projects = self.bench.read(cx).atlas().projects().to_vec();
        self.weigh(projects, cx);
    }

    // A folder's size, `…` while its project is being weighed for the first time, with a `+`
    // when the look stopped short of all it holds.
    fn size(&self, atlas: &Atlas, path: &Path) -> Option<SharedString> {
        let project = owner(path, atlas.projects())?;
        let Some(weights) = self.weights.get(project) else {
            return self.weighing.contains(project).then(|| "…".into());
        };
        let weight = weights.of(path)?;
        let more = if weight.whole { "" } else { "+" };
        Some(format!("{}{more}", bytes(weight.bytes)).into())
    }

    // Brings a file into sight on the home, as the palette finds it: its project alone, the
    // folders down to it open, and the camera gliding to its card, picked.
    pub fn reveal(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match self.bench.update(cx, |bench, _| bench.show_on_home(path)) {
            Ok(true) => {
                self.camera.look_at(id(path));
                cx.emit(Event::Arranged);
                self.focus(window, cx);
                cx.notify();
            }
            Ok(false) => {}
            Err(err) => notice::warn(format!("{}: {err}", path.display()), window, cx),
        }
    }

    // Every card back where the layout puts it, the whole graph centred in view; groups stay.
    pub fn reorganize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.camera.center();
        self.act(&Act::Untangle, window, cx);
    }

    pub fn show_hidden(
        &mut self,
        place: Option<Place>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.act(&Act::Show(place), window, cx);
    }

    pub fn show_group(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.act(&Act::ShowGroup(id), window, cx);
    }

    fn act(&mut self, act: &Act, window: &mut Window, cx: &mut Context<Self>) {
        let isolated = match act {
            Act::Isolate(root) => Some(root.clone()),
            _ => None,
        };
        let mut made = None;
        let done = self.bench.update(cx, |bench, _| {
            match act {
                Act::Isolate(root) => return bench.isolate(root.as_deref()),
                Act::Toggle(node) => return bench.toggle(node),
                Act::Spread(dir) => bench.atlas_mut().spread(dir),
                Act::Pile(dir) => bench.atlas_mut().pile(dir),
                Act::Hide(place, name) => bench.hide(place.clone(), name),
                Act::Show(place) => bench.show_hidden(place.as_ref()),
                Act::HideMany(places) => {
                    for (place, name) in places {
                        bench.hide(place.clone(), name);
                    }
                }
                Act::Move(moves) => {
                    for (key, by) in moves {
                        bench.atlas_mut().nudge(key.clone(), *by);
                    }
                }
                Act::Untangle => bench.atlas_mut().untangle(),
                Act::Group(keys) => made = bench.atlas_mut().group(keys.clone()),
                Act::Ungroup(id) => bench.atlas_mut().ungroup(*id),
                Act::Rename(id, name) => bench.atlas_mut().rename(*id, name),
                Act::Fold(id, collapsed) => bench.atlas_mut().fold(*id, *collapsed),
                Act::HideGroup(id) => bench.atlas_mut().hide_group(*id),
                Act::ShowGroup(id) => bench.atlas_mut().show_group(*id),
                Act::Leave(key) => bench.atlas_mut().leave(key),
            }
            Ok(())
        });
        if let (Ok(()), Some(root)) = (&done, isolated) {
            cx.emit(Event::Isolated(root));
        }
        self.report(done, window, cx);
        // A new group opens its title to be named.
        if let Some(id) = made {
            self.name(id, window, cx);
        }
    }

    fn report(&mut self, done: std::io::Result<()>, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = done {
            notice::error(err.to_string(), window, cx);
        }
        cx.emit(Event::Arranged);
        cx.notify();
    }

    // Opens the title of a group to type its name in, all of it selected.
    fn name(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let atlas = self.bench.read(cx).atlas();
        let Some(name) = atlas
            .groups()
            .iter()
            .find(|group| group.id() == id)
            .map(|group| group.name().to_owned())
        else {
            return;
        };
        let state = cx.new(|cx| field::state("Group name", window, cx));
        state.update(cx, |state, cx| {
            state.set_value(name, window, cx);
            state.select_all(window, cx);
            state.focus(window, cx);
        });
        let sub = cx.subscribe_in(&state, window, |this, _, event, window, cx| {
            if matches!(event, field::Event::PressEnter { .. } | field::Event::Blur) {
                this.named(window, cx);
            }
        });
        self.naming = Some((id, state, sub));
        cx.notify();
    }

    // The name typed in goes to the group; a blank one leaves it as it was.
    fn named(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, state, _)) = self.naming.take() else {
            return;
        };
        let name = state.read(cx).value().to_string();
        self.act(&Act::Rename(id, name), window, cx);
    }

    // Hides the cards picked together, those that can be.
    fn hide_cards(&mut self, keys: &[SharedString], window: &mut Window, cx: &mut Context<Self>) {
        let places = self.places.borrow();
        let hidden = keys
            .iter()
            .filter_map(|key| places.get(key).cloned())
            .collect();
        drop(places);
        self.act(&Act::HideMany(hidden), window, cx);
    }

    // The linked projects, each a root: picking one stands it alone.
    fn workspace(&self, atlas: &Atlas, view: &WeakEntity<Self>, cx: &App) -> Vec<Branch> {
        if atlas.projects().is_empty() {
            let link = view.clone();
            let card = Card::new("note:workspace", "No projects linked")
                .muted(true)
                .meta("Click to link a folder")
                .on_click(move |_, cx| {
                    link.update(cx, |_, cx| cx.emit(Event::Link)).ok();
                });
            return vec![Branch::new(card)];
        }
        let projects: Vec<Branch> = atlas
            .projects()
            .iter()
            .filter(|project| shown(atlas, Place::Node((*project).clone()), &id(project)))
            .map(|project| {
                let node = Node::dir(project);
                let act = Act::Isolate(Some(project.clone()));
                let card = self.node_card(atlas, &node, false, act, view, cx);
                Branch::new(unlinkable(self.hideable(card, &node, view), project, view))
            })
            .collect();
        if projects.is_empty() {
            let card = Card::new("note:workspace", "All projects hidden").muted(true);
            return vec![Branch::new(card)];
        }
        projects
    }

    // The isolated project, open, and clicking it goes back to all of them.
    fn project(&self, atlas: &Atlas, root: &Path, view: &WeakEntity<Self>, cx: &App) -> Branch {
        let node = Node::dir(root);
        let card = self.node_card(atlas, &node, true, Act::Isolate(None), view, cx);
        Branch::new(unlinkable(card, root, view))
            .open(true)
            .children(self.entries(atlas, root, view, cx))
    }

    // What a folder holds, as shown: its folders, then its files changed in git, then the rest,
    // each block a little apart; a crowded folder piles the rest until it is spread out.
    fn entries(&self, atlas: &Atlas, dir: &Path, view: &WeakEntity<Self>, cx: &App) -> Vec<Branch> {
        let children = atlas.children(dir).unwrap_or_default();
        if children.is_empty() {
            return vec![note(dir, "Empty folder")];
        }
        let shown: Vec<Node> = children
            .iter()
            .filter(|node| {
                shown(
                    atlas,
                    Place::Node(node.path().to_path_buf()),
                    &id(node.path()),
                )
            })
            .cloned()
            .collect();
        if shown.is_empty() {
            return vec![note(dir, "All hidden")];
        }
        let repos = self.repos.read(cx);
        let changed = |node: &Node| repos.change_of(node.path()).is_some();
        let column = column(&shown, changed, atlas.is_spread(dir));
        let mut branches = Vec::new();
        for block in column.blocks {
            for (ix, item) in block.into_iter().enumerate() {
                let branch = match item {
                    Item::Node(node) => self.node(atlas, &node, view, cx),
                    Item::Pile(pile) => self.piled(dir, &pile, view),
                };
                branches.push(branch.apart(ix == 0));
            }
        }
        if let Some(count) = column.spread {
            branches.push(fold(dir, count, view));
        }
        branches
    }

    // A pile of the files of one kind in a crowded folder, a Show all chip under it showing them
    // all whatever it shows. The wheel turns it to each of its files and back to the cover, a
    // click opening the file it shows; on the cover a click shows them all too.
    fn piled(&self, dir: &Path, pile: &Pile, view: &WeakEntity<Self>) -> Branch {
        let files = pile.files();
        let key = SharedString::from(format!("pile:{}:{}", dir.display(), pile.kind()));
        let count = files.len();
        let at = self.turned.get(&key).copied().unwrap_or_default() % (count + 1);
        let card = match at.checked_sub(1).and_then(|ix| files.get(ix)) {
            None => {
                let first = files
                    .first()
                    .map(|file| file.name().to_owned())
                    .unwrap_or_default();
                let title = format!("{count} {} files", pile.kind());
                let card = Card::new(key.clone(), title)
                    .icon(move |size| FileIcon::new(first.clone()).size(size));
                clicks(card, Act::Spread(dir.to_path_buf()), view)
            }
            Some(file) => {
                let name = file.name().to_owned();
                let meta = format!("{at} of {count} {} files", pile.kind());
                let (opener, path) = (view.clone(), file.path().to_path_buf());
                Card::new(key.clone(), name.clone())
                    .icon(move |size| FileIcon::new(name.clone()).size(size))
                    .meta(meta)
                    .on_click(move |_, cx| {
                        let path = path.clone();
                        opener.update(cx, |_, cx| cx.emit(Event::Open(path))).ok();
                    })
            }
        };
        let spread = || run(Act::Spread(dir.to_path_buf()), view);
        let turner = view.clone();
        let card = card
            .pile("Show all", spread())
            .action("Show all", Glyph::Expand, spread())
            .on_turn(move |by, _, cx| {
                let key = key.clone();
                turner
                    .update(cx, |this, cx| this.turn(key, by, count, cx))
                    .ok();
            });
        Branch::new(card)
    }

    // Turns a pile of `count` files by `by`, its cover counting as one more.
    fn turn(&mut self, key: SharedString, by: i32, count: usize, cx: &mut Context<Self>) {
        let at = self.turned.entry(key).or_default();
        let sides = count as i64 + 1;
        *at = (*at as i64 + i64::from(by)).rem_euclid(sides) as usize;
        cx.notify();
    }

    // Only a folder opens on the home; a file opens in the code.
    fn node(&self, atlas: &Atlas, node: &Node, view: &WeakEntity<Self>, cx: &App) -> Branch {
        let path = node.path();
        let open = node.is_dir() && atlas.is_open(path);
        let card = self.node_card(atlas, node, open, Act::Toggle(node.clone()), view, cx);
        let branch = Branch::new(self.hideable(card, node, view)).open(open);
        if !open {
            return branch;
        }
        branch.children(self.entries(atlas, path, view, cx))
    }

    fn node_card(
        &self,
        atlas: &Atlas,
        node: &Node,
        open: bool,
        act: Act,
        view: &WeakEntity<Self>,
        cx: &App,
    ) -> Card {
        let path = node.path();
        let name = node.name().to_owned();
        let mut card = Card::new(id(path), name.clone());
        // A changed file stands out with a fill; a folder takes the color of what changed in it.
        if let Some(change) = self.repos.read(cx).change_of(path) {
            card = card.tint(tint(change, cx), !node.is_dir());
        }
        if node.is_dir() {
            card = card
                .icon(move |size| FolderIcon::new(name.clone()).open(open).size(size))
                .folder(whereabouts(atlas, path), self.size(atlas, path));
            if let Some(children) = atlas.children(path) {
                card = card.meta(summary(children));
            }
            card = folder_menu(card, atlas, path, view);
        } else {
            // A click opens the file's code.
            let (opener, file) = (view.clone(), path.to_path_buf());
            let card = card
                .icon(move |size| FileIcon::new(name.clone()).size(size))
                .on_click(move |_, cx| {
                    let file = file.clone();
                    opener.update(cx, |_, cx| cx.emit(Event::Open(file))).ok();
                });
            let card = file_menu(card, atlas, path);
            return self.grouped(card, atlas, view);
        }
        clicks(self.grouped(card, atlas, view), act, view)
    }

    fn hideable(&self, card: Card, node: &Node, view: &WeakEntity<Self>) -> Card {
        let place = Place::Node(node.path().to_path_buf());
        let name = node.name().to_owned();
        self.places
            .borrow_mut()
            .insert(card.id().clone(), (place.clone(), name.clone()));
        card.action("Hide", Glyph::EyeOff, run(Act::Hide(place, name), view))
    }

    // A card in a group can be taken out of it.
    fn grouped(&self, card: Card, atlas: &Atlas, view: &WeakEntity<Self>) -> Card {
        if atlas.group_of(card.id()).is_none() {
            return card;
        }
        let act = Act::Leave(card.id().to_string());
        card.action("Remove from group", Glyph::LogOut, run(act, view))
    }

    // A group as the graph draws it, with its title open to typing while it is being named.
    fn group(&self, group: &Group, view: &WeakEntity<Self>) -> graph::Group {
        let id = group.id();
        let collapsed = group.is_collapsed();
        let members = group
            .members()
            .iter()
            .map(|key| SharedString::from(key.clone()));
        let mut shown = graph::Group::new(format!("group:{id}"), group.name().to_owned(), members)
            .collapsed(collapsed);
        if let Some((naming, state, _)) = &self.naming
            && *naming == id
        {
            shown = shown.editor(field::compact(state));
        }
        if collapsed {
            shown = shown.on_click(run(Act::Fold(id, false), view));
        }
        let (fold, glyph) = if collapsed {
            ("Expand", Glyph::Expand)
        } else {
            ("Collapse", Glyph::Shrink)
        };
        let rename = view.clone();
        shown
            .action("Rename", Glyph::Pencil, move |window, cx| {
                rename.update(cx, |this, cx| this.name(id, window, cx)).ok();
            })
            .action(fold, glyph, run(Act::Fold(id, !collapsed), view))
            .action("Hide group", Glyph::EyeOff, run(Act::HideGroup(id), view))
            .action("Ungroup", Glyph::Ungroup, run(Act::Ungroup(id), view))
    }
}

impl Render for Overview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.places.borrow_mut().clear();
        let view = cx.entity().downgrade();
        let atlas = self.bench.read(cx).atlas();
        let roots = match atlas.root() {
            Some(root) => vec![self.project(atlas, root, &view, cx)],
            None => self.workspace(atlas, &view, cx),
        };
        let nudges = atlas
            .nudges()
            .iter()
            .map(|(key, by)| (SharedString::from(key.clone()), *by))
            .collect();
        let groups: Vec<graph::Group> = atlas
            .groups()
            .iter()
            .filter(|group| !group.is_hidden())
            .map(|group| self.group(group, &view))
            .collect();
        let (moved, grouped, hidden, tidy) = (view.clone(), view.clone(), view.clone(), view);
        Graph::new("overview", &self.camera, roots)
            .focus(&self.focus)
            .nudges(nudges)
            .groups(groups)
            .on_move(move |moves, window, cx| {
                let moves = moves
                    .into_iter()
                    .map(|(key, by)| (key.to_string(), by))
                    .collect();
                moved
                    .update(cx, |this, cx| this.act(&Act::Move(moves), window, cx))
                    .ok();
            })
            .on_group(move |keys, window, cx| {
                let keys = keys.iter().map(ToString::to_string).collect();
                grouped
                    .update(cx, |this, cx| this.act(&Act::Group(keys), window, cx))
                    .ok();
            })
            .on_hide(move |keys, window, cx| {
                hidden
                    .update(cx, |this, cx| this.hide_cards(&keys, window, cx))
                    .ok();
            })
            .on_reorganize(move |window, cx| {
                tidy.update(cx, |this, cx| this.reorganize(window, cx)).ok();
            })
    }
}

fn run(act: Act, view: &WeakEntity<Overview>) -> impl Fn(&mut Window, &mut App) + 'static {
    let view = view.clone();
    move |window, cx| {
        view.update(cx, |this, cx| this.act(&act, window, cx)).ok();
    }
}

fn clicks(card: Card, act: Act, view: &WeakEntity<Overview>) -> Card {
    card.on_click(run(act, view))
}

// The chip at the end of a crowded folder spread out, which piles its files back.
fn fold(dir: &Path, count: usize, view: &WeakEntity<Overview>) -> Branch {
    let card = Card::new(
        format!("fold:{}", dir.display()),
        format!("Collapse {count} files"),
    )
    .icon(|size| Icon::new(Glyph::ChevronUp).size(size))
    .chip();
    Branch::new(clicks(card, Act::Pile(dir.to_path_buf()), view))
}

fn unlinkable(card: Card, project: &Path, view: &WeakEntity<Overview>) -> Card {
    let view = view.clone();
    let project = project.to_path_buf();
    card.action("Unlink", Glyph::Unlink, move |_, cx| {
        let project = project.clone();
        view.update(cx, |_, cx| cx.emit(Event::Unlink(project)))
            .ok();
    })
}

// Neither hidden itself nor in a hidden group.
fn shown(atlas: &Atlas, place: Place, key: &str) -> bool {
    !atlas.is_hidden(&place) && !atlas.is_veiled(key)
}

fn note(under: &Path, text: &str) -> Branch {
    let card = Card::new(format!("note:{}", under.display()), text.to_owned())
        .muted(true)
        .selectable(false);
    Branch::new(card)
}

// How many characters of a path a folder's card shows.
const PATH: usize = 30;

// Where a folder is, as its card shows it: a project by its whole path, any other folder from
// its project's name down, cut at the front when too long.
fn whereabouts(atlas: &Atlas, path: &Path) -> String {
    let shown = match owner(path, atlas.projects()) {
        Some(project) if project != path => {
            let name = project
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let rest = path.strip_prefix(project).unwrap_or(path);
            format!("{name}/{}", rest.display())
        }
        _ => tilde(path),
    };
    path::tail(&shown, PATH)
}

// The path from the project down, none for the project itself.
fn relative(atlas: &Atlas, path: &Path) -> Option<String> {
    let project = owner(path, atlas.projects())?;
    let rest = path.strip_prefix(project).ok()?;
    (!rest.as_os_str().is_empty()).then(|| rest.display().to_string())
}

// What a right click on a folder offers before hiding it, in two menus: Copy, its path in full
// or from its project; Open, the folder in the file manager or a terminal in it.
fn folder_menu(card: Card, atlas: &Atlas, path: &Path, view: &WeakEntity<Overview>) -> Card {
    let (shown, inside) = (path.to_path_buf(), path.to_path_buf());
    let view = view.clone();
    let open = Submenu::new("Open", Glyph::FolderOpen)
        .entry("In file manager", Glyph::FolderOpen, move |_, cx| {
            cx.open_with_system(&shown);
        })
        .entry("Terminal here", Glyph::SquareTerminal, move |_, cx| {
            let inside = inside.clone();
            view.update(cx, |_, cx| cx.emit(Event::Terminal(inside)))
                .ok();
        });
    card.submenu(copies(atlas, path)).submenu(open).separator()
}

// What a right click on a file offers before hiding it: Copy, as for a folder, and Open, its
// folder in the file manager with it picked there.
fn file_menu(card: Card, atlas: &Atlas, path: &Path) -> Card {
    let file = path.to_path_buf();
    let open = Submenu::new("Open", Glyph::FolderOpen).entry(
        "Containing folder",
        Glyph::FolderOpen,
        move |_, cx| {
            cx.reveal_path(&file);
        },
    );
    card.submenu(copies(atlas, path)).submenu(open).separator()
}

// The path in full, and from the project down when it is inside one.
fn copies(atlas: &Atlas, path: &Path) -> Submenu {
    let full = path.display().to_string();
    let copies = Submenu::new("Copy", Glyph::Copy).entry("Path", Glyph::Copy, move |window, cx| {
        copy(&full, window, cx);
    });
    match relative(atlas, path) {
        Some(rest) => copies.entry("Relative path", Glyph::Copy, move |window, cx| {
            copy(&rest, window, cx);
        }),
        None => copies,
    }
}

fn copy(text: &str, window: &mut Window, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
    notice::info(format!("Copied {text}"), window, cx);
}

fn summary(children: &[Node]) -> String {
    let dirs = children.iter().filter(|node| node.is_dir()).count();
    let files = children.len() - dirs;
    let part = |count: usize, one: &str, many: &str| match count {
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    };
    match (dirs, files) {
        (0, 0) => "Empty".to_owned(),
        (0, files) => part(files, "file", "files"),
        (dirs, 0) => part(dirs, "folder", "folders"),
        (dirs, files) => format!(
            "{} · {}",
            part(dirs, "folder", "folders"),
            part(files, "file", "files")
        ),
    }
}

fn id(path: &Path) -> SharedString {
    SharedString::from(path.to_string_lossy().into_owned())
}
