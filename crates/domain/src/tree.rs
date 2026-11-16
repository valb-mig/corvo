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
}
