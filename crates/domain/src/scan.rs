use std::path::Path;

use crate::Node;
use crate::port::Fs;

// A look over every file of a project, bounded so a huge one stays quick: no deeper than DEPTH
// folders, no more than ENTRIES entries, and never into vendored or built folders.

const ENTRIES: usize = 8000;
const DEPTH: usize = 12;

pub(crate) const SKIP: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    ".git",
];

// Tells whether every entry was looked at, false when it stopped at ENTRIES.
pub fn files(fs: &dyn Fs, root: &Path, mut visit: impl FnMut(&Node)) -> bool {
    let mut stack = vec![(root.to_path_buf(), 0)];
    let mut seen = 0;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(nodes) = fs.list(&dir) else {
            continue;
        };
        for node in nodes {
            seen += 1;
            if seen > ENTRIES {
                return false;
            }
            if !node.is_dir() {
                visit(&node);
            } else if depth < DEPTH && !SKIP.contains(&node.name()) {
                stack.push((node.path().to_path_buf(), depth + 1));
            }
        }
    }
    true
}
