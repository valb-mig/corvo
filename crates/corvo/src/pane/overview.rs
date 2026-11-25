use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use corvo_domain::server::Registry;
use corvo_domain::{Atlas, Node, Place, Symbol, SymbolKind, Workbench, stack};
use corvo_ui::file_icon::{FileIcon, FolderIcon};
use corvo_ui::graph::{Branch, Camera, Card, Graph};
use corvo_ui::notice;
use corvo_ui::symbol_icon::SymbolIcon;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    Context, Entity, EventEmitter, IntoElement, Render, SharedString, Styled as _, WeakEntity,
    Window,
};

use crate::pane::detail::{self, Code, Key, Mentions, Panel};

pub enum Event {
    // A code panel wrote the file, which now holds this text.
    Saved(PathBuf, String),
    // A project stands alone as the root now, or none does.
    Isolated(Option<PathBuf>),
    // Asked from a card's menu: link a folder to the workspace, or unlink a project.
    Link,
    Unlink(PathBuf),
}

// What a click on a card does.
#[derive(Clone)]
enum Act {
    Isolate(Option<PathBuf>),
    Toggle(Node),
    Dive(PathBuf, Vec<usize>),
    Hide(Place, String),
    // One hidden card back, or all of them.
    Show(Option<Place>),
}

// The home of the workspace: the workspace and its projects, or one project as the root with
// any number of its folders, files and symbols open, as a tree read left to right. An open
// function drops its code down from under its header, an open variable the lines that mention
// it.
pub struct Overview {
    bench: Entity<Workbench>,
    registry: Registry,
    name: SharedString,
    // The code panels of open symbols, and of closed ones with unsaved edits, to come back with
    // them.
    codes: HashMap<Key, Code>,
    mentions: HashMap<(PathBuf, Vec<usize>), Mentions>,
    camera: Camera,
}

impl EventEmitter<Event> for Overview {}

impl Overview {
    pub fn new(bench: Entity<Workbench>, registry: Registry) -> Self {
        Self {
            bench,
            registry,
            name: SharedString::default(),
            codes: HashMap::new(),
            mentions: HashMap::new(),
            camera: Camera::default(),
        }
    }

