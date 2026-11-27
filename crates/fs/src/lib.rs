mod chats;
mod ignored;
mod options;
mod search;
mod store;
pub mod trash;

use std::io;
use std::path::Path;

use corvo_domain::Node;
use corvo_domain::port::Fs;
use ignore::WalkBuilder;

pub use chats::JsonChats;
pub use options::JsonOptions;
pub use search::search;
pub use store::JsonStore;

#[derive(Clone, Copy, Debug, Default)]
pub struct LocalFs;

// The files under a folder, at most so many, leaving out what git ignores; sorted, for a palette
// to search.
pub fn files(root: &Path, most: usize) -> Vec<std::path::PathBuf> {
    let walk = walk(root).build();
    let mut files: Vec<_> = walk
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| entry.into_path())
        .take(most)
        .collect();
    files.sort();
    files
}

// A walk of the files under a folder that leaves out .git and what git ignores, hidden ones kept.
fn walk(root: &Path) -> WalkBuilder {
    let mut walk = WalkBuilder::new(root);
    walk.hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git");
    walk
}

impl Fs for LocalFs {
    // The folder's entries but .git and what git ignores. The rules of git are kept between
    // listings (see ignored), so a folder costs its own entries, not a reading of every rule file
    // above it.
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>> {
        let rules = ignored::rules(dir);
        let mut nodes = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_name() == ".git" {
                continue;
            }
            let kind = entry.file_type()?;
            let path = entry.path();
            if rules.ignores(&path, kind.is_dir()) {
                continue;
            }
            // A link to a folder shows as a folder.
            let dir = kind.is_dir() || (kind.is_symlink() && path.is_dir());
            nodes.push(if dir {
                Node::dir(path)
            } else {
                Node::file(path)
            });
        }
        Ok(nodes)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn write(&self, path: &Path, text: &str) -> io::Result<()> {
        std::fs::write(path, text)
    }

    fn size(&self, path: &Path) -> io::Result<u64> {
        Ok(std::fs::metadata(path)?.len())
    }

    fn create_file(&self, path: &Path) -> io::Result<()> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(drop)
            .map_err(|err| taken(err, path))
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path).map_err(|err| taken(err, path))
    }

    // A rename on its own writes over a file already there: that is looked for first.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if to.symlink_metadata().is_ok() {
            return Err(taken(io::ErrorKind::AlreadyExists.into(), to));
        }
        std::fs::rename(from, to)
    }

    fn trash(&self, path: &Path) -> io::Result<()> {
        trash::send(path, &trash::home()?, &trash::now()).map(drop)
    }
}

// Says which name is taken, rather than the system's "File exists".
fn taken(err: io::Error, path: &Path) -> io::Error {
    if err.kind() != io::ErrorKind::AlreadyExists {
        return err;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("{name} is already there"),
    )
}
