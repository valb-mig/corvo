use std::path::{Path, PathBuf};
use std::str::FromStr;

use lsp_types::Uri;
use url::Url;

pub fn from_path(path: &Path) -> Option<Uri> {
    let url = Url::from_file_path(path).ok()?;
    Uri::from_str(url.as_str()).ok()
}

pub fn to_path(uri: &Uri) -> Option<PathBuf> {
    Url::parse(uri.as_str()).ok()?.to_file_path().ok()
}
