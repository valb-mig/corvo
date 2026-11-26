use std::collections::HashMap;

use crate::{Node, language};

// What an open folder shows on the home, in blocks from top to bottom: its folders, then its
// files changed in git, then the rest. A crowded folder piles the rest by kind, one pile for each
// language or extension, the biggest first; a kind with too few files stays loose, and so does a
// file with no extension. Spread out, the rest shows whole again.

// A folder holding more than this many cards is crowded.
pub const CROWD: usize = 10;
// Fewer files of a kind than this don't make a pile.
pub const FEW: usize = 3;

// Files of one kind piled together, in the order the folder lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pile {
    kind: String,
    files: Vec<Node>,
}

impl Pile {
    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn files(&self) -> &[Node] {
        &self.files
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Node(Node),
    Pile(Pile),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    // No block is empty.
    pub blocks: Vec<Vec<Item>>,
    // Spread out, how many files would go back into piles.
    pub spread: Option<usize>,
}

// The column of a folder from what it holds, sorted as the folder lists them.
pub fn column(children: &[Node], changed: impl Fn(&Node) -> bool, spread: bool) -> Column {
    let folders: Vec<Item> = children
        .iter()
        .filter(|node| node.is_dir())
        .cloned()
        .map(Item::Node)
        .collect();
    let (changed, rest): (Vec<&Node>, Vec<&Node>) = children
        .iter()
        .filter(|node| !node.is_dir())
        .partition(|node| changed(node));
    let changed = changed.into_iter().cloned().map(Item::Node).collect();
    let (loose, piles) = if children.len() > CROWD {
        pile(&rest)
    } else {
        (
            rest.iter().map(|node| (*node).clone()).collect(),
            Vec::new(),
        )
    };
    let piled = piles.iter().map(|pile| pile.files.len()).sum::<usize>();
    let (rest, spread) = match (piled, spread) {
        (0, _) => (loose.into_iter().map(Item::Node).collect(), None),
        (_, true) => (
            rest.into_iter().cloned().map(Item::Node).collect(),
            Some(piled),
        ),
        (_, false) => (
            loose
                .into_iter()
                .map(Item::Node)
                .chain(piles.into_iter().map(Item::Pile))
                .collect(),
            None,
        ),
    };
    let blocks = [folders, changed, rest]
        .into_iter()
        .filter(|block: &Vec<Item>| !block.is_empty())
        .collect();
    Column { blocks, spread }
}

// The files that stay loose, and the piles of the rest, the biggest first; a tie goes by kind.
fn pile(files: &[&Node]) -> (Vec<Node>, Vec<Pile>) {
    let kinds: Vec<Option<String>> = files.iter().map(|file| kind(file.name())).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for kind in kinds.iter().flatten() {
        *counts.entry(kind).or_default() += 1;
    }
    let mut loose = Vec::new();
    let mut piles: Vec<Pile> = Vec::new();
    for (file, kind) in files.iter().zip(&kinds) {
        let file = (*file).clone();
        match kind.as_deref().filter(|kind| counts[kind] >= FEW) {
            None => loose.push(file),
            Some(kind) => match piles.iter_mut().find(|pile| pile.kind == kind) {
                Some(pile) => pile.files.push(file),
                None => piles.push(Pile {
                    kind: kind.to_owned(),
                    files: vec![file],
                }),
            },
        }
    }
    piles.sort_by(|a, b| {
        b.files
            .len()
            .cmp(&a.files.len())
            .then_with(|| a.kind.cmp(&b.kind))
    });
    (loose, piles)
}

// The language a file is written in, or else its extension; a name with no extension, or a
// dotfile, has no kind.
fn kind(name: &str) -> Option<String> {
    if let Some(language) = language(name) {
        return Some(language.to_owned());
    }
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_ascii_uppercase())
}
