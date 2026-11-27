use std::path::PathBuf;

use crate::{Commit, FileStat};

// A branch, here or on a remote, with where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    // `main`, or `origin/main` for a remote one.
    pub name: String,
    pub remote: bool,
    // The commit it points at.
    pub hash: String,
    // Checked out in this folder.
    pub head: bool,
    // The upstream it tracks, and how far apart they are; gone when the upstream was deleted.
    pub upstream: Option<String>,
    pub gone: bool,
    pub ahead: usize,
    pub behind: usize,
    // Against the repository's main branch, for a branch tracking nothing.
    pub base: Option<Apart>,
    // The worktree it is checked out in, this folder's included.
    pub worktree: Option<PathBuf>,
    // All its commits are already in the main branch.
    pub merged: bool,
    // When its last commit was made, and what it says.
    pub when: i64,
    pub subject: String,
}

// How far a branch went from another: commits only it has, and commits only the other has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Apart {
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    // The commit it names.
    pub hash: String,
    pub when: i64,
}

// The branches and tags of a repository, and which branch is its main one: what others are
// measured against, merged into and rebased onto by default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refs {
    pub local: Vec<Branch>,
    pub remote: Vec<Branch>,
    pub tags: Vec<Tag>,
    pub main: Option<String>,
}

impl Refs {
    pub fn branch(&self, name: &str) -> Option<&Branch> {
        self.local
            .iter()
            .chain(&self.remote)
            .find(|branch| branch.name == name)
    }
}

// What one revision has that a base lacks: as a pull request would show it, the commits since
// they parted and the files changed since then (`base...head`), and the files that would
// conflict merging it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub base: String,
    pub head: String,
    pub commits: Vec<Commit>,
    pub files: Vec<FileStat>,
    // Commits the base got since they parted.
    pub behind: usize,
    // Merging it would stop on these files; empty when it merges clean.
    pub conflicts: Vec<String>,
}

// Work put aside with `git stash`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stash {
    // Its place, 0 the newest: `stash@{0}`.
    pub index: usize,
    pub hash: String,
    // What it was saved as; git's own `WIP on main: 2e49ded …` when no message was given.
    pub message: String,
    // The branch it was saved on.
    pub branch: String,
    pub when: i64,
}

impl Stash {
    pub fn name(&self) -> String {
        format!("stash@{{{}}}", self.index)
    }
}

// A folder checked out from the repository: the main one, or one added with `git worktree`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    // None before the first commit.
    pub head: Option<String>,
    // None when detached.
    pub branch: Option<String>,
    // The folder the repository was cloned or made in.
    pub main: bool,
    pub locked: bool,
    // Its folder is gone: `git worktree prune` takes it off the list.
    pub prunable: bool,
}

impl Worktree {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}
