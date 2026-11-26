use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use corvo_domain::Change;
use corvo_domain::port::Git;
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
        .args(["-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
}

// A repository with one commit holding a.rs and lib/b.rs.
fn repo(name: &str) -> PathBuf {
    let dir = scratch(name);
    run(&dir, &["init", "-q"]);
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
    assert_eq!(repo.change_of(&dir.join("lib/c.rs")), Some(Change::Untracked));
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
