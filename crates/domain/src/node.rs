use std::cmp::Ordering;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    path: PathBuf,
    name: String,
    kind: Kind,
}

impl Node {
    pub fn new(path: impl Into<PathBuf>, kind: Kind) -> Self {
        let path = path.into();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self { path, name, kind }
    }

    pub fn dir(path: impl Into<PathBuf>) -> Self {
        Self::new(path, Kind::Dir)
    }

    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::new(path, Kind::File)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .is_dir()
            .cmp(&self.is_dir())
            .then_with(|| self.name.to_lowercase().cmp(&other.name.to_lowercase()))
            .then_with(|| self.name.cmp(&other.name))
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
