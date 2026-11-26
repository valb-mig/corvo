mod chats;
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

impl Fs for LocalFs {
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>> {
        let walk = WalkBuilder::new(dir)
            .max_depth(Some(1))
            .hidden(false)
            .filter_entry(|entry| entry.file_name() != ".git")
            .build();

        let mut nodes = Vec::new();
        for entry in walk {
            let entry = entry.map_err(io::Error::other)?;
            if entry.depth() == 0 {
                continue;
            }
            let path = entry.into_path();
            nodes.push(if path.is_dir() {
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
