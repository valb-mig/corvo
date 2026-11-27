use std::io;
use std::path::Path;

use crate::{Commit, Comparison, Detail, FileStat, Holders, Log, Op, Refs, Repo, Stash, Worktree};

// Which change of a file a patch shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    // The index against the last commit: what the next commit takes.
    Staged,
    // The work tree against the index: what it leaves out.
    Unstaged,
}

// What git knows of the folders worked on, and what it is asked to do to them. Paths inside a
// repository are as git writes them, from its root, with `/`.
pub trait Git: Send + Sync {
    // The repository a folder is in, as it is now; none outside one.
    fn repo(&self, dir: &Path) -> io::Result<Option<Repo>>;
    // A file of the repository as its last commit has it; none when it is not there or is not
    // text.
    fn base(&self, root: &Path, file: &Path) -> io::Result<Option<String>>;
    // The patch from the last commit to the file as it is on disk.
    fn patch(&self, root: &Path, file: &Path) -> io::Result<String>;
    // The patch of one side of a file's change; an untracked file shows whole, as unstaged.
    fn side_patch(&self, root: &Path, file: &Path, side: Side) -> io::Result<String>;

    // A page of the history.
    fn log(&self, root: &Path, log: &Log) -> io::Result<Vec<Commit>>;
    // A commit opened: its message and files.
    fn detail(&self, root: &Path, rev: &str) -> io::Result<Detail>;
    // The patch a commit made, against its first parent.
    fn show(&self, root: &Path, rev: &str) -> io::Result<String>;
    // The branches that reach a commit.
    fn holders(&self, root: &Path, rev: &str) -> io::Result<Holders>;

    fn refs(&self, root: &Path) -> io::Result<Refs>;
    // What `head` has that `base` lacks.
    fn compare(&self, root: &Path, base: &str, head: &str) -> io::Result<Comparison>;
    // The patch of `base...head`.
    fn compare_patch(&self, root: &Path, base: &str, head: &str) -> io::Result<String>;

    fn stashes(&self, root: &Path) -> io::Result<Vec<Stash>>;
    fn stash_files(&self, root: &Path, index: usize) -> io::Result<Vec<FileStat>>;
    fn stash_patch(&self, root: &Path, index: usize) -> io::Result<String>;

    fn worktrees(&self, root: &Path) -> io::Result<Vec<Worktree>>;

    // Does what the dev asked; the error carries what git said.
    fn run(&self, root: &Path, op: &Op) -> io::Result<()>;
}
