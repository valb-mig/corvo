use std::path::Path;

use corvo_domain::{CROWD, Column, Item, Node, column};

// What a folder holds, sorted as the folder lists them.
fn folder(names: &[&str]) -> Vec<Node> {
    let mut nodes: Vec<Node> = names
        .iter()
        .map(|name| match name.strip_suffix('/') {
            Some(dir) => Node::dir(Path::new("/p").join(dir)),
            None => Node::file(Path::new("/p").join(name)),
        })
        .collect();
    nodes.sort();
    nodes
}

fn changed(names: &'static [&'static str]) -> impl Fn(&Node) -> bool {
    move |node| names.contains(&node.name())
}

// Each block as names, a pile as its kind and count.
fn names(column: &Column) -> Vec<Vec<String>> {
    column
        .blocks
        .iter()
        .map(|block| {
            block
                .iter()
                .map(|item| match item {
                    Item::Node(node) => node.name().to_owned(),
                    Item::Pile(pile) => format!("[{} × {}]", pile.kind(), pile.files().len()),
                })
                .collect()
        })
        .collect()
}

// Rust sources, enough to crowd a folder.
fn sources(count: usize) -> Vec<String> {
    (0..count).map(|ix| format!("m{ix:02}.rs")).collect()
}

#[test]
fn folders_come_first_then_changed_files_then_the_rest() {
    let nodes = folder(&["lib.rs", "src/", "main.rs", "Cargo.toml", "docs/"]);

    let column = column(&nodes, changed(&["main.rs"]), false);

    assert_eq!(
        names(&column),
        [
            vec!["docs", "src"],
            vec!["main.rs"],
            vec!["Cargo.toml", "lib.rs"]
        ]
    );
    assert_eq!(column.spread, None);
}

#[test]
fn a_block_with_nothing_in_it_is_left_out() {
    let nodes = folder(&["a.rs", "b.rs"]);

    let column = column(&nodes, changed(&[]), false);

    assert_eq!(names(&column), [vec!["a.rs", "b.rs"]]);
}

#[test]
fn a_folder_that_is_not_crowded_piles_nothing() {
    let files = sources(CROWD);
    let nodes = folder(&files.iter().map(String::as_str).collect::<Vec<_>>());

    let column = column(&nodes, changed(&[]), false);

    assert_eq!(column.blocks[0].len(), CROWD);
    assert!(
        column.blocks[0]
            .iter()
            .all(|item| matches!(item, Item::Node(_)))
    );
}

#[test]
fn a_crowded_folder_piles_the_rest_by_kind_the_biggest_first() {
    let mut names_in = vec![
        "src/",
        "chat.rs",
        "a.svg",
        "b.svg",
        "c.svg",
        "x.css",
        "y.css",
        "z.css",
        "w.css",
        "app.ts",
        "view.tsx",
        "page.tsx",
        "notes.md",
        "justfile",
        ".gitignore",
    ];
    let files = sources(4);
    names_in.extend(files.iter().map(String::as_str));
    let nodes = folder(&names_in);

    let column = column(&nodes, changed(&["chat.rs"]), false);

    assert_eq!(
        names(&column),
        [
            vec!["src".to_owned()],
            vec!["chat.rs".to_owned()],
            vec![
                ".gitignore".to_owned(),
                "justfile".to_owned(),
                "notes.md".to_owned(),
                "[CSS × 4]".to_owned(),
                "[Rust × 4]".to_owned(),
                "[SVG × 3]".to_owned(),
                "[TypeScript × 3]".to_owned(),
            ],
        ]
    );
    assert_eq!(column.spread, None);
}

#[test]
fn spread_out_the_rest_shows_whole_and_says_how_many_would_pile() {
    let mut names_in = vec!["src/", "chat.rs", "notes.md"];
    let files = sources(CROWD);
    names_in.extend(files.iter().map(String::as_str));
    let nodes = folder(&names_in);

    let column = column(&nodes, changed(&["chat.rs"]), true);

    let rest = &names(&column)[2];
    assert_eq!(rest.len(), CROWD + 1);
    assert_eq!(rest[0], "m00.rs");
    assert_eq!(rest.last().unwrap(), "notes.md");
    assert_eq!(column.spread, Some(CROWD));
}

#[test]
fn a_crowded_folder_with_nothing_to_pile_has_nothing_to_spread() {
    let nodes = folder(&["src/", "a.rs", "b.rs", "c.md", "d.toml"]);

    let piled = column(&nodes, changed(&[]), false);
    let spread = column(&nodes, changed(&[]), true);

    assert_eq!(names(&piled)[1], ["a.rs", "b.rs", "c.md", "d.toml"]);
    assert_eq!(piled, spread);
    assert_eq!(spread.spread, None);
}

#[test]
fn one_file_more_than_crowd_makes_a_pile() {
    let files = sources(CROWD + 1);
    let nodes = folder(&files.iter().map(String::as_str).collect::<Vec<_>>());

    let column = column(&nodes, changed(&[]), false);

    assert_eq!(names(&column), [vec![format!("[Rust × {}]", CROWD + 1)]]);
}

#[test]
fn folders_do_not_crowd_a_folder() {
    let mut names_in: Vec<String> = (0..8).map(|ix| format!("d{ix}/")).collect();
    names_in.extend(sources(CROWD));
    let nodes = folder(&names_in.iter().map(String::as_str).collect::<Vec<_>>());

    let column = column(&nodes, changed(&[]), false);

    assert_eq!(names(&column)[1], sources(CROWD));
}

#[test]
fn changed_files_are_never_piled() {
    let files = sources(7);
    let nodes = folder(&files.iter().map(String::as_str).collect::<Vec<_>>());

    let column = column(&nodes, |node| node.name() < "m03", false);

    assert_eq!(
        names(&column),
        [vec!["m00.rs", "m01.rs", "m02.rs"], vec!["[Rust × 4]"]]
    );
}
