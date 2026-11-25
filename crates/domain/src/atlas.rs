use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::server::Registry;
use crate::{Node, Symbol};

// The home of a workspace: its projects, or one project isolated as the root, with any number
// of its folders and files open at once. An open folder shows what it holds, an open file its
// outline, and an open symbol the symbols inside it. A symbol is known by its file and the
// indexes that lead to it through the outline. Any card can be hidden, and stays hidden until
// shown again, whatever is isolated or open.

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Place {
    Node(PathBuf),
    Symbol(PathBuf, Vec<usize>),
}

#[derive(Clone, Debug, Default)]
pub struct Atlas {
    projects: Vec<PathBuf>,
    root: Option<PathBuf>,
    open: BTreeMap<PathBuf, Node>,
    dives: BTreeSet<(PathBuf, Vec<usize>)>,
    // What is hidden, with its name to tell it by.
    hidden: BTreeMap<Place, String>,
    lists: HashMap<PathBuf, Vec<Node>>,
    outlines: HashMap<PathBuf, Vec<Symbol>>,
}

impl Atlas {
    pub fn projects(&self) -> &[PathBuf] {
        &self.projects
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn chart(&mut self, projects: &[PathBuf]) {
        self.projects = projects.to_vec();
        if self
            .root
            .as_ref()
            .is_some_and(|root| !projects.contains(root))
        {
            self.isolate(None);
        }
    }

    // Stands one project alone as the root, or goes back to all of them. Either way, nothing
    // is open anymore.
    pub fn isolate(&mut self, root: Option<&Path>) {
        self.root = root.map(Path::to_path_buf);
        self.open.clear();
        self.dives.clear();
    }

    pub fn open(&mut self, node: Node) {
        self.open.insert(node.path().to_path_buf(), node);
    }

    // Closes a folder or file with everything open inside it.
    pub fn close(&mut self, path: &Path) {
        self.open.retain(|open, _| !open.starts_with(path));
        self.dives.retain(|(file, _)| !file.starts_with(path));
    }

    pub fn is_open(&self, path: &Path) -> bool {
        self.open.contains_key(path)
    }

    pub fn opened(&self) -> impl Iterator<Item = &Node> {
        self.open.values()
    }

    // Hides a card, closing it first with everything open inside.
    pub fn hide(&mut self, place: Place, name: impl Into<String>) {
        match &place {
            Place::Node(path) => self.close(path),
            Place::Symbol(file, at) => self.surface(file, at),
        }
        self.hidden.insert(place, name.into());
    }

    pub fn show(&mut self, place: &Place) {
        self.hidden.remove(place);
    }

    pub fn show_all(&mut self) {
        self.hidden.clear();
    }

    pub fn is_hidden(&self, place: &Place) -> bool {
        self.hidden.contains_key(place)
    }

    pub fn hidden(&self) -> impl Iterator<Item = (&Place, &str)> {
        self.hidden
            .iter()
            .map(|(place, name)| (place, name.as_str()))
    }

    pub fn dive(&mut self, file: &Path, at: &[usize]) {
        self.dives.insert((file.to_path_buf(), at.to_vec()));
    }

    // Closes a symbol with every symbol open inside it.
    pub fn surface(&mut self, file: &Path, at: &[usize]) {
        self.dives
            .retain(|(open, path)| !(open == file && path.starts_with(at)));
    }

    pub fn is_dived(&self, file: &Path, at: &[usize]) -> bool {
        self.dives.contains(&(file.to_path_buf(), at.to_vec()))
    }

    pub fn dives(&self) -> impl Iterator<Item = (&Path, &[usize])> {
        self.dives
            .iter()
            .map(|(file, at)| (file.as_path(), at.as_slice()))
    }

    // The symbols down the indexes, outermost first; none when the outline no longer has them.
    pub fn symbols_at(&self, file: &Path, at: &[usize]) -> Option<Vec<&Symbol>> {
        let mut symbols = self.outlines.get(file)?.as_slice();
        let mut chain = Vec::with_capacity(at.len());
        for &ix in at {
            let symbol = symbols.get(ix)?;
            chain.push(symbol);
            symbols = symbol.nested();
        }
        Some(chain)
    }

    pub fn fill(&mut self, dir: &Path, mut nodes: Vec<Node>) {
        nodes.sort();
        self.lists.insert(dir.to_path_buf(), nodes);
    }

    pub fn trace(&mut self, file: &Path, symbols: Vec<Symbol>) {
        self.outlines.insert(file.to_path_buf(), symbols);
    }

    pub fn clear(&mut self) {
        self.lists.clear();
        self.outlines.clear();
    }

    pub fn is_loaded(&self, dir: &Path) -> bool {
        self.lists.contains_key(dir)
    }

    pub fn is_traced(&self, file: &Path) -> bool {
        self.outlines.contains_key(file)
    }

    pub fn children(&self, dir: &Path) -> Option<&[Node]> {
        self.lists.get(dir).map(Vec::as_slice)
    }

    pub fn outline(&self, file: &Path) -> Option<&[Symbol]> {
        self.outlines.get(file).map(Vec::as_slice)
    }
}

pub fn stack<'r>(children: &[Node], registry: &'r Registry) -> Option<&'r str> {
    registry.stack(|marker| {
        children
            .iter()
            .any(|node| !node.is_dir() && node.name() == marker)
    })
}
