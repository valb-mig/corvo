use std::path::{Path, PathBuf};

use crate::Lang;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    path: PathBuf,
    name: String,
    lang: Lang,
    dirty: bool,
}

impl Doc {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let lang = Lang::of(&path);
        Self {
            path,
            name,
            lang,
            dirty: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn touch(&mut self) {
        self.dirty = true;
    }

    pub fn clean(&mut self) {
        self.dirty = false;
    }
}
