use std::collections::HashMap;
use std::path::{Path, PathBuf};

use corvo_domain::{Finder, Found};

// How big a file is looked into at most; a bigger one is taken for data, and passed over.
const BIGGEST: u64 = 1 << 20;
// How much of the start of a file tells whether it is text: none with a NUL in it is.
const SNIFF: usize = 8000;

// Looks for what the finder finds in the files of a project, those git ignores left out, in the
// order of their paths. A file open in the editor is looked at as it is there, not as on disk;
// one too big, or not text, is passed over. Stops at `most` hits; gives nothing when `stop`
// says, between two files, that it is no longer wanted.
pub fn search(
    root: &Path,
    finder: &Finder,
    open: &HashMap<PathBuf, String>,
    most: usize,
    stop: &dyn Fn() -> bool,
) -> Option<Found> {
    let mut found = Found::default();
    let walk = crate::walk(root).sort_by_file_name(|a, b| a.cmp(b)).build();
    for entry in walk.flatten() {
        if stop() {
            return None;
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.into_path();
        let hits = match open.get(&path) {
            Some(text) => finder.find(text, most),
            None => match text_of(&path) {
                Some(text) => finder.find(&text, most),
                None => continue,
            },
        };
        if !found.add(path, hits, most) {
            break;
        }
    }
    Some(found)
}

fn text_of(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > BIGGEST {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes[..bytes.len().min(SNIFF)].contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}
