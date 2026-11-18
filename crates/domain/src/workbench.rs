use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::port::Fs;
use crate::{Content, Tabs, Tree};

pub struct Workbench {
    fs: Box<dyn Fs>,
    trees: HashMap<PathBuf, Tree>,
    project: Option<PathBuf>,
    tabs: Tabs,
}

impl Workbench {
    pub fn new(fs: Box<dyn Fs>) -> Self {
        Self {
            fs,
            trees: HashMap::new(),
            project: None,
            tabs: Tabs::new(),
        }
    }

    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.trees.get(self.project.as_ref()?)
    }

    pub fn tabs(&self) -> &Tabs {
        &self.tabs
    }

    pub fn show(&mut self, project: Option<&Path>) -> io::Result<()> {
        let Some(root) = project else {
            self.project = None;
            return Ok(());
        };
        if !self.trees.contains_key(root) {
            let mut tree = Tree::new(root);
            tree.fill(root, self.fs.list(root)?);
            tree.open(root);
            self.trees.insert(root.to_path_buf(), tree);
        }
        self.project = Some(root.to_path_buf());
        Ok(())
    }

    pub fn forget(&mut self, project: &Path) {
        self.trees.remove(project);
        if self.project.as_deref() == Some(project) {
            self.project = None;
        }
    }

    pub fn expand(&mut self, dir: &Path) -> io::Result<()> {
        let Some(root) = &self.project else {
            return Ok(());
        };
        let Some(tree) = self.trees.get_mut(root) else {
            return Ok(());
        };
        if !tree.is_loaded(dir) {
            tree.fill(dir, self.fs.list(dir)?);
        }
        tree.open(dir);
        Ok(())
    }

    pub fn collapse(&mut self, dir: &Path) {
        if let Some(tree) = self.tree_mut() {
            tree.close(dir);
        }
    }

    pub fn reveal(&mut self, path: &Path) -> io::Result<()> {
        let Some(root) = self.project.clone() else {
            return Ok(());
        };
        let mut dirs: Vec<_> = path
            .ancestors()
            .skip(1)
            .take_while(|dir| dir.starts_with(&root))
            .map(Path::to_path_buf)
            .collect();
        dirs.reverse();
        for dir in dirs {
            self.expand(&dir)?;
        }
        Ok(())
    }

    pub fn open(&mut self, path: &Path) -> io::Result<Option<Content>> {
        if self.tabs.contains(path) {
            self.tabs.focus(path);
            return Ok(None);
        }
        let content = if Content::is_image(path) {
            Content::Image
        } else {
            Content::from_bytes(self.fs.read(path)?)
        };
        self.tabs.open(path);
        Ok(Some(content))
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

    fn tree_mut(&mut self) -> Option<&mut Tree> {
        self.trees.get_mut(self.project.as_ref()?)
    }
}
