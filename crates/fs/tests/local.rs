use std::fs;
use std::path::PathBuf;

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
