use std::path::PathBuf;

// How far a reset goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reset {
    // Moves the branch only; what it leaves behind stays staged.
    Soft,
    // Moves the branch and the index; what it leaves behind stays on disk, unstaged.
    Mixed,
    // Moves everything: what it leaves behind is lost.
    Hard,
}

// Something the dev asks git to do to a repository. Paths are inside the repository, as git
// writes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    // Puts files back as the index has them, and deletes the untracked ones.
    Discard {
        tracked: Vec<String>,
        untracked: Vec<String>,
    },
    // A patch of hunks or lines: staged (`cached`), unstaged (`cached` and `reverse`), or
    // discarded from the work tree (`reverse`).
    Apply {
        patch: String,
        cached: bool,
        reverse: bool,
    },
    Commit {
        message: String,
        amend: bool,
    },
    Fetch,
    Pull,
    // `upstream` sets where the branch pushes, for a branch never pushed.
    Push {
        upstream: Option<String>,
    },
    Checkout(String),
    // A new branch at `from`, checked out when `checkout`.
    Branch {
        name: String,
        from: String,
        checkout: bool,
    },
    Rename {
        from: String,
        to: String,
    },
    // `force` deletes it even with commits merged nowhere.
    DeleteBranch {
        name: String,
        force: bool,
    },
    Merge(String),
    Rebase(String),
    CherryPick(String),
    Revert(String),
    ResetTo {
        rev: String,
        how: Reset,
    },
    Tag {
        name: String,
        at: String,
    },
    DeleteTag(String),
    Stash {
        message: String,
        untracked: bool,
        keep_index: bool,
    },
    StashApply(usize),
    StashPop(usize),
    StashDrop(usize),
    // A new branch where the stash was saved, with the stash applied on it, then dropped.
    StashBranch {
        name: String,
        index: usize,
    },
    // A worktree at `path`: on a branch there is, or on a new one made from `from`.
    AddWorktree {
        path: PathBuf,
        branch: String,
        from: Option<String>,
    },
    RemoveWorktree {
        path: PathBuf,
        force: bool,
    },
    PruneWorktrees,
    // What a stopped merge, rebase, cherry-pick or revert does next.
    Continue,
    Skip,
    Abort,
    // A conflicted file taken whole from one side, then marked resolved.
    TakeSide {
        path: String,
        ours: bool,
    },
}

impl Op {
    // Whether it can lose work the dev has not committed or kept elsewhere, so it asks first.
    pub fn loses_work(&self) -> bool {
        matches!(
            self,
            Op::Discard { .. }
                | Op::Apply {
                    cached: false,
                    reverse: true,
                    ..
                }
                | Op::ResetTo {
                    how: Reset::Hard,
                    ..
                }
                | Op::StashDrop(_)
                | Op::DeleteBranch { force: true, .. }
                | Op::RemoveWorktree { force: true, .. }
                | Op::Abort
        )
    }

    // Whether it talks to a remote, which can take a while.
    pub fn is_remote(&self) -> bool {
        matches!(self, Op::Fetch | Op::Pull | Op::Push { .. })
    }
}
