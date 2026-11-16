use std::io;
use std::path::Path;

use crate::Node;

pub trait Fs {
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>>;
    fn read(&self, path: &Path) -> io::Result<String>;
    fn write(&self, path: &Path, text: &str) -> io::Result<()>;
}
