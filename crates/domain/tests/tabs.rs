use std::path::Path;

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
