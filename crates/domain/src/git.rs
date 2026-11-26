use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use similar::{Algorithm, DiffOp, capture_diff_slices_deadline};

// How long a diff may take before it settles for a coarser one: it runs as the dev types.
const DEADLINE: Duration = Duration::from_millis(50);

// How a file stands against the last commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    Added,
    Modified,
    Deleted,
    Renamed,
    // Not known to git yet.
    Untracked,
    // Left in a merge that stopped on it.
    Conflicted,
}

impl Change {
    // The letter git lists it with.
    pub fn letter(self) -> char {
        match self {
            Change::Added => 'A',
            Change::Modified => 'M',
            Change::Deleted => 'D',
            Change::Renamed => 'R',
            Change::Untracked => 'U',
            Change::Conflicted => '!',
        }
    }

    // How much it says about a folder that holds it: a conflict first, then a change to what
    // was there, then something new.
    fn weight(self) -> u8 {
        match self {
            Change::Conflicted => 3,
            Change::Modified | Change::Deleted | Change::Renamed => 2,
            Change::Added | Change::Untracked => 1,
        }
    }
}

// A changed file, by its path in full, and where it came from when it was renamed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub path: PathBuf,
    pub change: Change,
    pub from: Option<PathBuf>,
}

// A file whose standing against the last commit changed between two reads of its repository,
// and whether it is there to show: one that came or changed is, one that went or settled (back
// as the commit has it, or committed) is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub path: PathBuf,
    pub shown: bool,
}

// A repository as git reads it now: where it is, the branch, the commit it is on and what
// changed since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    root: PathBuf,
    branch: String,
    head: Option<String>,
    changes: Vec<Changed>,
    // The change of each file, and the weightiest one under each folder up to the root.
    files: HashMap<PathBuf, Change>,
    dirs: HashMap<PathBuf, Change>,
}

impl Repo {
    pub fn new(
        root: impl Into<PathBuf>,
        branch: impl Into<String>,
        head: Option<String>,
        mut changes: Vec<Changed>,
    ) -> Self {
        let root = root.into();
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        let mut files = HashMap::new();
        let mut dirs: HashMap<PathBuf, Change> = HashMap::new();
        for changed in &changes {
            files.insert(changed.path.clone(), changed.change);
            for dir in changed.path.ancestors().skip(1) {
                if !dir.starts_with(&root) {
                    break;
                }
                let kept = dirs.entry(dir.to_path_buf()).or_insert(changed.change);
                if changed.change.weight() > kept.weight() {
                    *kept = changed.change;
                }
            }
        }
        Self {
            root,
            branch: branch.into(),
            head,
            changes,
            files,
            dirs,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    // The branch checked out, or the short commit when none is.
    pub fn branch(&self) -> &str {
        &self.branch
    }

    // The commit checked out; none before the first one.
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }

    pub fn changes(&self) -> &[Changed] {
        &self.changes
    }

    pub fn holds(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    // How a file stands, or the weightiest change under a folder.
    pub fn change_of(&self, path: &Path) -> Option<Change> {
        self.files
            .get(path)
            .or_else(|| self.dirs.get(path))
            .copied()
    }

    // The files that moved since the repository was read before.
    pub fn moved_since(&self, before: &Repo) -> Vec<Moved> {
        let mut moved = Vec::new();
        for changed in &self.changes {
            if before.files.get(&changed.path) == Some(&changed.change) {
                continue;
            }
            moved.push(Moved {
                path: changed.path.clone(),
                shown: changed.change != Change::Deleted,
            });
            if let Some(from) = &changed.from {
                moved.push(Moved {
                    path: from.clone(),
                    shown: false,
                });
            }
        }
        for gone in before
            .changes
            .iter()
            .filter(|c| !self.files.contains_key(&c.path))
        {
            moved.push(Moved {
                path: gone.path.clone(),
                shown: false,
            });
        }
        moved
    }
}

// What the text of a file does to the lines of its last commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Added,
    Modified,
    // Lines taken out before the line the hunk starts at.
    Deleted,
}

// A run of lines of the text, by their numbers from 0; empty for lines taken out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub mark: Mark,
    pub lines: Range<usize>,
}

// Where the text differs from the base, line by line.
pub fn hunks(base: &str, text: &str) -> Vec<Hunk> {
    let old: Vec<&str> = base.lines().collect();
    let new: Vec<&str> = text.lines().collect();
    let mut hunks: Vec<Hunk> = Vec::new();
    let deadline = Some(Instant::now() + DEADLINE);
    for op in capture_diff_slices_deadline(Algorithm::Myers, &old, &new, deadline) {
        let hunk = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete { new_index, .. } => Hunk {
                mark: Mark::Deleted,
                lines: new_index..new_index,
            },
            DiffOp::Insert {
                new_index, new_len, ..
            } => Hunk {
                mark: Mark::Added,
                lines: new_index..new_index + new_len,
            },
            DiffOp::Replace {
                new_index, new_len, ..
            } => Hunk {
                mark: Mark::Modified,
                lines: new_index..new_index + new_len,
            },
        };
        // Lines taken out right where others came in changed them.
        match hunks.last_mut() {
            Some(last)
                if last.mark == Mark::Deleted
                    && hunk.mark == Mark::Added
                    && last.lines.start == hunk.lines.start =>
            {
                *last = Hunk {
                    mark: Mark::Modified,
                    lines: hunk.lines,
                };
            }
            _ => hunks.push(hunk),
        }
    }
    hunks
}
