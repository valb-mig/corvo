use std::io;
use std::path::Path;

use crate::Node;

pub trait Fs {
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>>;
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn write(&self, path: &Path, text: &str) -> io::Result<()>;

    // How many bytes a file takes.
    fn size(&self, _path: &Path) -> io::Result<u64> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
