use std::path::{Path, PathBuf};

use crate::owner;

pub fn bound(file: &Path, projects: &[PathBuf], active: Option<&Path>) -> PathBuf {
    owner(file, projects)
        .or(active)
        .or_else(|| file.parent())
        .unwrap_or(file)
        .to_path_buf()
}

pub fn root(file: &Path, markers: &[String], workspace: &Path) -> PathBuf {
    file.ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(workspace))
        .find(|dir| markers.iter().any(|marker| dir.join(marker).exists()))
        .unwrap_or(workspace)
        .to_path_buf()
}
