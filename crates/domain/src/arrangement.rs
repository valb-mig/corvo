use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::Node;

// How the home of a workspace was left, to come back to it the same: the project standing alone,
// the folders, files and symbols open, the crowded folders spread out, the cards hidden, where
// cards were moved by hand, and the groups made of them. A symbol is kept by its trail, the names that lead to it through the
// outline, so it is found again after its lines move.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
    pub root: Option<PathBuf>,
    pub open: Vec<Node>,
    pub dives: Vec<(PathBuf, Vec<String>)>,
    pub spread: Vec<PathBuf>,
    pub hidden: Vec<(Mark, String)>,
    pub nudges: BTreeMap<String, (f32, f32)>,
    pub groups: Vec<Group>,
}

// A hidden card as it is kept: a folder or file by its path, a symbol by its file and trail.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mark {
    Node(PathBuf),
    Symbol(PathBuf, Vec<String>),
}

// Cards of the home put together under a name, each known by the key the home gives its card. A
// collapsed group shows as one card; a hidden one is off the home with all its cards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    id: u64,
    name: String,
    members: Vec<String>,
    collapsed: bool,
    hidden: bool,
}

impl Group {
    pub fn new(id: u64, name: impl Into<String>, members: Vec<String>) -> Self {
        Self {
            id,
            name: name.into(),
            members,
            collapsed: false,
            hidden: false,
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn members(&self) -> &[String] {
        &self.members
    }

    pub fn holds(&self, key: &str) -> bool {
        self.members.iter().any(|member| member == key)
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub(crate) fn rename(&mut self, name: String) {
        self.name = name;
    }

    pub(crate) fn fold(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    pub(crate) fn veil(&mut self, hidden: bool) {
        self.hidden = hidden;
    }

    pub(crate) fn drop_member(&mut self, key: &str) {
        self.members.retain(|member| member != key);
    }
}
