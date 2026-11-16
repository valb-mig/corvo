use std::io;
use std::path::{Path, PathBuf};

use crate::port::Fs;
use crate::{Tabs, Tree};

pub struct Workbench {
    fs: Box<dyn Fs>,
    tree: Tree,
    tabs: Tabs,
}

impl Workbench {
    pub fn new(root: impl Into<PathBuf>, fs: Box<dyn Fs>) -> io::Result<Self> {
        let root = root.into();
        let mut bench = Self {
            fs,
            tree: Tree::new(&root),
            tabs: Tabs::new(),
        };
        bench.expand(&root)?;
        Ok(bench)
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn tabs(&self) -> &Tabs {
        &self.tabs
    }

    pub fn expand(&mut self, dir: &Path) -> io::Result<()> {
        if !self.tree.is_loaded(dir) {
            let nodes = self.fs.list(dir)?;
            self.tree.fill(dir, nodes);
        }
        self.tree.open(dir);
        Ok(())
    }

    pub fn collapse(&mut self, dir: &Path) {
        self.tree.close(dir);
    }

    pub fn open(&mut self, path: &Path) -> io::Result<Option<String>> {
        if self.tabs.contains(path) {
            self.tabs.focus(path);
            return Ok(None);
        }
        let text = self.fs.read(path)?;
        self.tabs.open(path);
        Ok(Some(text))
    }

    pub fn focus(&mut self, path: &Path) {
        self.tabs.focus(path);
    }

    pub fn close(&mut self, path: &Path) {
        self.tabs.close(path);
    }

    pub fn touch(&mut self, path: &Path) {
        self.tabs.touch(path);
    }

    pub fn save(&mut self, path: &Path, text: &str) -> io::Result<()> {
        self.fs.write(path, text)?;
        self.tabs.clean(path);
        Ok(())
    }
}
