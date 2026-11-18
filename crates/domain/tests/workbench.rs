use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::Fs;
use corvo_domain::{Content, Node, Workbench};

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

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .borrow()
            .get(path)
            .map(|text| text.clone().into_bytes())
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
    fs.dirs
        .borrow_mut()
        .insert("/q".into(), vec![Node::file("/q/index.ts")]);
    fs.files
        .borrow_mut()
        .insert("/p/main.rs".into(), "fn main() {}".into());
    fs
}

fn bench(fs: Memory) -> Workbench {
    let mut bench = Workbench::new(Box::new(fs));
    bench.show(Some(Path::new("/p"))).unwrap();
    bench
}

#[test]
fn loads_root_sorted() {
    let bench = bench(project());
    let root = bench.tree().unwrap().children(Path::new("/p")).unwrap();

    let names: Vec<_> = root.iter().map(Node::name).collect();
    assert_eq!(names, ["src", "main.rs"]);
}

#[test]
fn expands_lazily_once() {
    let fs = project();
    let mut bench = bench(fs.clone());
    let src = Path::new("/p/src");
    assert!(!bench.tree().unwrap().is_loaded(src));

    bench.expand(src).unwrap();
    bench.collapse(src);
    bench.expand(src).unwrap();

    assert!(bench.tree().unwrap().is_open(src));
    assert_eq!(*fs.lists.borrow(), 2);
}

#[test]
fn reveal_expands_ancestors() {
    let mut bench = bench(project());
    let src = Path::new("/p/src");

    bench.reveal(Path::new("/p/src/lib.rs")).unwrap();

    assert!(bench.tree().unwrap().is_loaded(src));
    assert!(bench.tree().unwrap().is_open(src));
}

#[test]
fn reveal_ignores_paths_outside_root() {
    let fs = project();
    let mut bench = bench(fs.clone());

    bench.reveal(Path::new("/usr/lib/rust/core.rs")).unwrap();

    assert_eq!(*fs.lists.borrow(), 1);
}

#[test]
fn switching_projects_keeps_each_tree() {
    let fs = project();
    let mut bench = bench(fs.clone());
    let src = Path::new("/p/src");
    bench.expand(src).unwrap();

    bench.show(Some(Path::new("/q"))).unwrap();
    assert_eq!(bench.tree().unwrap().root(), Path::new("/q"));
    bench.show(Some(Path::new("/p"))).unwrap();

    assert!(bench.tree().unwrap().is_open(src));
    assert_eq!(*fs.lists.borrow(), 3);
}

#[test]
fn tabs_survive_project_switch() {
    let mut bench = bench(project());
    bench.open(Path::new("/p/main.rs")).unwrap();

    bench.show(Some(Path::new("/q"))).unwrap();

    assert_eq!(bench.tabs().docs().len(), 1);
}

#[test]
fn forgetting_active_project_leaves_no_tree() {
    let mut bench = bench(project());

    bench.forget(Path::new("/p"));

    assert!(bench.project().is_none());
    assert!(bench.tree().is_none());
}

#[test]
fn open_returns_text_only_once() {
    let mut bench = bench(project());
    let main = Path::new("/p/main.rs");

    assert_eq!(
        bench.open(main).unwrap(),
        Some(Content::Text("fn main() {}".into()))
    );
    assert_eq!(bench.open(main).unwrap(), None);
    assert_eq!(bench.tabs().docs().len(), 1);
}

#[test]
fn images_open_without_reading() {
    let mut bench = bench(project());

    let opened = bench.open(Path::new("/p/logo.PNG")).unwrap();

    assert_eq!(opened, Some(Content::Image));
    assert_eq!(bench.tabs().docs().len(), 1);
}

#[test]
fn save_writes_and_cleans() {
    let fs = project();
    let mut bench = bench(fs.clone());
    let main = Path::new("/p/main.rs");
    bench.open(main).unwrap();
    bench.touch(main);

    bench.save(main, "fn main() { todo!() }").unwrap();

    assert_eq!(fs.files.borrow()[main], "fn main() { todo!() }");
    assert!(!bench.tabs().active().unwrap().is_dirty());
}
