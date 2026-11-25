use std::io;
use std::path::{Path, PathBuf};

use crate::Arrangement;
use crate::port::Store;

#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    id: u64,
    name: String,
    projects: Vec<PathBuf>,
    // How its home was left.
    home: Arrangement,
}

impl Workspace {
    pub fn new(id: u64, name: impl Into<String>, projects: Vec<PathBuf>) -> Self {
        Self {
            id,
            name: name.into(),
            projects,
            home: Arrangement::default(),
        }
    }

    pub fn arranged(mut self, home: Arrangement) -> Self {
        self.home = home;
        self
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn projects(&self) -> &[PathBuf] {
        &self.projects
    }

    pub fn home(&self) -> &Arrangement {
        &self.home
    }

    pub fn owner(&self, file: &Path) -> Option<&Path> {
        owner(file, &self.projects)
    }
}

pub fn owner<'a>(file: &Path, projects: &'a [PathBuf]) -> Option<&'a Path> {
    projects
        .iter()
        .filter(|project| file.starts_with(project))
        .max_by_key(|project| project.components().count())
        .map(PathBuf::as_path)
}

pub struct Workspaces {
    store: Box<dyn Store>,
    items: Vec<Workspace>,
}

impl Workspaces {
    pub fn load(store: Box<dyn Store>) -> io::Result<Self> {
        let items = store.load()?;
        Ok(Self { store, items })
    }

    pub fn items(&self) -> &[Workspace] {
        &self.items
    }

    pub fn get(&self, id: u64) -> Option<&Workspace> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn add(&mut self, name: &str) -> io::Result<u64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workspace name is empty",
            ));
        }
        let id = self.items.iter().map(Workspace::id).max().unwrap_or(0) + 1;
        self.items.push(Workspace::new(id, name, Vec::new()));
        self.save()?;
        Ok(id)
    }

    pub fn remove(&mut self, id: u64) -> io::Result<()> {
        self.items.retain(|item| item.id != id);
        self.save()
    }

    pub fn link(&mut self, id: u64, project: &Path) -> io::Result<()> {
        let Some(item) = self.item_mut(id) else {
            return Ok(());
        };
        if !item.projects.iter().any(|known| known == project) {
            item.projects.push(project.to_path_buf());
        }
        self.save()
    }

    pub fn unlink(&mut self, id: u64, project: &Path) -> io::Result<()> {
        if let Some(item) = self.item_mut(id) {
            item.projects.retain(|known| known != project);
        }
        self.save()
    }

    // Keeps how the home was left, saving only when it changed.
    pub fn arrange(&mut self, id: u64, home: Arrangement) -> io::Result<()> {
        let Some(item) = self.item_mut(id) else {
            return Ok(());
        };
        if item.home == home {
            return Ok(());
        }
        item.home = home;
        self.save()
    }

    fn item_mut(&mut self, id: u64) -> Option<&mut Workspace> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    fn save(&self) -> io::Result<()> {
        self.store.save(&self.items)
    }
}
