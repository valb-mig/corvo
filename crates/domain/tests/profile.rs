use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use corvo_domain::port::Fs;
use corvo_domain::server::Registry;
use corvo_domain::{Node, mix, profile};

// Files by path; a folder is there while some file is under it.
struct Disk(BTreeSet<PathBuf>);

impl Fs for Disk {
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>> {
        let mut nodes: Vec<Node> = Vec::new();
        let mut found = false;
        for path in &self.0 {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            found = true;
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let child = dir.join(first);
            let node = if parts.next().is_some() {
                Node::dir(child)
            } else {
                Node::file(child)
            };
            if !nodes.contains(&node) {
                nodes.push(node);
            }
        }
        if !found {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Ok(nodes)
    }

    fn read(&self, _: &Path) -> io::Result<Vec<u8>> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, _: &Path, _: &str) -> io::Result<()> {
        Ok(())
    }
}

fn disk(paths: &[&str]) -> Disk {
    Disk(paths.iter().map(PathBuf::from).collect())
}

fn stamps(found: &corvo_domain::Profile) -> Vec<(Option<&str>, &str)> {
    found
        .stamps()
        .iter()
        .map(|stamp| (stamp.dir(), stamp.stack()))
        .collect()
}

#[test]
fn a_project_with_a_marker_has_one_stamp() {
    let fs = disk(&[
        "/p/Cargo.toml",
        "/p/src/main.rs",
        "/p/src/lib.rs",
        "/p/x.py",
    ]);

    let found = profile(&fs, Path::new("/p"), &Registry::default());

    assert!(!found.is_missing());
    assert_eq!(stamps(&found), [(None, "Rust")]);
    assert_eq!(found.shares()[0].name(), "Rust");
    assert_eq!(found.files(), 3);
    assert!(found.is_whole());
}

#[test]
fn a_monorepo_is_stamped_by_its_folders() {
    let fs = disk(&[
        "/w/README.md",
        "/w/web/package.json",
        "/w/web/src/app.tsx",
        "/w/engine/pyproject.toml",
        "/w/engine/main.py",
        "/w/docs/index.md",
        "/w/node_modules/x/package.json",
        "/w/.github/package.json",
    ]);

    let found = profile(&fs, Path::new("/w"), &Registry::default());

    assert_eq!(
        stamps(&found),
        [(Some("engine"), "Python"), (Some("web"), "Node")]
    );
}

#[test]
fn a_folder_that_is_gone_is_missing() {
    let fs = disk(&["/other/Cargo.toml"]);

    let found = profile(&fs, Path::new("/p"), &Registry::default());

    assert!(found.is_missing());
    assert!(found.stamps().is_empty());
    assert!(found.shares().is_empty());
}

#[test]
fn mix_adds_up_the_languages_of_several_projects() {
    let a = profile(
        &disk(&["/a/x.rs", "/a/y.rs", "/a/z.ts"]),
        Path::new("/a"),
        &Registry::default(),
    );
    let b = profile(
        &disk(&["/b/x.ts", "/b/y.ts", "/b/z.ts"]),
        Path::new("/b"),
        &Registry::default(),
    );

    let mixed: Vec<(&str, usize)> = mix([&a, &b])
        .iter()
        .map(|share| (share.name(), share.files()))
        .collect();

    assert_eq!(mixed, [("TypeScript", 4), ("Rust", 2)]);
}
