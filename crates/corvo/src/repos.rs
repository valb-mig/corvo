use std::path::{Path, PathBuf};
use std::sync::Arc;

use corvo_domain::port::Git;
use corvo_domain::{Change, Moved, Repo};
use gpui_kit::{Context, EventEmitter};

// Files that moved on disk since the read before: came, changed, went or settled. A repository
// read for the first time is only where things stand.
pub struct Event(pub Vec<Moved>);

// The repositories of the projects linked, as git last read them, one for each project or for
// projects that share one. A read runs off the main thread, and one asked for meanwhile runs
// right after it. A folder git cannot read, or git missing, just has no repository.
pub struct Repos {
    git: Arc<dyn Git>,
    projects: Vec<PathBuf>,
    repos: Vec<Repo>,
    reading: bool,
    again: bool,
}

impl EventEmitter<Event> for Repos {}

impl Repos {
    pub fn new(git: Arc<dyn Git>) -> Self {
        Self {
            git,
            projects: Vec::new(),
            repos: Vec::new(),
            reading: false,
            again: false,
        }
    }

    pub fn git(&self) -> Arc<dyn Git> {
        self.git.clone()
    }

    // The projects to follow, read at once when they are others.
    pub fn track(&mut self, projects: Vec<PathBuf>, cx: &mut Context<Self>) {
        if projects != self.projects {
            self.projects = projects;
            self.refresh(cx);
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.reading {
            self.again = true;
            return;
        }
        self.reading = true;
        let (git, projects) = (self.git.clone(), self.projects.clone());
        let task = cx.background_executor().spawn(async move {
            let mut repos: Vec<Repo> = Vec::new();
            for project in &projects {
                if repos.iter().any(|repo| repo.holds(project)) {
                    continue;
                }
                if let Ok(Some(repo)) = git.repo(project) {
                    repos.push(repo);
                }
            }
            repos
        });
        cx.spawn(async move |this, cx| {
            let repos = task.await;
            this.update(cx, |this, cx| {
                this.reading = false;
                if repos != this.repos {
                    let moved: Vec<Moved> = repos
                        .iter()
                        .filter_map(|repo| {
                            let before = this.repos.iter().find(|b| b.root() == repo.root())?;
                            Some(repo.moved_since(before))
                        })
                        .flatten()
                        .collect();
                    this.repos = repos;
                    if !moved.is_empty() {
                        cx.emit(Event(moved));
                    }
                    cx.notify();
                }
                if std::mem::take(&mut this.again) {
                    this.refresh(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn repos(&self) -> &[Repo] {
        &self.repos
    }

    // The repository a path is in: the innermost, when one holds another.
    pub fn repo_of(&self, path: &Path) -> Option<&Repo> {
        self.repos
            .iter()
            .filter(|repo| repo.holds(path))
            .max_by_key(|repo| repo.root().components().count())
    }

    // How a file stands, or the weightiest change under a folder.
    pub fn change_of(&self, path: &Path) -> Option<Change> {
        self.repo_of(path)?.change_of(path)
    }
}
