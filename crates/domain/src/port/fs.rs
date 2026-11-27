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

    // An empty file where there was nothing; something already there is refused.
    fn create_file(&self, _path: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    // A folder where there was nothing; something already there is refused.
    fn create_dir(&self, _path: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    // Moves a file or folder; something already at `to` is refused, not written over.
    fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    // Sends a file or folder to the trash, where it can be brought back from.
    fn trash(&self, _path: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
