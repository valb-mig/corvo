// What git is run with for each thing the dev asks.

use std::io;

use corvo_domain::{Doing, Op, Reset};

// One run of git: its arguments, and what goes to its input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub args: Vec<String>,
    pub input: Option<String>,
}

fn call<I, S>(args: I) -> Call
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Call {
        args: args.into_iter().map(Into::into).collect(),
        input: None,
    }
}

// Arguments, then `--`, then paths.
fn on<I, S>(args: I, paths: &[String]) -> Call
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut call = call(args);
    call.args.push("--".into());
    call.args.extend(paths.iter().cloned());
    call
}

fn stash(index: usize) -> String {
    format!("stash@{{{index}}}")
}

// The runs an op takes, in order; the first that fails stops the rest. `doing` is what the
// repository has half done, for what goes on, skips or gives up.
pub fn plan(op: &Op, doing: Option<&Doing>) -> io::Result<Vec<Call>> {
    let nothing = || io::Error::other("Nothing is half done in this repository.");
    Ok(match op {
        Op::Stage(paths) => vec![on(["add", "--all"], paths)],
        Op::Unstage(paths) => vec![on(["reset", "-q"], paths)],
        Op::Discard { tracked, untracked } => {
            let mut calls = Vec::new();
            if !tracked.is_empty() {
                calls.push(on(["restore", "--worktree"], tracked));
            }
            if !untracked.is_empty() {
                calls.push(on(["clean", "-f", "-q"], untracked));
            }
            calls
        }
        Op::Apply {
            patch,
            cached,
            reverse,
        } => {
            let mut args = vec!["apply", "--whitespace=nowarn"];
            if *cached {
                args.push("--cached");
            }
            if *reverse {
                args.push("--reverse");
            }
            args.push("-");
            vec![Call {
                input: Some(patch.clone()),
                ..call(args)
            }]
        }
        Op::Commit { message, amend } => {
            let message = message.trim();
            match (message.is_empty(), amend) {
                (true, false) => return Err(io::Error::other("A commit needs a message.")),
                (true, true) => vec![call(["commit", "--amend", "--no-edit"])],
                (false, true) => vec![call(["commit", "--amend", "-m", message])],
                (false, false) => vec![call(["commit", "-m", message])],
            }
        }
        Op::Fetch => vec![call(["fetch", "--all", "--prune"])],
        Op::Pull => vec![call(["pull", "--no-edit"])],
        Op::Push { upstream } => match upstream {
            Some(remote) => vec![call(["push", "-u", remote.as_str(), "HEAD"])],
            None => vec![call(["push"])],
        },
        Op::Checkout(rev) => vec![call(["checkout", "-q", rev.as_str()])],
        Op::Branch {
            name,
            from,
            checkout,
        } => {
            if *checkout {
                vec![call(["switch", "-c", name.as_str(), from.as_str()])]
            } else {
                vec![call(["branch", name.as_str(), from.as_str()])]
            }
        }
        Op::Rename { from, to } => vec![call(["branch", "-m", from.as_str(), to.as_str()])],
        Op::DeleteBranch { name, force } => {
            let flag = if *force { "-D" } else { "-d" };
            vec![call(["branch", flag, name.as_str()])]
        }
        Op::Merge(rev) => vec![call(["merge", "--no-edit", rev.as_str()])],
        Op::Rebase(onto) => vec![call(["rebase", onto.as_str()])],
        Op::CherryPick(rev) => vec![call(["cherry-pick", rev.as_str()])],
        Op::Revert(rev) => vec![call(["revert", "--no-edit", rev.as_str()])],
        Op::ResetTo { rev, how } => {
            let how = match how {
                Reset::Soft => "--soft",
                Reset::Mixed => "--mixed",
                Reset::Hard => "--hard",
            };
            vec![call(["reset", "-q", how, rev.as_str()])]
        }
        Op::Tag { name, at } => vec![call(["tag", name.as_str(), at.as_str()])],
        Op::DeleteTag(name) => vec![call(["tag", "-d", name.as_str()])],
        Op::Stash {
            message,
            untracked,
            keep_index,
        } => {
            let mut args = vec!["stash".to_owned(), "push".to_owned()];
            if *untracked {
                args.push("--include-untracked".into());
            }
            if *keep_index {
                args.push("--keep-index".into());
            }
            if !message.trim().is_empty() {
                args.push("-m".into());
                args.push(message.trim().to_owned());
            }
            vec![call(args)]
        }
        Op::StashApply(ix) => vec![call(["stash".into(), "apply".into(), stash(*ix)])],
        Op::StashPop(ix) => vec![call(["stash".into(), "pop".into(), stash(*ix)])],
        Op::StashDrop(ix) => vec![call(["stash".into(), "drop".into(), stash(*ix)])],
        Op::StashBranch { name, index } => {
            vec![call([
                "stash".into(),
                "branch".into(),
                name.clone(),
                stash(*index),
            ])]
        }
        Op::AddWorktree { path, branch, from } => {
            let path = path.to_string_lossy().into_owned();
            match from {
                Some(from) => vec![call([
                    "worktree".to_owned(),
                    "add".into(),
                    "-b".into(),
                    branch.clone(),
                    path,
                    from.clone(),
                ])],
                None => vec![call([
                    "worktree".to_owned(),
                    "add".into(),
                    path,
                    branch.clone(),
                ])],
            }
        }
        Op::RemoveWorktree { path, force } => {
            let mut args = vec!["worktree".to_owned(), "remove".into()];
            if *force {
                args.push("--force".into());
            }
            args.push(path.to_string_lossy().into_owned());
            vec![call(args)]
        }
        Op::PruneWorktrees => vec![call(["worktree", "prune"])],
        Op::Continue => vec![match doing.ok_or_else(nothing)? {
            // The merge is done once committed, with the message git prepared.
            Doing::Merge { .. } => call(["commit", "--no-edit"]),
            Doing::Rebase { .. } => call(["rebase", "--continue"]),
            Doing::CherryPick { .. } => call(["cherry-pick", "--continue"]),
            Doing::Revert { .. } => call(["revert", "--continue"]),
        }],
        Op::Skip => vec![match doing.ok_or_else(nothing)? {
            Doing::Rebase { .. } => call(["rebase", "--skip"]),
            Doing::CherryPick { .. } => call(["cherry-pick", "--skip"]),
            _ => return Err(io::Error::other("Only a rebase or a cherry-pick skips.")),
        }],
        Op::Abort => vec![match doing.ok_or_else(nothing)? {
            Doing::Merge { .. } => call(["merge", "--abort"]),
            Doing::Rebase { .. } => call(["rebase", "--abort"]),
            Doing::CherryPick { .. } => call(["cherry-pick", "--abort"]),
            Doing::Revert { .. } => call(["revert", "--abort"]),
        }],
        Op::TakeSide { path, ours } => {
            let side = if *ours { "--ours" } else { "--theirs" };
            let paths = std::slice::from_ref(path);
            vec![on(["checkout", side], paths), on(["add"], paths)]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(op: Op) -> Vec<Vec<String>> {
        plan(&op, None)
            .unwrap()
            .into_iter()
            .map(|call| call.args)
            .collect()
    }

    #[test]
    fn stages_and_unstages_paths_after_a_separator() {
        assert_eq!(
            args(Op::Stage(vec!["-a.rs".into()])),
            [["add", "--all", "--", "-a.rs"]]
        );
        assert_eq!(
            args(Op::Unstage(vec!["a.rs".into()])),
            [["reset", "-q", "--", "a.rs"]]
        );
    }

    #[test]
    fn discards_tracked_and_untracked_files_each_their_way() {
        let op = Op::Discard {
            tracked: vec!["a.rs".into()],
            untracked: vec!["new.rs".into()],
        };
        assert_eq!(
            args(op),
            [
                vec!["restore", "--worktree", "--", "a.rs"],
                vec!["clean", "-f", "-q", "--", "new.rs"]
            ]
        );
    }

    #[test]
    fn a_patch_goes_to_git_on_its_input() {
        let op = Op::Apply {
            patch: "diff".into(),
            cached: true,
            reverse: true,
        };
        let calls = plan(&op, None).unwrap();
        assert_eq!(
            calls[0].args,
            ["apply", "--whitespace=nowarn", "--cached", "--reverse", "-"]
        );
        assert_eq!(calls[0].input.as_deref(), Some("diff"));
    }

    #[test]
    fn a_commit_needs_a_message_unless_it_amends() {
        let commit = |message: &str, amend| Op::Commit {
            message: message.into(),
            amend,
        };
        assert!(plan(&commit("  ", false), None).is_err());
        assert_eq!(args(commit("", true)), [["commit", "--amend", "--no-edit"]]);
        assert_eq!(
            args(commit(" fix it ", false)),
            [["commit", "-m", "fix it"]]
        );
    }

    #[test]
    fn going_on_depends_on_what_is_half_done() {
        let merge = Doing::Merge { theirs: "x".into() };
        let rebase = Doing::Rebase {
            branch: "x".into(),
            onto: "y".into(),
            step: 1,
            steps: 2,
        };
        let first = |op: Op, doing: &Doing| plan(&op, Some(doing)).unwrap()[0].args.clone();
        assert_eq!(first(Op::Continue, &merge), ["commit", "--no-edit"]);
        assert_eq!(first(Op::Continue, &rebase), ["rebase", "--continue"]);
        assert_eq!(first(Op::Abort, &merge), ["merge", "--abort"]);
        assert_eq!(first(Op::Skip, &rebase), ["rebase", "--skip"]);
        assert!(plan(&Op::Skip, Some(&merge)).is_err());
        assert!(plan(&Op::Abort, None).is_err());
    }

    #[test]
    fn a_new_worktree_takes_a_branch_there_is_or_makes_one() {
        let made = Op::AddWorktree {
            path: "/w/x".into(),
            branch: "feat/x".into(),
            from: Some("main".into()),
        };
        assert_eq!(
            args(made),
            [["worktree", "add", "-b", "feat/x", "/w/x", "main"]]
        );
        let there = Op::AddWorktree {
            path: "/w/x".into(),
            branch: "feat/x".into(),
            from: None,
        };
        assert_eq!(args(there), [["worktree", "add", "/w/x", "feat/x"]]);
    }

    #[test]
    fn a_side_taken_is_marked_resolved() {
        let op = Op::TakeSide {
            path: "a.rs".into(),
            ours: false,
        };
        assert_eq!(
            args(op),
            [
                vec!["checkout", "--theirs", "--", "a.rs"],
                vec!["add", "--", "a.rs"]
            ]
        );
    }
}
