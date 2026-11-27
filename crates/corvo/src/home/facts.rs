use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use corvo_domain::port::Git;
use corvo_domain::server::Registry;
use corvo_domain::{Profile, profile};
use corvo_fs::LocalFs;
use corvo_git::GitCli;
use gpui_kit::Context;

// Where a project stands in git: its branch, and how many files changed in its repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub root: PathBuf,
    pub name: String,
    pub changed: usize,
}

// What the list of workspaces knows of the linked projects, read off the main thread: the profile
// of every project, read once a run, and the git of those shown, read again each time they are.
// It lives as long as the window, so going back to the list reads nothing again.
pub struct Facts {
    registry: Registry,
    profiles: HashMap<PathBuf, Profile>,
    weighing: HashSet<PathBuf>,
    // None for a folder outside any repository.
    branches: HashMap<PathBuf, Option<Branch>>,
    looking: HashSet<PathBuf>,
}

impl Facts {
    pub fn new() -> Self {
        Self {
            registry: Registry::default(),
            profiles: HashMap::new(),
            weighing: HashSet::new(),
            branches: HashMap::new(),
            looking: HashSet::new(),
        }
    }

    // None while it is being read.
    pub fn profile(&self, project: &Path) -> Option<&Profile> {
        self.profiles.get(project)
    }

    // None while it is being read, Some(None) outside git.
    pub fn branch(&self, project: &Path) -> Option<Option<&Branch>> {
        self.branches.get(project).map(Option::as_ref)
    }

    // Reads the profiles not read yet, one task each.
    pub fn weigh(&mut self, projects: &[PathBuf], cx: &mut Context<Self>) {
        for project in projects {
            if self.profiles.contains_key(project) || !self.weighing.insert(project.clone()) {
                continue;
            }
            let root = project.clone();
            let registry = self.registry.clone();
            let task = cx
                .background_executor()
                .spawn(async move { profile(&LocalFs, &root, &registry) });
            let root = project.clone();
            cx.spawn(async move |this, cx| {
                let found = task.await;
                this.update(cx, |this, cx| {
                    this.weighing.remove(&root);
                    this.profiles.insert(root, found);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    // Reads a project again, as after it was found where it moved to.
    pub fn forget(&mut self, project: &Path) {
        self.profiles.remove(project);
        self.branches.remove(project);
    }

    // Reads the git of these projects again, keeping what was read before meanwhile.
    pub fn look(&mut self, projects: &[PathBuf], cx: &mut Context<Self>) {
        for project in projects {
            if !self.looking.insert(project.clone()) {
                continue;
            }
            let dir = project.clone();
            let task = cx.background_executor().spawn(async move {
                GitCli.repo(&dir).ok().flatten().map(|repo| Branch {
                    root: repo.root().to_path_buf(),
                    name: repo.branch().to_owned(),
                    changed: repo.changes().len(),
                })
            });
            let dir = project.clone();
            cx.spawn(async move |this, cx| {
                let found = task.await;
                this.update(cx, |this, cx| {
                    this.looking.remove(&dir);
                    this.branches.insert(dir, found);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }
}
