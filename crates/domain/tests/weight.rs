use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use corvo_domain::port::Fs;
use corvo_domain::{Node, Weight, bytes, weigh};

// Files by path, with their sizes; a folder is there while some file is under it.
struct Disk(BTreeMap<PathBuf, u64>);

impl Fs for Disk {
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>> {
        let mut nodes: Vec<Node> = Vec::new();
        for path in self.0.keys() {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
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
        Ok(nodes)
    }

    fn read(&self, _: &Path) -> io::Result<Vec<u8>> {
        Err(io::ErrorKind::NotFound.into())
    }

    fn write(&self, _: &Path, _: &str) -> io::Result<()> {
        Ok(())
    }

    fn size(&self, path: &Path) -> io::Result<u64> {
        self.0
            .get(path)
            .copied()
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

fn disk(files: &[(&str, u64)]) -> Disk {
    Disk(
        files
            .iter()
            .map(|(path, size)| (PathBuf::from(path), *size))
            .collect(),
    )
}

fn whole(bytes: u64) -> Option<Weight> {
    Some(Weight { bytes, whole: true })
}

#[test]
fn every_folder_weighs_all_it_holds() {
    let fs = disk(&[
        ("/p/Cargo.toml", 100),
        ("/p/src/main.rs", 1000),
        ("/p/src/home/mod.rs", 300),
        ("/p/src/home/list.rs", 200),
    ]);

    let weights = weigh(&fs, Path::new("/p"));

    assert_eq!(weights.of(Path::new("/p")), whole(1600));
    assert_eq!(weights.of(Path::new("/p/src")), whole(1500));
    assert_eq!(weights.of(Path::new("/p/src/home")), whole(500));
    assert_eq!(weights.of(Path::new("/elsewhere")), None);
}

#[test]
fn vendored_and_built_folders_are_left_out() {
    let fs = disk(&[
        ("/p/index.ts", 10),
        ("/p/node_modules/x/index.js", 5_000_000),
        ("/p/target/debug/p", 9_000_000),
    ]);

    let weights = weigh(&fs, Path::new("/p"));

    assert_eq!(weights.of(Path::new("/p")), whole(10));
    assert_eq!(weights.of(Path::new("/p/node_modules")), None);
}

#[test]
fn bytes_read_short() {
    assert_eq!(bytes(0), "0 B");
    assert_eq!(bytes(812), "812 B");
    assert_eq!(bytes(1024), "1.0 KB");
    assert_eq!(bytes(58_368), "57 KB");
    assert_eq!(bytes(1_363_148), "1.3 MB");
    assert_eq!(bytes(1_288_490_189), "1.2 GB");
    assert_eq!(bytes(5 * 1024 * 1024 * 1024 * 1024), "5120 GB");
}
