mod chats;
mod ignored;
mod options;
mod store;

use std::io;
use std::path::Path;

use corvo_domain::Node;
use corvo_domain::port::Fs;
use ignore::WalkBuilder;

pub use chats::JsonChats;
pub use options::JsonOptions;
pub use store::JsonStore;

#[derive(Clone, Copy, Debug, Default)]
pub struct LocalFs;

// The files under a folder, at most so many, leaving out what git ignores; sorted, for a palette
// to search.
pub fn files(root: &Path, most: usize) -> Vec<std::path::PathBuf> {
    let walk = WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();
    let mut files: Vec<_> = walk
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| entry.into_path())
        .take(most)
        .collect();
    files.sort();
    files
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
}
