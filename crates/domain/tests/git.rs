use std::path::{Path, PathBuf};

use corvo_domain::{Change, Changed, Hunk, HunkMark, Repo, hunks};

fn hunk(mark: HunkMark, lines: std::ops::Range<usize>) -> Hunk {
    Hunk { mark, lines }
}

#[test]
fn the_same_text_has_no_hunks() {
    assert!(hunks("a\nb\n", "a\nb\n").is_empty());
}

#[test]
fn lines_put_in_are_added() {
    assert_eq!(
        hunks("a\nc\n", "a\nb\nc\n"),
        vec![hunk(HunkMark::Added, 1..2)]
    );
}

#[test]
fn lines_taken_out_mark_where_they_were() {
    assert_eq!(
        hunks("a\nb\nc\n", "a\nc\n"),
        vec![hunk(HunkMark::Deleted, 1..1)]
    );
}

#[test]
fn lines_written_over_are_modified() {
    assert_eq!(
        hunks("a\nb\nc\nd\n", "a\nB\nc\nd\ne\n"),
        vec![hunk(HunkMark::Modified, 1..2), hunk(HunkMark::Added, 4..5)]
    );
}

#[test]
fn a_last_line_with_no_newline_is_the_same_line() {
    assert!(hunks("a\nb", "a\nb\n").is_empty());
}

fn changed(path: &str, change: Change) -> Changed {
    Changed {
        path: PathBuf::from(path),
        change,
        from: None,
    }
}

fn repo() -> Repo {
    Repo::new(
        "/p",
        "main",
        Some("abc".into()),
        vec![
            changed("/p/src/b.rs", Change::Untracked),
            changed("/p/src/a.rs", Change::Modified),
            changed("/p/README.md", Change::Added),
        ],
    )
}

#[test]
fn files_keep_their_change_and_come_sorted() {
    let repo = repo();
    assert_eq!(repo.change_of(Path::new("/p/src/a.rs")), Some(Change::Modified));
    assert_eq!(repo.change_of(Path::new("/p/src/c.rs")), None);
    let paths: Vec<_> = repo.changes().iter().map(|c| c.path.clone()).collect();
    assert_eq!(
        paths,
        ["/p/README.md", "/p/src/a.rs", "/p/src/b.rs"].map(PathBuf::from)
    );
}

#[test]
fn a_folder_takes_the_weightiest_change_under_it() {
    let repo = repo();
    assert_eq!(repo.change_of(Path::new("/p/src")), Some(Change::Modified));
    assert_eq!(repo.change_of(Path::new("/p")), Some(Change::Modified));
    assert_eq!(repo.change_of(Path::new("/")), None);
}

#[test]
fn holds_only_what_is_under_its_root() {
    let repo = repo();
    assert!(repo.holds(Path::new("/p/src/a.rs")));
    assert!(!repo.holds(Path::new("/q/a.rs")));
}
