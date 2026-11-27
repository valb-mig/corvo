use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use corvo_domain::port::{Git, Side};
use corvo_domain::{Change, Doing, Log, LogScope, Op, RefName};
use corvo_git::GitCli;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("corvo-git-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) {
    let done = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=corvo", "-c", "user.email=corvo@test"])
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
}

// A repository with one commit holding a.rs and lib/b.rs.
fn repo(name: &str) -> PathBuf {
    let dir = scratch(name);
    run(&dir, &["init", "-q"]);
    // What corvo runs itself takes its identity from the repository.
    run(&dir, &["config", "user.name", "corvo"]);
    run(&dir, &["config", "user.email", "corvo@test"]);
    run(&dir, &["config", "commit.gpgsign", "false"]);
    fs::write(dir.join("a.rs"), "one\ntwo\n").unwrap();
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::write(dir.join("lib/b.rs"), "b\n").unwrap();
    run(&dir, &["add", "."]);
    run(&dir, &["commit", "-q", "-m", "first"]);
    dir
}

#[test]
fn a_folder_outside_a_repository_has_none() {
    let dir = scratch("none");
    assert!(GitCli.repo(&dir).unwrap().is_none());
}

#[test]
fn reads_the_branch_and_what_changed_from_any_folder_inside() {
    let dir = repo("status");
    fs::write(dir.join("a.rs"), "one\n2\n").unwrap();
    fs::remove_file(dir.join("lib/b.rs")).unwrap();
    fs::write(dir.join("lib/c.rs"), "c\n").unwrap();

    let repo = GitCli.repo(&dir.join("lib")).unwrap().unwrap();
    assert_eq!(repo.root(), dir);
    assert_eq!(repo.branch(), "main");
    assert!(repo.head().is_some());
    assert_eq!(repo.change_of(&dir.join("a.rs")), Some(Change::Modified));
    assert_eq!(repo.change_of(&dir.join("lib/b.rs")), Some(Change::Deleted));
    assert_eq!(
        repo.change_of(&dir.join("lib/c.rs")),
        Some(Change::Untracked)
    );
    assert_eq!(repo.change_of(&dir.join("lib")), Some(Change::Deleted));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gives_a_file_as_the_last_commit_has_it() {
    let dir = repo("base");
    fs::write(dir.join("a.rs"), "changed\n").unwrap();
    fs::write(dir.join("new.rs"), "new\n").unwrap();
    assert_eq!(
        GitCli.base(&dir, &dir.join("a.rs")).unwrap().as_deref(),
        Some("one\ntwo\n")
    );
    assert_eq!(GitCli.base(&dir, &dir.join("new.rs")).unwrap(), None);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn patches_a_changed_file_and_a_new_one() {
    let dir = repo("patch");
    fs::write(dir.join("a.rs"), "one\n2\n").unwrap();
    fs::write(dir.join("new.rs"), "new\n").unwrap();
    let changed = GitCli.patch(&dir, &dir.join("a.rs")).unwrap();
    assert!(changed.contains("-two\n+2\n"), "{changed}");
    let new = GitCli.patch(&dir, &dir.join("new.rs")).unwrap();
    assert!(new.contains("+new\n"), "{new}");
    let _ = fs::remove_dir_all(dir);
}

fn commit_all(dir: &Path, message: &str) {
    run(dir, &["add", "--all"]);
    run(dir, &["commit", "-q", "-m", message]);
}

fn op(dir: &Path, op: Op) {
    GitCli.run(dir, &op).unwrap();
}

#[test]
fn stages_unstages_and_commits() {
    let dir = repo("stage");
    fs::write(dir.join("a.rs"), "one\n2\n").unwrap();
    fs::write(dir.join("new.rs"), "new\n").unwrap();
    op(&dir, Op::Stage(vec!["a.rs".into()]));
    let repo = GitCli.repo(&dir).unwrap().unwrap();
    let stage = |name: &str| {
        repo.changes()
            .iter()
            .find(|c| c.path.ends_with(name))
            .unwrap()
            .stage
    };
    assert!(stage("a.rs").staged() && !stage("a.rs").unstaged());
    assert!(!stage("new.rs").staged() && stage("new.rs").unstaged());
    let staged = GitCli
        .side_patch(&dir, &dir.join("a.rs"), Side::Staged)
        .unwrap();
    assert!(staged.contains("+2"), "{staged}");
    let new = GitCli
        .side_patch(&dir, &dir.join("new.rs"), Side::Unstaged)
        .unwrap();
    assert!(new.contains("+new"), "{new}");

    op(&dir, Op::Unstage(vec!["a.rs".into()]));
    let repo = GitCli.repo(&dir).unwrap().unwrap();
    assert!(repo.changes().iter().all(|c| !c.stage.staged()));

    op(&dir, Op::Stage(vec!["a.rs".into(), "new.rs".into()]));
    op(
        &dir,
        Op::Commit {
            message: "second".into(),
            amend: false,
        },
    );
    let repo = GitCli.repo(&dir).unwrap().unwrap();
    assert!(repo.changes().is_empty());
    let log = GitCli.log(&dir, &Log::new(LogScope::All, "")).unwrap();
    assert_eq!(log[0].subject, "second");
    assert_eq!(log[0].refs, [RefName::Head("main".into())]);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn discards_changes_and_untracked_files() {
    let dir = repo("discard");
    fs::write(dir.join("a.rs"), "gone\n").unwrap();
    fs::write(dir.join("new.rs"), "new\n").unwrap();
    op(
        &dir,
        Op::Discard {
            tracked: vec!["a.rs".into()],
            untracked: vec!["new.rs".into()],
        },
    );
    assert_eq!(fs::read_to_string(dir.join("a.rs")).unwrap(), "one\ntwo\n");
    assert!(!dir.join("new.rs").exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn stages_one_hunk_of_a_file() {
    let dir = repo("hunk");
    let lines: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    fs::write(dir.join("a.rs"), lines.join("\n") + "\n").unwrap();
    commit_all(&dir, "twenty");
    let mut changed = lines.clone();
    changed[1] = "line two".into();
    changed[18] = "line nineteen".into();
    fs::write(dir.join("a.rs"), changed.join("\n") + "\n").unwrap();
    // The first hunk only, as the window cuts it out of the file's patch.
    let patch = GitCli
        .side_patch(&dir, &dir.join("a.rs"), Side::Unstaged)
        .unwrap();
    let second = patch
        .match_indices("\n@@")
        .nth(1)
        .map(|(ix, _)| ix + 1)
        .unwrap();
    op(
        &dir,
        Op::Apply {
            patch: patch[..second].to_owned(),
            cached: true,
            reverse: false,
        },
    );
    let staged = GitCli
        .side_patch(&dir, &dir.join("a.rs"), Side::Staged)
        .unwrap();
    assert!(
        staged.contains("+line two") && !staged.contains("nineteen"),
        "{staged}"
    );
    let left = GitCli
        .side_patch(&dir, &dir.join("a.rs"), Side::Unstaged)
        .unwrap();
    assert!(
        left.contains("+line nineteen") && !left.contains("line two"),
        "{left}"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn reads_a_commit_its_patch_and_who_holds_it() {
    let dir = repo("detail");
    fs::write(dir.join("a.rs"), "one\n2\n").unwrap();
    fs::rename(dir.join("lib/b.rs"), dir.join("lib/bee.rs")).unwrap();
    run(&dir, &["add", "--all"]);
    run(
        &dir,
        &["commit", "-q", "-m", "change it\n\nWhy it changed."],
    );
    let detail = GitCli.detail(&dir, "HEAD").unwrap();
    assert_eq!(detail.commit.subject, "change it");
    assert_eq!(detail.body, "Why it changed.");
    let renamed = detail
        .files
        .iter()
        .find(|f| f.change == Change::Renamed)
        .unwrap();
    assert_eq!(
        (renamed.path.as_str(), renamed.from.as_deref()),
        ("lib/bee.rs", Some("lib/b.rs"))
    );
    let a = detail.files.iter().find(|f| f.path == "a.rs").unwrap();
    assert_eq!(a.lines, Some((1, 1)));
    assert!(GitCli.show(&dir, "HEAD").unwrap().contains("+2"));
    // The first commit has no parent to set against.
    let first = GitCli.detail(&dir, "HEAD~1").unwrap();
    assert_eq!(first.files.len(), 2);
    assert!(GitCli.show(&dir, "HEAD~1").unwrap().contains("+one"));
    let holders = GitCli.holders(&dir, "HEAD").unwrap();
    assert_eq!(holders.local, ["main"]);
    assert!(holders.remote.is_empty());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn finds_commits_by_message_author_hash_and_path() {
    let dir = repo("find");
    fs::write(dir.join("titlebar.rs"), "bar\n").unwrap();
    commit_all(&dir, "the bar on top");
    fs::write(dir.join("a.rs"), "x\n").unwrap();
    commit_all(&dir, "something else");
    let find = |text: &str| {
        GitCli
            .log(&dir, &Log::new(LogScope::All, text))
            .unwrap()
            .into_iter()
            .map(|c| c.subject)
            .collect::<Vec<_>>()
    };
    assert_eq!(find("BAR ON"), ["the bar on top"]);
    assert_eq!(find("titlebar"), ["the bar on top"]);
    assert_eq!(find("corvo").len(), 3);
    let first = GitCli.log(&dir, &Log::new(LogScope::All, "first")).unwrap();
    assert_eq!(find(&first[0].hash[..7]), ["first"]);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compares_a_branch_and_tells_what_would_conflict() {
    let dir = repo("compare");
    run(&dir, &["switch", "-q", "-c", "feat"]);
    fs::write(dir.join("a.rs"), "one\nfeat\n").unwrap();
    fs::write(dir.join("feat.rs"), "f\n").unwrap();
    commit_all(&dir, "feat it");
    run(&dir, &["switch", "-q", "main"]);
    let clean = GitCli.compare(&dir, "main", "feat").unwrap();
    assert_eq!(clean.commits.len(), 1);
    assert_eq!(clean.files.len(), 2);
    assert!(clean.conflicts.is_empty());
    fs::write(dir.join("a.rs"), "one\nmain\n").unwrap();
    commit_all(&dir, "main too");
    let clashing = GitCli.compare(&dir, "main", "feat").unwrap();
    assert_eq!(clashing.conflicts, ["a.rs"]);
    assert_eq!(clashing.behind, 1);
    assert!(
        GitCli
            .compare_patch(&dir, "main", "feat")
            .unwrap()
            .contains("+feat")
    );

    let refs = GitCli.refs(&dir).unwrap();
    assert_eq!(refs.main.as_deref(), Some("main"));
    assert_eq!(refs.local[0].name, "main");
    assert!(refs.local[0].head);
    let feat = refs.branch("feat").unwrap();
    assert_eq!(feat.base.map(|a| (a.ahead, a.behind)), Some((1, 1)));
    assert!(!feat.merged);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_merge_stops_on_a_conflict_and_goes_on_once_resolved() {
    let dir = repo("merge");
    run(&dir, &["switch", "-q", "-c", "feat"]);
    fs::write(dir.join("a.rs"), "one\nfeat\n").unwrap();
    commit_all(&dir, "feat it");
    run(&dir, &["switch", "-q", "main"]);
    fs::write(dir.join("a.rs"), "one\nmain\n").unwrap();
    commit_all(&dir, "main too");

    let stopped = GitCli.run(&dir, &Op::Merge("feat".into()));
    assert!(stopped.unwrap_err().to_string().contains("CONFLICT"));
    let repo = GitCli.repo(&dir).unwrap().unwrap();
    assert_eq!(
        repo.doing(),
        Some(&Doing::Merge {
            theirs: "feat".into()
        })
    );
    assert_eq!(repo.conflicts().count(), 1);
    op(
        &dir,
        Op::TakeSide {
            path: "a.rs".into(),
            ours: false,
        },
    );
    assert_eq!(fs::read_to_string(dir.join("a.rs")).unwrap(), "one\nfeat\n");
    op(&dir, Op::Continue);
    let repo = GitCli.repo(&dir).unwrap().unwrap();
    assert_eq!(repo.doing(), None);
    let log = GitCli
        .log(&dir, &Log::new(LogScope::Rev("main".into()), ""))
        .unwrap();
    assert!(log[0].is_merge());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn an_aborted_rebase_leaves_the_branch_as_it_was() {
    let dir = repo("rebase");
    run(&dir, &["switch", "-q", "-c", "feat"]);
    fs::write(dir.join("a.rs"), "one\nfeat\n").unwrap();
    commit_all(&dir, "feat it");
    run(&dir, &["switch", "-q", "main"]);
    fs::write(dir.join("a.rs"), "one\nmain\n").unwrap();
    commit_all(&dir, "main too");
    run(&dir, &["switch", "-q", "feat"]);
    assert!(GitCli.run(&dir, &Op::Rebase("main".into())).is_err());
    let repo = GitCli.repo(&dir).unwrap().unwrap();
    assert!(matches!(
        repo.doing(),
        Some(Doing::Rebase {
            step: 1,
            steps: 1,
            ..
        })
    ));
    op(&dir, Op::Abort);
    let repo = GitCli.repo(&dir).unwrap().unwrap();
    assert_eq!((repo.doing(), repo.branch()), (None, "feat"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn stashes_and_brings_work_back() {
    let dir = repo("stash");
    fs::write(dir.join("a.rs"), "one\nwip\n").unwrap();
    fs::write(dir.join("new.rs"), "new\n").unwrap();
    op(
        &dir,
        Op::Stash {
            message: "half of it".into(),
            untracked: true,
            keep_index: false,
        },
    );
    let repo = GitCli.repo(&dir).unwrap().unwrap();
    assert!(repo.changes().is_empty());
    assert_eq!(repo.stashes(), 1);
    let stashes = GitCli.stashes(&dir).unwrap();
    assert_eq!(
        (stashes[0].message.as_str(), stashes[0].branch.as_str()),
        ("half of it", "main")
    );
    let files = GitCli.stash_files(&dir, 0).unwrap();
    assert_eq!(files.len(), 2);
    assert!(GitCli.stash_patch(&dir, 0).unwrap().contains("+wip"));
    op(&dir, Op::StashPop(0));
    assert_eq!(fs::read_to_string(dir.join("a.rs")).unwrap(), "one\nwip\n");
    assert!(dir.join("new.rs").exists());
    assert!(GitCli.stashes(&dir).unwrap().is_empty());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn adds_lists_and_removes_worktrees() {
    let dir = repo("worktree");
    let tree = dir.with_file_name(format!(
        "{}-tree",
        dir.file_name().unwrap().to_string_lossy()
    ));
    let _ = fs::remove_dir_all(&tree);
    op(
        &dir,
        Op::AddWorktree {
            path: tree.clone(),
            branch: "feat".into(),
            from: Some("main".into()),
        },
    );
    let trees = GitCli.worktrees(&dir).unwrap();
    assert_eq!(trees.len(), 2);
    assert!(trees[0].main);
    assert_eq!(trees[1].branch.as_deref(), Some("feat"));
    let refs = GitCli.refs(&dir).unwrap();
    assert_eq!(
        refs.branch("feat").and_then(|b| b.worktree.as_deref()),
        Some(tree.as_path())
    );
    op(
        &dir,
        Op::RemoveWorktree {
            path: tree.clone(),
            force: false,
        },
    );
    assert_eq!(GitCli.worktrees(&dir).unwrap().len(), 1);
    let _ = fs::remove_dir_all(dir);
    let _ = fs::remove_dir_all(tree);
}
