use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::port::Fs;
use crate::server::Registry;
use crate::{Node, Share, census, scan, stack};

// What the list of workspaces tells of a linked project, read off the main thread: whether its
// folder is still there, its stack, and the languages it is written in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    missing: bool,
    stamps: Vec<Stamp>,
    shares: Vec<Share>,
    whole: bool,
}

impl Profile {
    // The folder was moved or deleted.
    pub fn is_missing(&self) -> bool {
        self.missing
    }

    pub fn stamps(&self) -> &[Stamp] {
        &self.stamps
    }

    // Most files first.
    pub fn shares(&self) -> &[Share] {
        &self.shares
    }

    // How many files were counted as code.
    pub fn files(&self) -> usize {
        self.shares.iter().map(Share::files).sum()
    }

    // Whether every file was counted, false for a project too big to look all over.
    pub fn is_whole(&self) -> bool {
        self.whole
    }
}

// A stack badge: the stack of the project, or of one of its folders when the project has no
// marker of its own, as a monorepo does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    dir: Option<String>,
    stack: String,
}

impl Stamp {
    // The folder the stack was found in, none for the project itself.
    pub fn dir(&self) -> Option<&str> {
        self.dir.as_deref()
    }

    pub fn stack(&self) -> &str {
        &self.stack
    }
}

// How many folders of a project with no marker are looked into for one.
const FOLDERS: usize = 16;

pub fn profile(fs: &dyn Fs, root: &Path, registry: &Registry) -> Profile {
    let children = match fs.list(root) {
        Ok(children) => children,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Profile {
                missing: true,
                ..Profile::default()
            };
        }
        Err(_) => Vec::new(),
    };
    let census = census(fs, root);
    Profile {
        missing: false,
        stamps: stamps(fs, &children, registry),
        shares: census.shares,
        whole: census.whole,
    }
}

fn stamps(fs: &dyn Fs, children: &[Node], registry: &Registry) -> Vec<Stamp> {
    if let Some(stack) = stack(children, registry) {
        return vec![Stamp {
            dir: None,
            stack: stack.to_owned(),
        }];
    }
    let mut dirs: Vec<&Node> = children
        .iter()
        .filter(|node| {
            node.is_dir() && !node.name().starts_with('.') && !scan::SKIP.contains(&node.name())
        })
        .collect();
    dirs.sort_by(|a, b| a.name().cmp(b.name()));
    dirs.into_iter()
        .take(FOLDERS)
        .filter_map(|dir| {
            let inside = fs.list(dir.path()).ok()?;
            let stack = stack(&inside, registry)?;
            Some(Stamp {
                dir: Some(dir.name().to_owned()),
                stack: stack.to_owned(),
            })
        })
        .collect()
}

// The languages of several projects together, most files first; a tie goes by name.
pub fn mix<'a>(profiles: impl IntoIterator<Item = &'a Profile>) -> Vec<Share> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for share in profiles.into_iter().flat_map(Profile::shares) {
        *counts.entry(share.name()).or_default() += share.files();
    }
    let mut shares: Vec<Share> = counts
        .into_iter()
        .map(|(name, files)| Share::new(name, files))
        .collect();
    shares.sort_by(|a, b| b.files().cmp(&a.files()).then(a.name().cmp(b.name())));
    shares
}
