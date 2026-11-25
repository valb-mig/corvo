use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::arrangement::{Arrangement, Group, Mark};
use crate::server::Registry;
use crate::{Node, Symbol, symbol};

// The home of a workspace: its projects, or one project isolated as the root, with any number
// of its folders and files open at once. An open folder shows what it holds, an open file its
// outline, and an open symbol the symbols inside it. A symbol is known by its file and the
// indexes that lead to it through the outline. Any card can be hidden, and stays hidden until
// shown again, whatever is isolated or open. Cards moved by hand keep how far they were moved, and
// cards can be put together in groups; both are known by the key the home gives each card.

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
    // What is hidden, with its name to tell it by and how it is kept between runs.
    hidden: BTreeMap<Place, (String, Option<Mark>)>,
    nudges: BTreeMap<String, (f32, f32)>,
    groups: Vec<Group>,
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
        let mark = match &place {
            Place::Node(path) => {
                self.close(path);
                Some(Mark::Node(path.clone()))
            }
            Place::Symbol(file, at) => {
                self.surface(file, at);
                self.trail(file, at)
                    .map(|trail| Mark::Symbol(file.clone(), trail))
            }
        };
        self.hidden.insert(place, (name.into(), mark));
    }

    pub fn show(&mut self, place: &Place) {
        self.hidden.remove(place);
    }

    // Every hidden card and group back.
    pub fn show_all(&mut self) {
        self.hidden.clear();
        for group in &mut self.groups {
            group.veil(false);
        }
    }

    pub fn is_hidden(&self, place: &Place) -> bool {
        self.hidden.contains_key(place)
    }

    pub fn hidden(&self) -> impl Iterator<Item = (&Place, &str)> {
        self.hidden
            .iter()
            .map(|(place, (name, _))| (place, name.as_str()))
    }

    // How many cards and groups are off the home.
    pub fn hidden_count(&self) -> usize {
        self.hidden.len() + self.groups.iter().filter(|group| group.is_hidden()).count()
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

    // The trail of names that leads to a symbol, which holds while its lines move.
    pub fn trail(&self, file: &Path, at: &[usize]) -> Option<Vec<String>> {
        symbol::trail(self.outlines.get(file)?, at)
    }

    pub fn find(&self, file: &Path, trail: &[String]) -> Option<Vec<usize>> {
        symbol::find(self.outlines.get(file)?, trail)
    }

    pub fn nudges(&self) -> &BTreeMap<String, (f32, f32)> {
        &self.nudges
    }

    // How far a card was moved by hand from where the layout puts it.
    pub fn nudge(&mut self, key: impl Into<String>, by: (f32, f32)) {
        self.nudges.insert(key.into(), by);
    }

    // Every card back where the layout puts it; groups stay.
    pub fn untangle(&mut self) {
        self.nudges.clear();
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn group_of(&self, key: &str) -> Option<&Group> {
        self.groups.iter().find(|group| group.holds(key))
    }

    // Puts cards together in a new group, out of any group they were in, and gives its id.
    pub fn group(&mut self, keys: Vec<String>) -> Option<u64> {
        if keys.is_empty() {
            return None;
        }
        for key in &keys {
            self.leave(key);
        }
        let id = self.groups.iter().map(Group::id).max().unwrap_or(0) + 1;
        self.groups
            .push(Group::new(id, format!("Group {id}"), keys));
        Some(id)
    }

    pub fn ungroup(&mut self, id: u64) {
        self.groups.retain(|group| group.id() != id);
    }

    // A blank name leaves the group as it was.
    pub fn rename(&mut self, id: u64, name: &str) {
        let name = name.trim();
        if let Some(group) = self.group_mut(id)
            && !name.is_empty()
        {
            group.rename(name.to_owned());
        }
    }

    pub fn fold(&mut self, id: u64, collapsed: bool) {
        if let Some(group) = self.group_mut(id) {
            group.fold(collapsed);
        }
    }

    pub fn hide_group(&mut self, id: u64) {
        if let Some(group) = self.group_mut(id) {
            group.veil(true);
        }
    }

    pub fn show_group(&mut self, id: u64) {
        if let Some(group) = self.group_mut(id) {
            group.veil(false);
        }
    }

    // Takes a card out of its group; a group left empty goes.
    pub fn leave(&mut self, key: &str) {
        for group in &mut self.groups {
            group.drop_member(key);
        }
        self.groups.retain(|group| !group.members().is_empty());
    }

    // The card is in a hidden group, so it is off the home.
    pub fn is_veiled(&self, key: &str) -> bool {
        self.groups
            .iter()
            .any(|group| group.is_hidden() && group.holds(key))
    }

    // How the home is now, to come back to it the same.
    pub fn arrangement(&self) -> Arrangement {
        let dives = self
            .dives
            .iter()
            .filter_map(|(file, at)| Some((file.clone(), self.trail(file, at)?)))
            .collect();
        let hidden = self
            .hidden
            .values()
            .filter_map(|(name, mark)| Some((mark.clone()?, name.clone())))
            .collect();
        Arrangement {
            root: self.root.clone(),
            open: self.open.values().cloned().collect(),
            dives,
            hidden,
            nudges: self.nudges.clone(),
            groups: self.groups.clone(),
        }
    }

    // Takes back where cards were moved and the groups made of them.
    pub fn rearrange(&mut self, nudges: BTreeMap<String, (f32, f32)>, groups: Vec<Group>) {
        self.nudges = nudges;
        self.groups = groups;
    }

    fn group_mut(&mut self, id: u64) -> Option<&mut Group> {
        self.groups.iter_mut().find(|group| group.id() == id)
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