    pub fn chart(
        &mut self,
        name: impl Into<SharedString>,
        projects: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.name = name.into();
        let charted = self.bench.update(cx, |bench, _| bench.chart(projects));
        self.report(charted, window, cx);
    }

    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let charted = self.bench.update(cx, |bench, _| bench.rechart());
        self.report(charted, window, cx);
    }

    // Writes the code panel being typed in back into its file; with none focused, the only one
    // with unsaved edits.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self
            .codes
            .iter()
            .find(|(_, code)| code.is_focused(window, cx))
            .map(|(key, _)| key.clone());
        let key = focused.or_else(|| {
            let mut dirty = self.codes.iter().filter(|(_, code)| code.is_dirty(cx));
            match (dirty.next(), dirty.next()) {
                (Some((key, _)), None) => Some(key.clone()),
                _ => None,
            }
        });
        let Some((key, code)) = key.and_then(|key| self.codes.get(&key).map(|code| (key, code)))
        else {
            return;
        };
        if code.is_locked() || !code.is_dirty(cx) {
            return;
        }
        let edited = code.edited(cx);
        let file = code.file().to_path_buf();
        let (first, last) = code.lines();
        let original = code.original().to_owned();
        let spliced = self.bench.update(cx, |bench, _| {
            bench.splice(&file, first, last, &original, &edited)
        });
        match spliced {
            Ok(text) => {
                if let Some(code) = self.codes.get_mut(&key) {
                    code.saved(edited);
                }
                // The other clean panels of the file read their lines again.
                self.codes
                    .retain(|other, code| *other == key || other.0 != file || code.is_dirty(cx));
                cx.emit(Event::Saved(file, text));
                self.refresh(window, cx);
            }
            Err(err) => notice::error(format!("{}: {err}", file.display()), window, cx),
        }
    }

    // A tab wrote the file: its clean code panels read it again.
    pub fn saved(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.codes
            .retain(|key, code| key.0 != path || code.is_dirty(cx));
        self.refresh(window, cx);
    }

    // Every card back where the layout puts it, the whole graph centred in view.
    pub fn reorganize(&mut self, cx: &mut Context<Self>) {
        self.camera.reorganize();
        cx.notify();
    }

    pub fn show_hidden(
        &mut self,
        place: Option<Place>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.act(&Act::Show(place), window, cx);
    }

    fn act(&mut self, act: &Act, window: &mut Window, cx: &mut Context<Self>) {
        let isolated = match act {
            Act::Isolate(root) => Some(root.clone()),
            _ => None,
        };
        let done = self.bench.update(cx, |bench, _| match act {
            Act::Isolate(root) => bench.isolate(root.as_deref()),
            Act::Toggle(node) => bench.toggle(node),
            Act::Dive(file, at) => {
                bench.toggle_symbol(file, at);
                Ok(())
            }
            Act::Hide(place, name) => {
                bench.hide(place.clone(), name);
                Ok(())
            }
            Act::Show(place) => {
                bench.show_hidden(place.as_ref());
                Ok(())
            }
        });
        if let (Ok(()), Some(root)) = (&done, isolated) {
            cx.emit(Event::Isolated(root));
        }
        self.report(done, window, cx);
    }

    fn report(&mut self, done: std::io::Result<()>, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = done {
            notice::error(err.to_string(), window, cx);
        }
        self.follow(window, cx);
        cx.notify();
    }

    // Matches the panels to the open symbols.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut codes = Vec::new();
        let mut mentions = Vec::new();
        let atlas = self.bench.read(cx).atlas();
        for (file, at) in atlas.dives() {
            let Some(chain) = atlas.symbols_at(file, at) else {
                continue;
            };
            let Some((symbol, rest)) = chain.split_last() else {
                continue;
            };
            let parent = rest.last().map(|parent| (*parent).clone());
            match detail::panel(symbol.kind()) {
                Some(Panel::Code) => codes.push((file.to_path_buf(), (*symbol).clone())),
                Some(Panel::Mentions) => {
                    let place = (file.to_path_buf(), at.to_vec());
                    mentions.push((place, (*symbol).clone(), parent));
                }
                None => {}
            }
        }

        let open: HashSet<Key> = codes
            .iter()
            .map(|(file, symbol)| detail::key(file, symbol))
            .collect();
        self.codes
            .retain(|key, code| open.contains(key) || code.is_dirty(cx));
        for (file, symbol) in codes {
            let key = detail::key(&file, &symbol);
            if self.codes.contains_key(&key) {
                continue;
            }
            match Code::open(&self.bench, &file, &symbol, window, cx) {
                Ok(code) => {
                    self.codes.insert(key, code);
                }
                Err(err) => notice::error(format!("{}: {err}", file.display()), window, cx),
            }
        }

        self.mentions.clear();
        for (place, symbol, parent) in mentions {
            let found = Mentions::find(self.bench.read(cx), &place.0, &symbol, parent.as_ref());
            match found {
                Ok(found) => {
                    self.mentions.insert(place, found);
                }
                Err(err) => notice::error(format!("{}: {err}", place.0.display()), window, cx),
            }
        }
    }

    fn workspace(&self, atlas: &Atlas, view: &WeakEntity<Self>, cx: &Context<Self>) -> Branch {
        let count = atlas.projects().len();
        let meta = match count {
            0 => "No projects linked".to_owned(),
            1 => "1 project".to_owned(),
            n => format!("{n} projects"),
        };
        let tint = cx.theme().muted_foreground;
        let link = view.clone();
        let card = Card::new("workspace", self.name.clone())
            .icon(move |size| Icon::new(Glyph::LayoutGrid).size(size).text_color(tint))
            .meta(meta)
            .action("Link folder…", Glyph::FolderPlus, move |_, cx| {
                link.update(cx, |_, cx| cx.emit(Event::Link)).ok();
            });
        // Picking a project stands it alone as the root.
        let projects = atlas
            .projects()
            .iter()
            .filter(|project| !atlas.is_hidden(&Place::Node((*project).clone())))
            .map(|project| {
                let node = Node::dir(project);
                let act = Act::Isolate(Some(project.clone()));
                let card = self.node_card(atlas, &node, false, act, view);
                Branch::new(unlinkable(hideable(card, &node, view), project, view))
            });
        Branch::new(card).open(true).children(projects)
    }

    // The isolated project, open, and clicking it goes back to the workspace.
    fn project(
        &self,
        atlas: &Atlas,
        root: &Path,
        view: &WeakEntity<Self>,
        cx: &Context<Self>,
    ) -> Branch {
        let node = Node::dir(root);
        let card = self.node_card(atlas, &node, true, Act::Isolate(None), view);
        Branch::new(unlinkable(card, root, view))
            .open(true)
            .children(self.entries(atlas, root, view, cx))
    }

    fn entries(
        &self,
        atlas: &Atlas,
        dir: &Path,
        view: &WeakEntity<Self>,
        cx: &Context<Self>,
    ) -> Vec<Branch> {
        let children = atlas.children(dir).unwrap_or_default();
        if children.is_empty() {
            return vec![note(dir, "Empty folder")];
        }
        let shown: Vec<Branch> = children
            .iter()
            .filter(|node| !atlas.is_hidden(&Place::Node(node.path().to_path_buf())))
            .map(|node| self.node(atlas, node, view, cx))
            .collect();
        if shown.is_empty() {
            return vec![note(dir, "All hidden")];
        }
        shown
    }

    fn node(
        &self,
        atlas: &Atlas,
        node: &Node,
        view: &WeakEntity<Self>,
        cx: &Context<Self>,
    ) -> Branch {
        let path = node.path();
        let open = atlas.is_open(path);
        let card = self.node_card(atlas, node, open, Act::Toggle(node.clone()), view);
        let branch = Branch::new(hideable(card, node, view)).open(open);
        if !open {
            return branch;
        }
        if node.is_dir() {
            return branch.children(self.entries(atlas, path, view, cx));
        }
        match atlas.outline(path) {
            Some(symbols) if !symbols.is_empty() => {
                branch.children(self.symbols(atlas, path, symbols, &[], view, cx))
            }
            _ => branch.children([note(path, "No structure")]),
        }
    }

    fn symbols(
        &self,
        atlas: &Atlas,
        file: &Path,
        symbols: &[Symbol],
        prefix: &[usize],
        view: &WeakEntity<Self>,
        cx: &Context<Self>,
    ) -> Vec<Branch> {
        symbols
            .iter()
            .enumerate()
            .map(|(ix, symbol)| (symbol, [prefix, &[ix]].concat()))
            .filter(|(_, at)| !atlas.is_hidden(&Place::Symbol(file.to_path_buf(), at.clone())))
            .map(|(symbol, at)| {
                let open = atlas.is_dived(file, &at);
                let mut card = self.symbol_card(file, symbol, &at, view);
                if !open {
                    return Branch::new(card);
                }
                let panel = match detail::panel(symbol.kind()) {
                    Some(Panel::Code) => self
                        .codes
                        .get(&detail::key(file, symbol))
                        .map(|code| code.element(cx)),
                    Some(Panel::Mentions) => self
                        .mentions
                        .get(&(file.to_path_buf(), at.clone()))
                        .map(|found| found.element(cx)),
                    None => None,
                };
                if let Some((panel, width, height)) = panel {
                    card = card.dropdown(panel, width, height);
                }
                let nested = self.symbols(atlas, file, symbol.nested(), &at, view, cx);
                Branch::new(card).open(true).children(nested)
            })
            .collect()
    }

    fn node_card(
        &self,
        atlas: &Atlas,
        node: &Node,
        open: bool,
        act: Act,
        view: &WeakEntity<Self>,
    ) -> Card {
        let path = node.path();
        let name = node.name().to_owned();
        let mut card = Card::new(id(path), name.clone());
        if node.is_dir() {
            card = card.icon(move |size| FolderIcon::new(name.clone()).open(open).size(size));
            if let Some(children) = atlas.children(path) {
                card = card.meta(summary(children));
                if let Some(stack) = stack(children, &self.registry) {
                    card = card.badge(stack.to_owned());
                }
            }
        } else {
            card = card.icon(move |size| FileIcon::new(name.clone()).size(size));
        }
        clicks(card, act, view)
    }

    fn symbol_card(
        &self,
        file: &Path,
        symbol: &Symbol,
        at: &[usize],
        view: &WeakEntity<Self>,
    ) -> Card {
        let (kind, line) = (symbol.kind(), symbol.line());
        let mut meta = format!("{} · line {}", kind_name(kind), line + 1);
        if !symbol.nested().is_empty() {
            meta.push_str(&format!(" · {} inside", symbol.nested().len()));
        }
        let card = Card::new(
            format!("{}#{line}:{}", file.display(), symbol.column()),
            symbol.name().to_owned(),
        )
        .icon(move |size| SymbolIcon::new(kind).size(size))
        .meta(meta);
        let place = Place::Symbol(file.to_path_buf(), at.to_vec());
        let card = with_hide(card, place, symbol.name(), view);
        clicks(card, Act::Dive(file.to_path_buf(), at.to_vec()), view)
    }
}

