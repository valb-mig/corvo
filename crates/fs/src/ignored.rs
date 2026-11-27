use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

// What git leaves out of a folder, decided as the ignore crate's walker decides it: the .ignore
// files of the folder and of every folder above it, then the .gitignore files and the exclude file
// up to the top of the repository, then the dev's global excludes; the nearest file that says
// anything about a path wins. Each file is read and compiled once and kept, read again only once
// it changed on disk, so listing a folder no longer reads every rule file above it.
pub struct Rules {
    // The folder listed and those above it, nearest first.
    chain: Vec<Arc<Folder>>,
}

impl Rules {
    pub fn ignores(&self, path: &Path, is_dir: bool) -> bool {
        // Without a repository around, the rules of git don't apply; those of .ignore do.
        let git = self.chain.iter().any(|folder| folder.git.is_some());
        let (mut ignore, mut gitignore, mut exclude) = (None, None, None);
        let mut above_repo = false;
        for folder in &self.chain {
            ignore = ignore.or_else(|| said(folder.ignore.matcher.matched(path, is_dir)));
            if git && !above_repo {
                gitignore =
                    gitignore.or_else(|| said(folder.gitignore.matcher.matched(path, is_dir)));
                if let Some(repo) = &folder.git {
                    exclude = exclude.or_else(|| said(repo.exclude.matcher.matched(path, is_dir)));
                }
            }
            above_repo |= folder.git.is_some();
        }
        let global = || git.then(|| said(global().matched(path, is_dir))).flatten();
        ignore
            .or(gitignore)
            .or(exclude)
            .or_else(global)
            .unwrap_or(false)
    }
}

// The rules a listing of `dir` goes by.
pub fn rules(dir: &Path) -> Rules {
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let chain = dir.ancestors().map(folder).collect();
    Rules { chain }
}

// Whether a rule matched, and if so whether it leaves the path out.
fn said<T>(found: Match<T>) -> Option<bool> {
    match found {
        Match::None => None,
        Match::Ignore(_) => Some(true),
        Match::Whitelist(_) => Some(false),
    }
}

// The rules a folder holds, as last read.
struct Folder {
    ignore: Kept,
    gitignore: Kept,
    // Set when the folder is the top of a repository.
    git: Option<Repo>,
}

struct Repo {
    // Whether .git was a file naming the repository elsewhere, as for a worktree, rather than a
    // folder, when its exclude file was found. Not when it was written: git writes in it all the
    // time.
    linked: Option<bool>,
    exclude: Kept,
}

// A compiled rule file and how the file was when read.
struct Kept {
    file: PathBuf,
    stamp: Stamp,
    matcher: Gitignore,
}

// What tells a file changed: when it was written and how long it is; none when it is not there.
type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

// Whether .git is a file rather than a folder; none when it is not there.
fn linked(dir: &Path) -> Option<bool> {
    fs::metadata(dir.join(".git"))
        .ok()
        .map(|meta| meta.is_file())
}

impl Kept {
    fn read(root: &Path, file: PathBuf) -> Self {
        let stamp = stamp(&file);
        let matcher = match stamp {
            Some(_) => {
                let mut builder = GitignoreBuilder::new(root);
                // Lines that don't parse are skipped, as the walker skips them.
                let _ = builder.add(&file);
                builder.build().unwrap_or_else(|_| Gitignore::empty())
            }
            None => Gitignore::empty(),
        };
        Self {
            file,
            stamp,
            matcher,
        }
    }

    fn is_fresh(&self) -> bool {
        stamp(&self.file) == self.stamp
    }
}

impl Folder {
    fn read(dir: &Path) -> Self {
        let linked = linked(dir);
        let git = (linked.is_some() || dir.join(".jj").exists()).then(|| Repo {
            linked,
            exclude: Kept::read(dir, common_dir(dir).join("info/exclude")),
        });
        Self {
            ignore: Kept::read(dir, dir.join(".ignore")),
            gitignore: Kept::read(dir, dir.join(".gitignore")),
            git,
        }
    }

    fn is_fresh(&self, dir: &Path) -> bool {
        let linked = linked(dir);
        let git_fresh = match &self.git {
            Some(repo) => {
                repo.linked == linked
                    && (linked.is_some() || dir.join(".jj").exists())
                    && repo.exclude.is_fresh()
            }
            None => linked.is_none() && !dir.join(".jj").exists(),
        };
        git_fresh && self.ignore.is_fresh() && self.gitignore.is_fresh()
    }
}

fn kept() -> &'static Mutex<HashMap<PathBuf, Arc<Folder>>> {
    static KEPT: OnceLock<Mutex<HashMap<PathBuf, Arc<Folder>>>> = OnceLock::new();
    KEPT.get_or_init(Mutex::default)
}

// The rules of a folder, read again only if a file of them changed.
fn folder(dir: &Path) -> Arc<Folder> {
    let known = kept().lock().ok().and_then(|kept| kept.get(dir).cloned());
    if let Some(known) = known
        && known.is_fresh(dir)
    {
        return known;
    }
    let fresh = Arc::new(Folder::read(dir));
    if let Ok(mut kept) = kept().lock() {
        kept.insert(dir.to_path_buf(), fresh.clone());
    }
    fresh
}

// The dev's global excludes (core.excludesFile, or git's default place for it), read once.
fn global() -> &'static Gitignore {
    static GLOBAL: OnceLock<Gitignore> = OnceLock::new();
    GLOBAL.get_or_init(|| Gitignore::global().0)
}

// Where the repository whose top is `dir` keeps what its worktrees share: .git itself, or for a
// worktree, the folder its .git file points to, through its commondir file.
fn common_dir(dir: &Path) -> PathBuf {
    let dot_git = dir.join(".git");
    if !dot_git.is_file() {
        return dot_git;
    }
    let first_line = |path: &Path| {
        let file = File::open(path).ok()?;
        BufReader::new(file).lines().next()?.ok()
    };
    let Some(git_dir) =
        first_line(&dot_git).and_then(|line| line.strip_prefix("gitdir: ").map(PathBuf::from))
    else {
        return dot_git;
    };
    match first_line(&git_dir.join("commondir")) {
        Some(common) if common.starts_with('.') => git_dir.join(common),
        Some(common) => PathBuf::from(common),
        // A gitdir with no commondir shares nothing: no exclude file is read.
        None => dot_git,
    }
}
