use std::io;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::Arrangement;
use crate::port::Store;

// Its id is a UUID, never handed out again: a workspace made after another is removed can't
// take what was kept under the old one's id, like its chats.
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    id: Uuid,
    name: String,
    projects: Vec<PathBuf>,
    // How its home was left.
    home: Arrangement,
}

impl Workspace {
    pub fn new(id: Uuid, name: impl Into<String>, projects: Vec<PathBuf>) -> Self {
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

    pub fn id(&self) -> Uuid {
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
    // The one picked last on their list, picked again when the list comes back.
    last: Option<Uuid>,
}

impl Workspaces {
    pub fn load(store: Box<dyn Store>) -> io::Result<Self> {
        let saved = store.load()?;
        Ok(Self {
            store,
            items: saved.items,
            last: saved.last,
        })
    }

    pub fn items(&self) -> &[Workspace] {
        &self.items
    }

    pub fn get(&self, id: Uuid) -> Option<&Workspace> {
        self.items.iter().find(|item| item.id == id)
    }

    // The one picked last, while it is still there.
    pub fn last(&self) -> Option<Uuid> {
        self.last.filter(|id| self.get(*id).is_some())
    }

    pub fn pick(&mut self, id: Uuid) -> io::Result<()> {
        if self.last == Some(id) || self.get(id).is_none() {
            return Ok(());
        }
        self.last = Some(id);
        self.save()
    }

    pub fn add(&mut self, name: &str) -> io::Result<Uuid> {
        let name = named(name)?;
        let id = Uuid::new_v4();
        self.items.push(Workspace::new(id, name, Vec::new()));
        self.save()?;
        Ok(id)
    }

    pub fn rename(&mut self, id: Uuid, name: &str) -> io::Result<()> {
        let name = named(name)?;
        let Some(item) = self.item_mut(id) else {
            return Ok(());
        };
        if item.name == name {
            return Ok(());
        }
        item.name = name.to_owned();
        self.save()
    }

    pub fn remove(&mut self, id: Uuid) -> io::Result<()> {
        self.items.retain(|item| item.id != id);
        if self.last == Some(id) {
            self.last = None;
        }
        self.save()
    }

    pub fn link(&mut self, id: Uuid, project: &Path) -> io::Result<()> {
        let Some(item) = self.item_mut(id) else {
            return Ok(());
        };
        if !item.projects.iter().any(|known| known == project) {
            item.projects.push(project.to_path_buf());
        }
        self.save()
    }

    pub fn unlink(&mut self, id: Uuid, project: &Path) -> io::Result<()> {
        if let Some(item) = self.item_mut(id) {
            item.projects.retain(|known| known != project);
        }
        self.save()
    }

    // Points a project that was moved at where it is now, in the same place among the others. If
    // the new place is linked already, the old one just goes.
    pub fn relocate(&mut self, id: Uuid, from: &Path, to: &Path) -> io::Result<()> {
        let Some(item) = self.item_mut(id) else {
            return Ok(());
        };
        let Some(ix) = item.projects.iter().position(|known| known == from) else {
            return Ok(());
        };
        if item.projects.iter().any(|known| known == to) {
            item.projects.remove(ix);
        } else {
            item.projects[ix] = to.to_path_buf();
        }
        if item.home.root.as_deref() == Some(from) {
            item.home.root = Some(to.to_path_buf());
        }
        self.save()
    }

    // Has the home open next time with only this project, isolated, or with all of them.
    pub fn focus(&mut self, id: Uuid, project: Option<&Path>) -> io::Result<()> {
        let Some(item) = self.item_mut(id) else {
            return Ok(());
        };
        let root = project
            .filter(|project| item.projects.iter().any(|known| known == project))
            .map(Path::to_path_buf);
        if item.home.root == root {
            return Ok(());
        }
        item.home.root = root;
        self.save()
    }

    // Keeps how the home was left, saving only when it changed.
    pub fn arrange(&mut self, id: Uuid, home: Arrangement) -> io::Result<()> {
        let Some(item) = self.item_mut(id) else {
            return Ok(());
        };
        if item.home == home {
            return Ok(());
        }
        item.home = home;
        self.save()
    }

    fn item_mut(&mut self, id: Uuid) -> Option<&mut Workspace> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    fn save(&self) -> io::Result<()> {
        self.store.save(&self.items, self.last)
    }
}

fn named(name: &str) -> io::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "workspace name is empty",
        ));
    }
    Ok(name)
}
