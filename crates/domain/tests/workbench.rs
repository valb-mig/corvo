use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::{Depth, Fs, Syntax};
use corvo_domain::{
    Arrangement, Content, Lang, Mark, Moved, Node, Place, Symbol, SymbolKind, Workbench,
};

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

struct Lines;

impl Syntax for Lines {
    fn outlines(&self, lang: Lang) -> bool {
        lang == Lang::Rust
    }

    fn outline(&self, _: Lang, text: &str, _: Depth) -> Vec<Symbol> {
        text.lines()
            .enumerate()
            .filter_map(|(ix, line)| {
                let name = line.strip_prefix("fn ")?.split('(').next()?;
                Some(Symbol::new(name, SymbolKind::Function, ix as u32, 3))
            })
            .collect()
    }

    fn mentions(&self, _: Lang, text: &str, name: &str, within: Range<usize>) -> Vec<Range<usize>> {
        text[within.clone()]
            .match_indices(name)
            .map(|(ix, _)| within.start + ix..within.start + ix + name.len())
            .collect()
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
    let mut bench = Workbench::new(Box::new(fs), Box::new(Lines));
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

#[test]
fn outline_reads_file_through_syntax() {
    let bench = bench(project());

    let symbols = bench
        .outline(Path::new("/p/main.rs"), Depth::Items)
        .unwrap();

    assert_eq!(symbols, [Symbol::new("main", SymbolKind::Function, 0, 3)]);
}

#[test]
fn outline_skips_languages_without_syntax() {
    let bench = bench(project());
    let path = Path::new("/q/index.ts");

    assert!(!bench.outlines(path));
    assert!(bench.outline(path, Depth::Items).unwrap().is_empty());
}

#[test]
fn chart_lists_projects_and_isolating_surveys_once() {
    let fs = project();
    let mut bench = bench(fs.clone());
    let before = *fs.lists.borrow();

    bench
        .chart(&[PathBuf::from("/p"), PathBuf::from("/q")])
        .unwrap();
    bench.isolate(Some(Path::new("/p"))).unwrap();
    bench.isolate(Some(Path::new("/p"))).unwrap();

    let atlas = bench.atlas();
    assert_eq!(atlas.root(), Some(Path::new("/p")));
    assert!(atlas.is_loaded(Path::new("/q")));
    assert!(atlas.is_loaded(Path::new("/p/src")));
    assert_eq!(*fs.lists.borrow() - before, 3);
}

#[test]
fn toggling_opens_and_closes_with_what_is_inside() {
    let mut bench = bench(project());
    bench.chart(&[PathBuf::from("/p")]).unwrap();
    bench.isolate(Some(Path::new("/p"))).unwrap();
    let (src, main) = (Node::dir("/p/src"), Node::file("/p/main.rs"));

    bench.toggle(&src).unwrap();
    bench.toggle(&main).unwrap();
    bench.toggle_symbol(main.path(), &[0]);

    let atlas = bench.atlas();
    assert!(atlas.is_open(src.path()) && atlas.is_open(main.path()));
    assert!(atlas.is_dived(main.path(), &[0]));

    bench.toggle(&main).unwrap();
    let atlas = bench.atlas();
    assert!(atlas.is_open(src.path()));
    assert!(!atlas.is_open(main.path()));
    assert!(!atlas.is_dived(main.path(), &[0]));
}

#[test]
fn rechart_reads_open_folders_and_files_again() {
    let fs = project();
    let mut bench = bench(fs.clone());
    bench.chart(&[PathBuf::from("/p")]).unwrap();
    bench.isolate(Some(Path::new("/p"))).unwrap();
    bench.toggle(&Node::file("/p/main.rs")).unwrap();
    assert_eq!(
        bench.atlas().outline(Path::new("/p/main.rs")).unwrap(),
        [Symbol::new("main", SymbolKind::Function, 0, 3)]
    );
    fs.dirs
        .borrow_mut()
        .get_mut(Path::new("/p"))
        .unwrap()
        .push(Node::file("/p/new.rs"));
    fs.files
        .borrow_mut()
        .insert("/p/main.rs".into(), "fn main() {}\nfn run() {}".into());

    bench.rechart().unwrap();

    let atlas = bench.atlas();
    assert_eq!(atlas.children(Path::new("/p")).unwrap().len(), 3);
    assert_eq!(atlas.outline(Path::new("/p/main.rs")).unwrap().len(), 2);
}

#[test]
fn a_file_that_cannot_be_read_stays_closed() {
    let mut bench = bench(project());
    bench.chart(&[PathBuf::from("/p")]).unwrap();
    bench.isolate(Some(Path::new("/p"))).unwrap();
    let gone = Node::file("/p/gone.rs");

    assert!(bench.toggle(&gone).is_err());
    assert!(!bench.atlas().is_open(gone.path()));
}

fn script() -> Memory {
    let fs = project();
    fs.files.borrow_mut().insert(
        "/p/main.rs".into(),
        "fn main() {\n    let n = 1;\n    go(n);\n}\nfn go(n: u8) {}\n".into(),
    );
    fs
}

#[test]
fn excerpt_reads_the_lines_of_a_symbol() {
    let bench = bench(script());

    let text = bench.excerpt(Path::new("/p/main.rs"), 1, 2).unwrap();

    assert_eq!(text, "    let n = 1;\n    go(n);");
    assert!(bench.excerpt(Path::new("/p/main.rs"), 3, 40).is_err());
}

#[test]
fn splice_writes_over_the_lines_it_read() {
    let fs = script();
    let mut bench = bench(fs.clone());
    let main = Path::new("/p/main.rs");

    let new = bench
        .splice(main, 1, 2, "    let n = 1;\n    go(n);", "    go(2);")
        .unwrap();

    assert_eq!(new, "fn main() {\n    go(2);\n}\nfn go(n: u8) {}\n");
    assert_eq!(fs.files.borrow()[main], new);
}

#[test]
fn splice_refuses_lines_that_changed() {
    let fs = script();
    let mut bench = bench(fs.clone());
    let main = Path::new("/p/main.rs");

    assert!(bench.splice(main, 1, 1, "    let m = 1;", "x").is_err());
    assert!(fs.files.borrow()[main].contains("let n = 1;"));
}

#[test]
fn mentions_stay_inside_the_given_lines() {
    let bench = bench(script());
    let main = Path::new("/p/main.rs");

    let inside = bench.mentions(main, "n", Some((0, 3))).unwrap();
    let lines: Vec<_> = inside.iter().map(|mention| mention.line()).collect();
    assert_eq!(lines, [0, 1, 2]);

    let all = bench.mentions(main, "n", None).unwrap();
    assert_eq!(all.len(), 4);
}

#[test]
fn restore_brings_the_home_back_after_lines_move() {
    let fs = project();
    fs.files
        .borrow_mut()
        .insert("/p/src/lib.rs".into(), "fn open()\nfn close()".into());
    let lib = PathBuf::from("/p/src/lib.rs");
    let left = Arrangement {
        root: Some(PathBuf::from("/p")),
        open: vec![Node::dir("/p/src"), Node::file(&lib)],
        dives: vec![(lib.clone(), vec!["close".to_owned()])],
        spread: vec![PathBuf::from("/p/src"), PathBuf::from("/q/src")],
        hidden: vec![(
            Mark::Symbol(lib.clone(), vec!["open".to_owned()]),
            "open".to_owned(),
        )],
        ..Arrangement::default()
    };
    // A line came in above both functions since the home was left.
    fs.files
        .borrow_mut()
        .insert(lib.clone(), "fn close()\n\nfn open()".into());
    let mut bench = Workbench::new(Box::new(fs), Box::new(Lines));

    bench.restore(&[PathBuf::from("/p")], &left);

    let atlas = bench.atlas();
    assert_eq!(atlas.root(), Some(Path::new("/p")));
    assert!(atlas.is_open(&lib));
    assert!(atlas.is_dived(&lib, &[0]));
    assert!(atlas.is_spread(Path::new("/p/src")));
    assert!(!atlas.is_spread(Path::new("/q/src")));
    assert!(atlas.is_hidden(&Place::Symbol(lib.clone(), vec![1])));
}

#[test]
fn restore_leaves_out_a_project_no_longer_linked() {
    let left = Arrangement {
        root: Some(PathBuf::from("/gone")),
        open: vec![Node::dir("/gone/src")],
        ..Arrangement::default()
    };
    let mut bench = Workbench::new(Box::new(project()), Box::new(Lines));

    bench.restore(&[PathBuf::from("/p")], &left);

    assert!(bench.atlas().root().is_none());
    assert_eq!(bench.atlas().opened().count(), 0);
}

// The home with /p open alone, and /p/src/deep, which holds nothing yet, closed.
fn isolated(fs: &Memory) -> Workbench {
    fs.dirs
        .borrow_mut()
        .get_mut(Path::new("/p/src"))
        .unwrap()
        .push(Node::dir("/p/src/deep"));
    let mut bench = bench(fs.clone());
    bench.chart(&[PathBuf::from("/p")]).unwrap();
    bench.isolate(Some(Path::new("/p"))).unwrap();
    bench
}

#[test]
fn a_file_that_came_opens_the_folders_down_to_it() {
    let fs = project();
    let mut bench = isolated(&fs);
    fs.dirs
        .borrow_mut()
        .insert("/p/src/deep".into(), vec![Node::file("/p/src/deep/new.rs")]);

    assert!(bench.uncover(&[moved("/p/src/deep/new.rs", true)]));

    let atlas = bench.atlas();
    assert!(atlas.is_open(Path::new("/p/src")));
    assert!(atlas.is_open(Path::new("/p/src/deep")));
    let deep: Vec<_> = atlas.children(Path::new("/p/src/deep")).unwrap().to_vec();
    assert_eq!(deep, [Node::file("/p/src/deep/new.rs")]);
}

#[test]
fn a_file_that_went_leaves_its_folders_as_they_were() {
    let fs = project();
    let mut bench = isolated(&fs);
    fs.dirs
        .borrow_mut()
        .get_mut(Path::new("/p"))
        .unwrap()
        .retain(|node| node.path() != Path::new("/p/main.rs"));

    assert!(!bench.uncover(&[moved("/p/main.rs", false)]));

    let atlas = bench.atlas();
    assert!(!atlas.is_open(Path::new("/p/src")));
    let root: Vec<_> = atlas.children(Path::new("/p")).unwrap().to_vec();
    assert_eq!(root, [Node::dir("/p/src")]);
}

#[test]
fn outside_the_project_open_the_home_stays_as_it_is() {
    let fs = project();
    let mut bench = bench(fs.clone());
    bench
        .chart(&[PathBuf::from("/p"), PathBuf::from("/q")])
        .unwrap();
    bench.isolate(Some(Path::new("/q"))).unwrap();

    assert!(!bench.uncover(&[moved("/p/src/lib.rs", true)]));
    assert!(!bench.atlas().is_open(Path::new("/p/src")));
}

#[test]
fn the_file_tree_reads_its_open_folders_again() {
    let fs = project();
    let mut bench = bench(fs.clone());
    fs.dirs
        .borrow_mut()
        .get_mut(Path::new("/p"))
        .unwrap()
        .push(Node::file("/p/new.rs"));

    bench.uncover(&[moved("/p/new.rs", true)]);

    let names: Vec<_> = bench
        .tree()
        .unwrap()
        .children(Path::new("/p"))
        .unwrap()
        .iter()
        .map(Node::name)
        .collect();
    assert_eq!(names, ["src", "main.rs", "new.rs"]);
}

fn moved(path: &str, shown: bool) -> Moved {
    Moved {
        path: PathBuf::from(path),
        shown,
    }
}
