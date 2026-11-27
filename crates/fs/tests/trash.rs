use std::fs;
use std::path::{Path, PathBuf};

use corvo_fs::trash;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("corvo-trash-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn note(trash: &Path, kept: &str) -> String {
    fs::read_to_string(trash.join("info").join(format!("{kept}.trashinfo"))).unwrap()
}

#[test]
fn a_file_goes_whole_with_a_note_of_where_it_was() {
    let dir = scratch("file");
    let (bin, file) = (dir.join("Trash"), dir.join("my notes.txt"));
    fs::write(&file, "olá").unwrap();

    let kept = trash::send(&file, &bin, "2026-11-27T10:00:00").unwrap();

    assert!(!file.exists());
    assert_eq!(kept, bin.join("files/my notes.txt"));
    assert_eq!(fs::read_to_string(&kept).unwrap(), "olá");
    let note = note(&bin, "my notes.txt");
    assert!(note.starts_with("[Trash Info]\n"));
    assert!(note.contains("/my%20notes.txt\n"), "{note}");
    assert!(note.contains("DeletionDate=2026-11-27T10:00:00\n"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_name_already_in_the_trash_takes_a_number() {
    let dir = scratch("again");
    let bin = dir.join("Trash");
    for _ in 0..3 {
        fs::write(dir.join("a.rs"), "").unwrap();
        trash::send(&dir.join("a.rs"), &bin, "2026-11-27T10:00:00").unwrap();
    }
    fs::write(dir.join(".env"), "").unwrap();
    fs::create_dir_all(bin.join("files/.env")).unwrap();
    trash::send(&dir.join(".env"), &bin, "2026-11-27T10:00:00").unwrap();

    for kept in ["a.rs", "a.2.rs", "a.3.rs", ".env.2"] {
        assert!(bin.join("files").join(kept).exists(), "{kept}");
        assert!(bin.join("info").join(format!("{kept}.trashinfo")).exists());
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_folder_goes_with_what_it_holds() {
    let dir = scratch("folder");
    let bin = dir.join("Trash");
    fs::create_dir_all(dir.join("src/deep")).unwrap();
    fs::write(dir.join("src/deep/lib.rs"), "fn x() {}").unwrap();

    trash::send(&dir.join("src"), &bin, "2026-11-27T10:00:00").unwrap();

    assert!(!dir.join("src").exists());
    assert!(bin.join("files/src/deep/lib.rs").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_is_not_there_leaves_no_note() {
    let dir = scratch("missing");
    let bin = dir.join("Trash");

    assert!(trash::send(&dir.join("nothing"), &bin, "2026-11-27T10:00:00").is_err());

    assert_eq!(fs::read_dir(bin.join("info")).unwrap().count(), 0);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn now_is_written_as_the_spec_asks() {
    let now = trash::now();

    assert_eq!(now.len(), 19, "{now}");
    assert_eq!(&now[4..5], "-");
    assert_eq!(&now[10..11], "T");
}
