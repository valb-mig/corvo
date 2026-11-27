use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::Node;

#[derive(Debug)]
pub struct Tree {
    root: PathBuf,
    nodes: HashMap<PathBuf, Vec<Node>>,
    open: HashSet<PathBuf>,
}

impl Tree {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            nodes: HashMap::new(),
            open: HashSet::new(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn fill(&mut self, dir: &Path, mut nodes: Vec<Node>) {
        nodes.sort();
        self.nodes.insert(dir.to_path_buf(), nodes);
    }

    pub fn children(&self, dir: &Path) -> Option<&[Node]> {
        self.nodes.get(dir).map(Vec::as_slice)
    }

    pub fn is_loaded(&self, dir: &Path) -> bool {
        self.nodes.contains_key(dir)
    }

    pub fn open(&mut self, dir: &Path) {
        self.open.insert(dir.to_path_buf());
    }

    pub fn close(&mut self, dir: &Path) {
        self.open.remove(dir);
    }

    pub fn is_open(&self, dir: &Path) -> bool {
        self.open.contains(dir)
    }

    // Drops what was read of a folder that went, and of the folders inside it.
    pub fn forget(&mut self, path: &Path) {
        self.nodes.retain(|dir, _| !dir.starts_with(path));
        self.open.retain(|dir| !dir.starts_with(path));
    }

    // A folder moved: what of it was open is forgotten where it was, and the folders that were
    // open come back where it went, in order from the outermost, to be read again there.
    pub fn moved(&mut self, from: &Path, to: &Path) -> Vec<PathBuf> {
        let mut open: Vec<PathBuf> = self
            .open
            .iter()
            .filter_map(|dir| dir.strip_prefix(from).ok())
            .map(|rest| crate::tabs::under(to, rest))
            .collect();
        open.sort();
        self.forget(from);
        open
    }
}
