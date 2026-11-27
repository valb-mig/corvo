use std::fs;
use std::path::{Path, PathBuf};

use corvo_domain::Node;
use corvo_domain::port::Fs;
use corvo_fs::LocalFs;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("corvo-fs-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn hides_git_and_respects_gitignore() {
    let dir = scratch("ignore");
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::create_dir_all(dir.join("target")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join(".gitignore"), "/target\n").unwrap();
    fs::write(dir.join("main.rs"), "").unwrap();

    let mut nodes = LocalFs.list(&dir).unwrap();
    nodes.sort();
    let names: Vec<_> = nodes.iter().map(Node::name).collect();

    assert_eq!(names, ["src", ".gitignore", "main.rs"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn writes_and_reads_back() {
    let dir = scratch("rw");
    let file = dir.join("a.txt");

    LocalFs.write(&file, "olá").unwrap();
    assert_eq!(LocalFs.read(&file).unwrap(), "olá".as_bytes());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn files_walks_down_and_leaves_out_what_git_ignores() {
    let dir = std::env::temp_dir().join(format!("corvo-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/deep")).unwrap();
    std::fs::create_dir_all(dir.join("target")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join(".gitignore"), "target\n").unwrap();
    std::fs::write(dir.join("src/deep/a.rs"), "").unwrap();
    std::fs::write(dir.join("target/out"), "").unwrap();
    std::fs::write(dir.join(".git/HEAD"), "").unwrap();

    let found: Vec<_> = corvo_fs::files(&dir, 100)
        .into_iter()
        .map(|path| path.strip_prefix(&dir).unwrap().to_path_buf())
        .collect();
    assert_eq!(
        found,
        [".gitignore", "src/deep/a.rs"].map(std::path::PathBuf::from)
    );
    assert_eq!(corvo_fs::files(&dir, 1).len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

// How the listing read before it kept git's rules: a walk of the folder alone by the ignore crate,
// which reads every rule file above it each time. What git ignores must not change.
fn walked(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = ignore::WalkBuilder::new(dir)
        .max_depth(Some(1))
        .hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build()
        .flatten()
        .filter(|entry| entry.depth() == 1)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn listed(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = LocalFs
        .list(dir)
        .unwrap()
        .iter()
        .map(|node| node.name().to_owned())
        .collect();
    names.sort();
    names
}

fn put(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

#[test]
fn the_nearest_rule_file_wins_and_can_bring_a_file_back() {
    let dir = scratch("nested");
    fs::create_dir_all(dir.join(".git/info")).unwrap();
    put(
        &dir,
        &[
            (".gitignore", "*.log\nbuild/\n"),
            ("a.log", ""),
            ("build/out", ""),
            ("src/.gitignore", "!keep.log\n"),
            ("src/keep.log", ""),
            ("src/other.log", ""),
            ("src/build/x", ""),
            (".git/info/exclude", "secret\n"),
            ("src/secret", ""),
        ],
    );

    assert_eq!(listed(&dir), [".gitignore", "src"]);
    assert_eq!(listed(&dir.join("src")), [".gitignore", "keep.log"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn git_rules_stop_at_the_top_of_the_repository_and_need_one() {
    let dir = scratch("above");
    put(
        &dir,
        &[
            (".gitignore", "loose\nfrom_above\n"),
            (".ignore", "skipped\n"),
            ("loose", ""),
            ("skipped", ""),
            ("repo/from_above", ""),
            ("repo/skipped", ""),
            ("repo/kept", ""),
        ],
    );
    fs::create_dir_all(dir.join("repo/.git")).unwrap();

    // No repository around the outer folder: its .gitignore says nothing, its .ignore does.
    assert_eq!(listed(&dir), [".gitignore", ".ignore", "loose", "repo"]);
    // Inside the repository, the .gitignore above its top says nothing either.
    assert_eq!(listed(&dir.join("repo")), ["from_above", "kept"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_rule_file_changed_on_disk_is_read_again() {
    let dir = scratch("changed");
    fs::create_dir_all(dir.join(".git")).unwrap();
    put(&dir, &[(".gitignore", "a\n"), ("a", ""), ("b", "")]);
    assert_eq!(listed(&dir), [".gitignore", "b"]);

    fs::write(dir.join(".gitignore"), "b\n# now b\n").unwrap();

    assert_eq!(listed(&dir), [".gitignore", "a"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn every_folder_lists_what_the_walker_listed() {
    let dir = scratch("parity");
    fs::create_dir_all(dir.join("repo/.git/info")).unwrap();
    put(
        &dir,
        &[
            (".ignore", "*.tmp\n"),
            (
                "repo/.gitignore",
                "/target\n*.o\n!keep.o\ndocs/*.md\n!docs/README.md\nlogs/\n",
            ),
            ("repo/.git/info/exclude", "local.txt\n"),
            ("repo/target/debug/x", ""),
            ("repo/a.o", ""),
            ("repo/keep.o", ""),
            ("repo/x.tmp", ""),
            ("repo/local.txt", ""),
            ("repo/docs/guide.md", ""),
            ("repo/docs/README.md", ""),
            ("repo/docs/logs/today", ""),
            ("repo/src/target/kept.rs", ""),
            ("repo/src/.gitignore", "*.rs\n!main.rs\n"),
            ("repo/src/main.rs", ""),
            ("repo/src/lib.rs", ""),
            ("repo/src/inner/.ignore", "!*.tmp\n"),
            ("repo/src/inner/y.tmp", ""),
            ("repo/src/inner/z.o", ""),
            ("repo/.hidden/file", ""),
        ],
    );

    let mut dirs = vec![dir.clone()];
    while let Some(next) = dirs.pop() {
        assert_eq!(listed(&next), walked(&next), "in {}", next.display());
        for node in LocalFs.list(&next).unwrap() {
            if node.is_dir() {
                dirs.push(node.path().to_path_buf());
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn creating_over_something_says_which_name_is_taken() {
    let dir = scratch("create");
    LocalFs.create_file(&dir.join("a.rs")).unwrap();
    LocalFs.create_dir(&dir.join("src")).unwrap();

    let file = LocalFs.create_file(&dir.join("a.rs")).unwrap_err();
    let folder = LocalFs.create_dir(&dir.join("src")).unwrap_err();

    assert_eq!(file.to_string(), "a.rs is already there");
    assert_eq!(folder.kind(), std::io::ErrorKind::AlreadyExists);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn renaming_never_writes_over_another_file() {
    let dir = scratch("rename");
    fs::write(dir.join("a.rs"), "a").unwrap();
    fs::write(dir.join("b.rs"), "b").unwrap();

    let err = LocalFs
        .rename(&dir.join("a.rs"), &dir.join("b.rs"))
        .unwrap_err();
    LocalFs
        .rename(&dir.join("a.rs"), &dir.join("c.rs"))
        .unwrap();

    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(dir.join("b.rs")).unwrap(), "b");
    assert_eq!(fs::read_to_string(dir.join("c.rs")).unwrap(), "a");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn search_reads_text_files_git_keeps_and_open_ones_as_typed() {
    use std::collections::HashMap;

    use corvo_domain::Query;

    let dir = scratch("search");
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::create_dir_all(dir.join("target")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join(".gitignore"), "/target\n").unwrap();
    fs::write(dir.join("target/out.rs"), "needle").unwrap();
    fs::write(dir.join(".git/config"), "needle").unwrap();
    fs::write(dir.join("blob.bin"), b"needle\0\x01").unwrap();
    fs::write(dir.join("src/b.rs"), "one needle\ntwo needle").unwrap();
    fs::write(dir.join("src/a.rs"), "nothing on disk").unwrap();
    fs::write(dir.join("src/c.rs"), "no").unwrap();
    let open = HashMap::from([(dir.join("src/a.rs"), "needle typed".to_owned())]);
    let finder = Query {
        text: "needle".into(),
        ..Query::default()
    }
    .finder()
    .unwrap()
    .unwrap();

    let found = corvo_fs::search(&dir, &finder, &open, 100, &|| false).unwrap();

    let files: Vec<_> = found
        .files
        .iter()
        .map(|(path, hits)| {
            let count: usize = hits.iter().map(|hit| hit.count).sum();
            (path.strip_prefix(&dir).unwrap().to_owned(), count)
        })
        .collect();
    assert_eq!(
        files,
        [
            (PathBuf::from("src/a.rs"), 1),
            (PathBuf::from("src/b.rs"), 2)
        ]
    );
    assert!(corvo_fs::search(&dir, &finder, &open, 100, &|| true).is_none());
    fs::remove_dir_all(dir).unwrap();
}
