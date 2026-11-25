use std::path::{Path, PathBuf};

use corvo_domain::server::Registry;
use corvo_domain::{Atlas, Node, Place, Symbol, SymbolKind, stack};

fn atlas() -> Atlas {
    let mut atlas = Atlas::default();
    atlas.chart(&[PathBuf::from("/p"), PathBuf::from("/q")]);
    atlas.isolate(Some(Path::new("/p")));
    atlas.trace(
        Path::new("/p/src/lib.rs"),
        vec![
            Symbol::new("Blobs", SymbolKind::Struct, 0, 11).children(vec![
                Symbol::new("put", SymbolKind::Method, 2, 7),
                Symbol::new("get", SymbolKind::Method, 6, 7),
            ]),
            Symbol::new("open", SymbolKind::Function, 12, 3),
        ],
    );
    atlas
}

#[test]
fn many_folders_and_files_stay_open_at_once() {
    let mut atlas = atlas();

    atlas.open(Node::dir("/p/src"));
    atlas.open(Node::dir("/p/docs"));
    atlas.open(Node::file("/p/src/lib.rs"));

    assert!(atlas.is_open(Path::new("/p/src")));
    assert!(atlas.is_open(Path::new("/p/docs")));
    assert!(atlas.is_open(Path::new("/p/src/lib.rs")));
}

#[test]
fn closing_a_folder_closes_what_is_open_inside_it() {
    let mut atlas = atlas();
    let lib = Path::new("/p/src/lib.rs");
    atlas.open(Node::dir("/p/src"));
    atlas.open(Node::dir("/p/srcs"));
    atlas.open(Node::file("/p/src/lib.rs"));
    atlas.dive(lib, &[0]);

    atlas.close(Path::new("/p/src"));

    assert!(!atlas.is_open(lib));
    assert!(!atlas.is_dived(lib, &[0]));
    assert!(atlas.is_open(Path::new("/p/srcs")));
}

#[test]
fn surfacing_a_symbol_closes_the_symbols_open_inside_it() {
    let mut atlas = atlas();
    let lib = Path::new("/p/src/lib.rs");
    atlas.dive(lib, &[0]);
    atlas.dive(lib, &[0, 1]);
    atlas.dive(lib, &[1]);

    atlas.surface(lib, &[0]);

    assert!(!atlas.is_dived(lib, &[0]));
    assert!(!atlas.is_dived(lib, &[0, 1]));
    assert!(atlas.is_dived(lib, &[1]));
}

#[test]
fn symbols_at_follow_the_indexes_through_the_outline() {
    let atlas = atlas();
    let lib = Path::new("/p/src/lib.rs");

    let chain = atlas.symbols_at(lib, &[0, 1]).unwrap();
    let names: Vec<_> = chain.iter().map(|symbol| symbol.name()).collect();
    assert_eq!(names, ["Blobs", "get"]);
    assert!(atlas.symbols_at(lib, &[0, 5]).is_none());
}

#[test]
fn isolating_closes_everything() {
    let mut atlas = atlas();
    atlas.open(Node::dir("/p/src"));
    atlas.dive(Path::new("/p/src/lib.rs"), &[0]);

    atlas.isolate(None);

    assert!(atlas.root().is_none());
    assert!(!atlas.is_open(Path::new("/p/src")));
    assert_eq!(atlas.dives().count(), 0);
}

#[test]
fn unlinking_the_isolated_project_goes_back_to_all() {
    let mut atlas = atlas();

    atlas.chart(&[PathBuf::from("/p"), PathBuf::from("/r")]);
    assert_eq!(atlas.root(), Some(Path::new("/p")));

    atlas.chart(&[PathBuf::from("/q")]);
    assert!(atlas.root().is_none());
}

#[test]
fn stack_comes_from_root_markers_in_registry_order() {
    let registry = Registry::default();
    let laravel = [
        Node::file("/w/package.json"),
        Node::file("/w/composer.json"),
    ];
    let node = [Node::file("/n/package.json")];
    let marker_dir = [Node::dir("/d/Cargo.toml")];

    assert_eq!(stack(&laravel, &registry), Some("PHP"));
    assert_eq!(stack(&node, &registry), Some("Node"));
    assert_eq!(stack(&marker_dir, &registry), None);
}

#[test]
fn hiding_closes_the_card_and_keeps_it_hidden_across_isolation() {
    let mut atlas = atlas();
    let lib = Path::new("/p/src/lib.rs");
    atlas.open(Node::dir("/p/src"));
    atlas.open(Node::file("/p/src/lib.rs"));
    atlas.dive(lib, &[0]);
    atlas.dive(lib, &[0, 1]);

    atlas.hide(Place::Symbol(lib.to_path_buf(), vec![0]), "Blobs");
    assert!(!atlas.is_dived(lib, &[0, 1]));
    atlas.hide(Place::Node(PathBuf::from("/p/src")), "src");
    assert!(!atlas.is_open(lib));

    atlas.isolate(None);
    atlas.isolate(Some(Path::new("/p")));

    let names: Vec<_> = atlas.hidden().map(|(_, name)| name).collect();
    assert_eq!(names, ["src", "Blobs"]);
    assert!(atlas.is_hidden(&Place::Node(PathBuf::from("/p/src"))));
}

#[test]
fn showing_brings_one_card_or_all_back() {
    let mut atlas = atlas();
    let src = Place::Node(PathBuf::from("/p/src"));
    let docs = Place::Node(PathBuf::from("/p/docs"));
    atlas.hide(src.clone(), "src");
    atlas.hide(docs.clone(), "docs");

    atlas.show(&src);
    assert!(!atlas.is_hidden(&src) && atlas.is_hidden(&docs));

    atlas.show_all();
    assert_eq!(atlas.hidden().count(), 0);
}
