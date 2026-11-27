use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::{Saved, Store};
use corvo_domain::{Arrangement, Uuid, Workspace, Workspaces, owner};

#[derive(Clone, Default)]
struct Memory {
    saved: Rc<RefCell<Vec<Workspace>>>,
    last: Rc<RefCell<Option<Uuid>>>,
}

impl Store for Memory {
    fn load(&self) -> io::Result<Saved> {
        Ok(Saved {
            items: self.saved.borrow().clone(),
            last: *self.last.borrow(),
        })
    }

    fn save(&self, items: &[Workspace], last: Option<Uuid>) -> io::Result<()> {
        *self.saved.borrow_mut() = items.to_vec();
        *self.last.borrow_mut() = last;
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

#[test]
fn rename_trims_and_saves() {
    let store = Memory::default();
    let mut spaces = Workspaces::load(Box::new(store.clone())).unwrap();
    let id = spaces.add("w").unwrap();

    spaces.rename(id, "  work ").unwrap();

    assert_eq!(spaces.get(id).unwrap().name(), "work");
    assert_eq!(store.saved.borrow()[0].name(), "work");
}

#[test]
fn rename_rejects_blank_name() {
    let mut spaces = Workspaces::load(Box::new(Memory::default())).unwrap();
    let id = spaces.add("w").unwrap();

    assert!(spaces.rename(id, "  ").is_err());
    assert_eq!(spaces.get(id).unwrap().name(), "w");
}

#[test]
fn picked_is_kept_and_comes_back() {
    let store = Memory::default();
    let mut spaces = Workspaces::load(Box::new(store.clone())).unwrap();
    let a = spaces.add("a").unwrap();
    let b = spaces.add("b").unwrap();

    spaces.pick(b).unwrap();
    let again = Workspaces::load(Box::new(store.clone())).unwrap();
    assert_eq!(again.last(), Some(b));

    spaces.remove(b).unwrap();
    assert_eq!(spaces.last(), None);
    assert_eq!(*store.last.borrow(), None);

    spaces.pick(Uuid::from_u128(7)).unwrap();
    assert_eq!(spaces.last(), None);
    spaces.pick(a).unwrap();
    assert_eq!(spaces.last(), Some(a));
}

#[test]
fn relocate_keeps_the_place_and_the_isolated_root() {
    let mut spaces = Workspaces::load(Box::new(Memory::default())).unwrap();
    let id = spaces.add("w").unwrap();
    spaces.link(id, Path::new("/a")).unwrap();
    spaces.link(id, Path::new("/b")).unwrap();
    spaces.focus(id, Some(Path::new("/a"))).unwrap();

    spaces
        .relocate(id, Path::new("/a"), Path::new("/moved/a"))
        .unwrap();

    let space = spaces.get(id).unwrap();
    assert_eq!(
        space.projects(),
        [PathBuf::from("/moved/a"), PathBuf::from("/b")]
    );
    assert_eq!(space.home().root, Some(PathBuf::from("/moved/a")));
}

#[test]
fn relocate_onto_a_linked_project_drops_the_old_one() {
    let mut spaces = Workspaces::load(Box::new(Memory::default())).unwrap();
    let id = spaces.add("w").unwrap();
    spaces.link(id, Path::new("/a")).unwrap();
    spaces.link(id, Path::new("/b")).unwrap();

    spaces
        .relocate(id, Path::new("/a"), Path::new("/b"))
        .unwrap();

    assert_eq!(spaces.get(id).unwrap().projects(), [PathBuf::from("/b")]);
}

#[test]
fn focus_isolates_only_a_linked_project() {
    let mut spaces = Workspaces::load(Box::new(Memory::default())).unwrap();
    let id = spaces.add("w").unwrap();
    spaces.link(id, Path::new("/a")).unwrap();

    spaces.focus(id, Some(Path::new("/a"))).unwrap();
    assert_eq!(
        spaces.get(id).unwrap().home().root,
        Some(PathBuf::from("/a"))
    );

    spaces.focus(id, Some(Path::new("/elsewhere"))).unwrap();
    assert_eq!(spaces.get(id).unwrap().home().root, None);
}
