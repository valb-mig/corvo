use std::path::{Path, PathBuf};

pub fn root(file: &Path, markers: &[String], workspace: &Path) -> PathBuf {
    file.ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(workspace))
        .find(|dir| markers.iter().any(|marker| dir.join(marker).exists()))
        .unwrap_or(workspace)
        .to_path_buf()
}