impl Render for Overview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let atlas = self.bench.read(cx).atlas();
        let root = match atlas.root() {
            Some(root) => self.project(atlas, root, &view, cx),
            None => self.workspace(atlas, &view, cx),
        };
        Graph::new("overview", &self.camera, root)
    }
}

fn clicks(card: Card, act: Act, view: &WeakEntity<Overview>) -> Card {
    let view = view.clone();
    card.on_click(move |window, cx| {
        view.update(cx, |this, cx| this.act(&act, window, cx)).ok();
    })
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

fn hideable(card: Card, node: &Node, view: &WeakEntity<Overview>) -> Card {
    let place = Place::Node(node.path().to_path_buf());
    with_hide(card, place, node.name(), view)
}

fn with_hide(card: Card, place: Place, name: &str, view: &WeakEntity<Overview>) -> Card {
    let view = view.clone();
    let act = Act::Hide(place, name.to_owned());
    card.action("Hide", Glyph::EyeOff, move |window, cx| {
        view.update(cx, |this, cx| this.act(&act, window, cx)).ok();
    })
}

fn note(under: &Path, text: &str) -> Branch {
    let card = Card::new(format!("note:{}", under.display()), text.to_owned()).muted(true);
    Branch::new(card)
}

fn kind_name(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Module => "module",
        SymbolKind::Class => "class",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Interface => "interface",
        SymbolKind::Impl => "impl",
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Field => "field",
        SymbolKind::Variant => "variant",
        SymbolKind::Constant => "constant",
        SymbolKind::Variable => "variable",
        SymbolKind::Parameter => "parameter",
        SymbolKind::Type => "type",
    }
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
