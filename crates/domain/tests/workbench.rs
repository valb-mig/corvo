use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::Fs;
use corvo_domain::{Node, Workbench};

#[derive(Clone, Default)]
struct Memory {
    files: Rc<RefCell<HashMap<PathBuf, String>>>,
    dirs: Rc<RefCell<HashMap<PathBuf, Vec<Node>>>>,
    lists: Rc<RefCell<usize>>,
}

impl Fs for Memory {
    fn list(&self, dir: &Path) -> io::Result<Vec<Node>> {
        *self.lists.borrow_mut() += 1;
        Ok(self.dirs.borrow().get(dir).cloned().unwrap_or_default())
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, path: &Path, text: &str) -> io::Result<()> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), text.to_owned());
        Ok(())
    }
}

fn project() -> Memory {
    let fs = Memory::default();
    fs.dirs.borrow_mut().insert(
        "/p".into(),
        vec![Node::file("/p/main.rs"), Node::dir("/p/src")],
    );
    fs.dirs
        .borrow_mut()
        .insert("/p/src".into(), vec![Node::file("/p/src/lib.rs")]);
    fs.files
        .borrow_mut()
        .insert("/p/main.rs".into(), "fn main() {}".into());
    fs
}

#[test]
fn loads_root_sorted() {
    let bench = Workbench::new("/p", Box::new(project())).unwrap();
    let root = bench.tree().children(Path::new("/p")).unwrap();

    let names: Vec<_> = root.iter().map(Node::name).collect();
    assert_eq!(names, ["src", "main.rs"]);
}

#[test]
fn expands_lazily_once() {
    let fs = project();
    let mut bench = Workbench::new("/p", Box::new(fs.clone())).unwrap();
    let src = Path::new("/p/src");
    assert!(!bench.tree().is_loaded(src));

    bench.expand(src).unwrap();
    bench.collapse(src);
    bench.expand(src).unwrap();

    assert!(bench.tree().is_open(src));
    assert_eq!(*fs.lists.borrow(), 2);
}

#[test]
fn open_returns_text_only_once() {
    let mut bench = Workbench::new("/p", Box::new(project())).unwrap();
    let main = Path::new("/p/main.rs");

    assert_eq!(bench.open(main).unwrap().as_deref(), Some("fn main() {}"));
    assert_eq!(bench.open(main).unwrap(), None);
    assert_eq!(bench.tabs().docs().len(), 1);
}

#[test]
fn save_writes_and_cleans() {
    let fs = project();
    let mut bench = Workbench::new("/p", Box::new(fs.clone())).unwrap();
    let main = Path::new("/p/main.rs");
    bench.open(main).unwrap();
    bench.touch(main);

    bench.save(main, "fn main() { todo!() }").unwrap();

    assert_eq!(fs.files.borrow()[main], "fn main() { todo!() }");
    assert!(!bench.tabs().active().unwrap().is_dirty());
}
