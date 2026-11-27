use std::path::{Path, PathBuf};

use corvo_domain::Tabs;

fn names(tabs: &Tabs) -> Vec<&str> {
    tabs.docs().iter().map(|doc| doc.name()).collect()
}

#[test]
fn open_twice_keeps_one_tab() {
    let mut tabs = Tabs::new();
    tabs.open(Path::new("/a.rs"));
    tabs.open(Path::new("/b.rs"));
    tabs.open(Path::new("/a.rs"));

    assert_eq!(names(&tabs), ["a.rs", "b.rs"]);
    assert_eq!(tabs.active().map(|doc| doc.name()), Some("a.rs"));
}

#[test]
fn close_active_moves_to_neighbor() {
    let mut tabs = Tabs::new();
    for path in ["/a.rs", "/b.rs", "/c.rs"] {
        tabs.open(Path::new(path));
    }
    tabs.focus(Path::new("/b.rs"));
    tabs.close(Path::new("/b.rs"));
    assert_eq!(tabs.active().map(|doc| doc.name()), Some("c.rs"));

    tabs.close(Path::new("/c.rs"));
    assert_eq!(tabs.active().map(|doc| doc.name()), Some("a.rs"));

    tabs.close(Path::new("/a.rs"));
    assert!(tabs.active().is_none());
}

#[test]
fn close_before_active_keeps_active() {
    let mut tabs = Tabs::new();
    for path in ["/a.rs", "/b.rs", "/c.rs"] {
        tabs.open(Path::new(path));
    }
    tabs.close(Path::new("/a.rs"));
    assert_eq!(tabs.active().map(|doc| doc.name()), Some("c.rs"));
}

#[test]
fn touch_and_clean() {
    let mut tabs = Tabs::new();
    let path = Path::new("/a.rs");
    tabs.open(path);

    tabs.touch(path);
    assert!(tabs.get(path).is_some_and(|doc| doc.is_dirty()));

    tabs.clean(path);
    assert!(tabs.get(path).is_some_and(|doc| !doc.is_dirty()));
}

#[test]
fn tabs_under_a_moved_folder_follow_it_and_no_others() {
    let mut tabs = Tabs::new();
    for path in ["/p/src/a.rs", "/p/srcs/b.rs", "/p/src/deep/c.rs"] {
        tabs.open(Path::new(path));
    }
    tabs.touch(Path::new("/p/src/a.rs"));

    let moves = tabs.moved(Path::new("/p/src"), Path::new("/p/core"));

    assert_eq!(
        moves,
        [
            (PathBuf::from("/p/src/a.rs"), PathBuf::from("/p/core/a.rs")),
            (
                PathBuf::from("/p/src/deep/c.rs"),
                PathBuf::from("/p/core/deep/c.rs")
            ),
        ]
    );
    assert!(tabs.get(Path::new("/p/core/a.rs")).unwrap().is_dirty());
    assert!(tabs.contains(Path::new("/p/srcs/b.rs")));
}

#[test]
fn a_moved_file_takes_the_language_of_its_new_name() {
    let mut tabs = Tabs::new();
    tabs.open(Path::new("/p/notes.txt"));

    tabs.moved(Path::new("/p/notes.txt"), Path::new("/p/notes.rs"));

    assert_eq!(tabs.active().unwrap().lang(), corvo_domain::Lang::Rust);
    assert_eq!(tabs.active().unwrap().path().to_str(), Some("/p/notes.rs"));
}

#[test]
fn closing_under_a_folder_closes_only_what_it_holds() {
    let mut tabs = Tabs::new();
    for path in ["/p/src/a.rs", "/p/srcs/b.rs", "/p/src/c.rs"] {
        tabs.open(Path::new(path));
    }

    let closed = tabs.close_under(Path::new("/p/src"));

    assert_eq!(
        closed,
        [PathBuf::from("/p/src/a.rs"), PathBuf::from("/p/src/c.rs")]
    );
    assert_eq!(names(&tabs), ["b.rs"]);
    assert_eq!(tabs.active().map(|doc| doc.name()), Some("b.rs"));
}
