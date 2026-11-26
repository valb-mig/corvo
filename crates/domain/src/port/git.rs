use std::io;
use std::path::Path;

use crate::Repo;

// What git knows of the folders worked on.
pub trait Git: Send + Sync {
    // The repository a folder is in, as it is now; none outside one.
    fn repo(&self, dir: &Path) -> io::Result<Option<Repo>>;
    // A file of the repository as its last commit has it; none when it is not there or is not
    // text.
    fn base(&self, root: &Path, file: &Path) -> io::Result<Option<String>>;
    // The patch from the last commit to the file as it is on disk.
    fn patch(&self, root: &Path, file: &Path) -> io::Result<String>;
}
