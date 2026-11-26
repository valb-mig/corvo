use std::path::{Path, PathBuf};

use corvo_domain::server::Registry;
use corvo_domain::{Atlas, Mark, Node, Place, Symbol, SymbolKind, stack};

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
fn a_crowded_folder_spreads_out_until_piled_back_or_closed() {
    let mut atlas = atlas();
    let (src, deep) = (Path::new("/p/src"), Path::new("/p/src/deep"));
    atlas.open(Node::dir(src));
    atlas.open(Node::dir(deep));
    atlas.spread(src);
    atlas.spread(deep);

    atlas.pile(src);
    assert!(!atlas.is_spread(src));
    assert!(atlas.is_spread(deep));

    atlas.spread(src);
    atlas.close(src);
    assert!(!atlas.is_spread(src));
    assert!(!atlas.is_spread(deep));
}

#[test]
fn the_arrangement_keeps_the_folders_spread_out() {
    let mut atlas = atlas();
    atlas.spread(Path::new("/p/src"));

    assert_eq!(atlas.arrangement().spread, vec![PathBuf::from("/p/src")]);

    atlas.isolate(Some(Path::new("/p")));
    assert!(atlas.arrangement().spread.is_empty());
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

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_trail_names_the_way_to_a_symbol_and_counts_repeated_names() {
    let mut atlas = atlas();
    let twice = Path::new("/p/twice.rs");
    atlas.trace(
        twice,
        vec![
            Symbol::new("new", SymbolKind::Function, 0, 3),
            Symbol::new("new", SymbolKind::Function, 4, 3),
        ],
    );
    let lib = Path::new("/p/src/lib.rs");

    assert_eq!(atlas.trail(lib, &[0, 1]).unwrap(), ["Blobs", "get"]);
    assert_eq!(atlas.trail(twice, &[1]).unwrap(), ["new~1"]);
    assert_eq!(atlas.find(twice, &keys(&["new~1"])), Some(vec![1]));
    assert_eq!(atlas.find(lib, &keys(&["Blobs", "gone"])), None);
}

#[test]
fn grouping_takes_cards_out_of_the_groups_they_were_in() {
    let mut atlas = atlas();
    let first = atlas.group(keys(&["a", "b"])).unwrap();
    let second = atlas.group(keys(&["b", "c"])).unwrap();

    assert_ne!(first, second);
    assert_eq!(atlas.group_of("a").unwrap().members(), ["a"]);
    assert_eq!(atlas.group_of("b").unwrap().id(), second);
    assert_eq!(
        atlas.group_of("c").unwrap().name(),
        format!("Group {second}")
    );

    atlas.leave("a");
    assert!(atlas.group_of("a").is_none());
    assert_eq!(atlas.groups().len(), 1);
    assert!(atlas.group(Vec::new()).is_none());
}

#[test]
fn a_blank_name_leaves_the_group_as_it_was() {
    let mut atlas = atlas();
    let id = atlas.group(keys(&["a"])).unwrap();

    atlas.rename(id, "  Storage ");
    atlas.rename(id, "   ");

    assert_eq!(atlas.groups()[0].name(), "Storage");
}

#[test]
fn a_hidden_group_veils_its_cards_until_shown() {
    let mut atlas = atlas();
    let id = atlas.group(keys(&["a", "b"])).unwrap();
    atlas.hide(Place::Node(PathBuf::from("/p/docs")), "docs");

    atlas.hide_group(id);
    assert!(atlas.is_veiled("a") && !atlas.is_veiled("c"));
    assert_eq!(atlas.hidden_count(), 2);

    atlas.show_all();
    assert!(!atlas.is_veiled("a"));
    assert_eq!(atlas.hidden_count(), 0);
}

#[test]
fn untangling_keeps_the_groups() {
    let mut atlas = atlas();
    atlas.group(keys(&["a"]));
    atlas.nudge("a", (10., -4.));

    atlas.untangle();

    assert!(atlas.nudges().is_empty());
    assert_eq!(atlas.groups().len(), 1);
}

#[test]
fn the_arrangement_keeps_symbols_by_their_trail() {
    let mut atlas = atlas();
    let lib = Path::new("/p/src/lib.rs");
    atlas.open(Node::dir("/p/src"));
    atlas.open(Node::file(lib));
    atlas.dive(lib, &[0]);
    atlas.hide(Place::Symbol(lib.to_path_buf(), vec![1]), "open");
    atlas.nudge("/p/src", (5., 5.));

    let kept = atlas.arrangement();

    assert_eq!(kept.root.as_deref(), Some(Path::new("/p")));
    assert_eq!(kept.open.len(), 2);
    assert_eq!(kept.dives, [(lib.to_path_buf(), keys(&["Blobs"]))]);
    assert_eq!(
        kept.hidden,
        [(
            Mark::Symbol(lib.to_path_buf(), keys(&["open"])),
            "open".to_owned()
        )]
    );
    assert_eq!(kept.nudges["/p/src"], (5., 5.));
}
