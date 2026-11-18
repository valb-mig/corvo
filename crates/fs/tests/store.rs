use std::fs;
use std::path::{Path, PathBuf};

use corvo_domain::Workspace;
use corvo_domain::port::Store;
use corvo_fs::JsonStore;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("corvo-store-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn missing_file_loads_empty() {
    let dir = scratch("missing");
    let store = JsonStore::new(dir.join("workspaces.json"));

    assert!(store.load().unwrap().is_empty());
}

#[test]
fn saves_and_loads_back() {
    let dir = scratch("roundtrip");
    let store = JsonStore::new(dir.join("corvo/workspaces.json"));
    let items = vec![Workspace::new(1, "work", vec![PathBuf::from("/p")])];

    store.save(&items).unwrap();

    assert_eq!(store.load().unwrap(), items);
    assert!(!Path::new(&dir.join("corvo/workspaces.json.tmp")).exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_file_is_an_error() {
    let dir = scratch("corrupt");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspaces.json");
    fs::write(&path, "{ not json").unwrap();

    assert!(JsonStore::new(&path).load().is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
    fs::remove_dir_all(dir).unwrap();
}
