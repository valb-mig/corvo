use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::Store;
use corvo_domain::{Arrangement, Workspace, Workspaces, owner};

#[derive(Clone, Default)]
struct Memory {
    saved: Rc<RefCell<Vec<Workspace>>>,
}

impl Store for Memory {
    fn load(&self) -> io::Result<Vec<Workspace>> {
        Ok(self.saved.borrow().clone())
    }

    fn save(&self, items: &[Workspace]) -> io::Result<()> {
        *self.saved.borrow_mut() = items.to_vec();
        Ok(())
    }
}

#[test]
fn add_assigns_new_ids_and_saves() {
    let store = Memory::default();
    let mut spaces = Workspaces::load(Box::new(store.clone())).unwrap();

    let a = spaces.add("  work ").unwrap();
    let b = spaces.add("home").unwrap();

    assert_ne!(a, b);
    assert_eq!(spaces.get(a).unwrap().name(), "work");
    assert_eq!(store.saved.borrow().len(), 2);
}

#[test]
fn add_rejects_blank_name() {
    let mut spaces = Workspaces::load(Box::new(Memory::default())).unwrap();

    assert!(spaces.add("   ").is_err());
    assert!(spaces.items().is_empty());
}

#[test]
fn ids_stay_unique_after_remove() {
    let mut spaces = Workspaces::load(Box::new(Memory::default())).unwrap();
    let a = spaces.add("a").unwrap();
    let b = spaces.add("b").unwrap();

    spaces.remove(b).unwrap();
    let c = spaces.add("c").unwrap();

    assert_ne!(c, a);
    assert_ne!(c, b);
    assert!(spaces.get(b).is_none());
}

#[test]
fn link_skips_duplicates_and_unlink_removes() {
    let store = Memory::default();
    let mut spaces = Workspaces::load(Box::new(store.clone())).unwrap();
    let id = spaces.add("w").unwrap();
    let project = Path::new("/p");

    spaces.link(id, project).unwrap();
    spaces.link(id, project).unwrap();
    assert_eq!(spaces.get(id).unwrap().projects(), [PathBuf::from("/p")]);

    spaces.unlink(id, project).unwrap();
    assert!(store.saved.borrow()[0].projects().is_empty());
}

#[test]
fn reloads_what_was_saved() {
    let store = Memory::default();
    let mut spaces = Workspaces::load(Box::new(store.clone())).unwrap();
    let id = spaces.add("w").unwrap();
    spaces.link(id, Path::new("/p")).unwrap();

    let again = Workspaces::load(Box::new(store)).unwrap();

    assert_eq!(again.items(), spaces.items());
}

#[test]
fn arranging_saves_only_what_changed() {
    let store = Memory::default();
    let mut spaces = Workspaces::load(Box::new(store.clone())).unwrap();
    let id = spaces.add("w").unwrap();
    let mut home = Arrangement::default();
    home.nudges.insert("/p".to_owned(), (1., 2.));

    spaces.arrange(id, home.clone()).unwrap();
    store.saved.borrow_mut().clear();
    spaces.arrange(id, home.clone()).unwrap();

    assert!(store.saved.borrow().is_empty());
    assert_eq!(spaces.get(id).unwrap().home(), &home);
}

#[test]
fn owner_picks_deepest_project() {
    let projects = [PathBuf::from("/a"), PathBuf::from("/a/b")];

    assert_eq!(
        owner(Path::new("/a/b/c.rs"), &projects),
        Some(Path::new("/a/b"))
    );
    assert_eq!(
        owner(Path::new("/a/c.rs"), &projects),
        Some(Path::new("/a"))
    );
    assert_eq!(owner(Path::new("/ab/c.rs"), &projects), None);
}
