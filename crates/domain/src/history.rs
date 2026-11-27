use crate::Change;

// A name pointing at a commit, as the history shows it beside the commit.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RefName {
    // The branch checked out.
    Head(String),
    Local(String),
    // `origin/main`.
    Remote(String),
    Tag(String),
}

impl RefName {
    pub fn name(&self) -> &str {
        match self {
            RefName::Head(name)
            | RefName::Local(name)
            | RefName::Remote(name)
            | RefName::Tag(name) => name,
        }
    }

    // A full ref name, `refs/heads/main`, as git decorates with; none for what is not a branch,
    // a remote branch or a tag (the stash, notes).
    pub fn from_full(full: &str) -> Option<Self> {
        if let Some(name) = full.strip_prefix("refs/heads/") {
            Some(RefName::Local(name.to_owned()))
        } else if let Some(name) = full.strip_prefix("refs/remotes/") {
            // `origin/HEAD` only says which branch the remote calls its own.
            (!name.ends_with("/HEAD")).then(|| RefName::Remote(name.to_owned()))
        } else {
            full.strip_prefix("refs/tags/")
                .map(|name| RefName::Tag(name.to_owned()))
        }
    }
}

// A commit as a list of them shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    // Seconds since the epoch, when it was authored.
    pub when: i64,
    pub subject: String,
    pub refs: Vec<RefName>,
    // In the reflog, what moved HEAD there: `checkout: moving from main to x`.
    pub note: Option<String>,
}

impl Commit {
    pub fn short(&self) -> &str {
        &self.hash[..self.hash.len().min(7)]
    }

    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
}

// Which commits a history lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    // Every branch, remote branch and tag.
    #[default]
    All,
    // What one branch, or any revision, reaches.
    Rev(String),
    // Where HEAD has been, newest first: the way back after a mistake.
    Reflog,
}

// A page of a history: its scope, what it is filtered by, and where the page starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Log {
    pub scope: Scope,
    // Matched against the message, the author, a hash and a path; empty for all.
    pub text: String,
    pub skip: usize,
    pub limit: usize,
}

impl Log {
    // How many commits a page holds.
    pub const PAGE: usize = 200;

    pub fn new(scope: Scope, text: impl Into<String>) -> Self {
        Self {
            scope,
            text: text.into(),
            skip: 0,
            limit: Self::PAGE,
        }
    }

    pub fn next_page(&self) -> Self {
        Self {
            skip: self.skip + self.limit,
            ..self.clone()
        }
    }

    // A filtered history skips commits, so its lines would join the wrong ones: it draws none.
    pub fn draws_graph(&self) -> bool {
        self.text.trim().is_empty() && self.scope != Scope::Reflog
    }
}

// A file a commit, a comparison or a stash touched, and by how many lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStat {
    // Inside the repository, as git writes it.
    pub path: String,
    pub from: Option<String>,
    pub change: Change,
    // None for a binary file.
    pub lines: Option<(usize, usize)>,
}

impl FileStat {
    pub fn added(&self) -> usize {
        self.lines.map_or(0, |(added, _)| added)
    }

    pub fn removed(&self) -> usize {
        self.lines.map_or(0, |(_, removed)| removed)
    }
}

// The lines all the files gained and lost.
pub fn totals(files: &[FileStat]) -> (usize, usize) {
    files
        .iter()
        .fold((0, 0), |(a, r), f| (a + f.added(), r + f.removed()))
}

// A commit opened: its whole message and the files it touched against its first parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detail {
    pub commit: Commit,
    // The message after its subject, trimmed.
    pub body: String,
    pub files: Vec<FileStat>,
}

// Where a commit can be found: the branches that reach it, and whether any remote one does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Holders {
    pub local: Vec<String>,
    pub remote: Vec<String>,
}

// One row of the graph beside a history: the lane its commit sits in, the lanes running past it,
// the lanes coming down into it and those leaving it downwards. Lanes are columns counted from
// the left; a lane keeps its column while it lives, so a line never jumps sideways.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub lane: usize,
    // Lanes passing by, top to bottom, not touching the commit.
    pub through: Vec<usize>,
    // Lanes ending in the commit from above: its own when a later commit had it as a parent,
    // and others that met it there.
    pub into: Vec<usize>,
    // Lanes leaving the commit downwards, to its parents.
    pub out: Vec<usize>,
}

impl Row {
    // How many columns the row needs.
    pub fn width(&self) -> usize {
        self.through
            .iter()
            .chain(&self.into)
            .chain(&self.out)
            .chain(std::iter::once(&self.lane))
            .max()
            .map_or(1, |widest| widest + 1)
    }
}

// The graph of a list of commits, newest first, each listed before its parents (as
// `git log --topo-order` or `--date-order` gives them). Parents not in the list (a page that
// ends, a shallow clone) leave lanes running off the bottom.
pub fn graph(commits: &[Commit]) -> Vec<Row> {
    // The commit each lane waits for, by column; None where a lane is free.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for commit in commits {
        let hash = commit.hash.as_str();
        let waiting: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, wants)| **wants == Some(hash))
            .map(|(ix, _)| ix)
            .collect();
        let lane = match waiting.first() {
            Some(first) => *first,
            // A tip nothing listed yet points at: the first free column.
            None => free(&mut lanes),
        };
        for ix in &waiting {
            lanes[*ix] = None;
        }
        let through: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(ix, wants)| wants.is_some() && *ix != lane)
            .map(|(ix, _)| ix)
            .collect();
        let mut out = Vec::new();
        for (nth, parent) in commit.parents.iter().enumerate() {
            let parent = parent.as_str();
            if nth == 0 {
                // The first parent carries on in the commit's own lane, unless a lane running by
                // already waits for it: then the line bends over to that one.
                match lanes.iter().position(|wants| *wants == Some(parent)) {
                    Some(ix) if ix < lane => out.push(ix),
                    _ => {
                        lanes[lane] = Some(parent);
                        out.push(lane);
                    }
                }
            } else {
                match lanes.iter().position(|wants| *wants == Some(parent)) {
                    Some(ix) => out.push(ix),
                    None => {
                        let ix = free(&mut lanes);
                        lanes[ix] = Some(parent);
                        out.push(ix);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
        rows.push(Row {
            lane,
            through,
            into: waiting,
            out,
        });
    }
    rows
}

// The first free column, made when there is none.
fn free(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(ix) => ix,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}
