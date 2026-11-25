use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use corvo_domain::port::Store;
use corvo_domain::{Arrangement, Group, Mark, Node, Workspace};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
struct File {
    workspaces: Vec<Entry>,
}

#[derive(Deserialize, Serialize)]
struct Entry {
    id: u64,
    name: String,
    projects: Vec<PathBuf>,
    #[serde(default)]
    home: Home,
}

// How the home was left; a file from before it was kept loads as an empty one.
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct Home {
    #[serde(skip_serializing_if = "Option::is_none")]
    root: Option<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    open: Vec<Opened>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    dives: Vec<Dived>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hidden: Vec<Hidden>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    nudges: BTreeMap<String, [f32; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    groups: Vec<Grouped>,
}

#[derive(Deserialize, Serialize)]
struct Opened {
    path: PathBuf,
    #[serde(default)]
    dir: bool,
}

#[derive(Deserialize, Serialize)]
struct Dived {
    file: PathBuf,
    trail: Vec<String>,
}

// A folder or file by its path; a symbol by its file and trail.
#[derive(Deserialize, Serialize)]
struct Hidden {
    path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trail: Option<Vec<String>>,
    name: String,
}

#[derive(Deserialize, Serialize)]
struct Grouped {
    id: u64,
    name: String,
    members: Vec<String>,
    #[serde(default)]
    collapsed: bool,
    #[serde(default)]
    hidden: bool,
}

impl From<Home> for Arrangement {
    fn from(home: Home) -> Self {
        Self {
            root: home.root,
            open: home
                .open
                .into_iter()
                .map(|opened| {
                    if opened.dir {
                        Node::dir(opened.path)
                    } else {
                        Node::file(opened.path)
                    }
                })
                .collect(),
            dives: home
                .dives
                .into_iter()
                .map(|dived| (dived.file, dived.trail))
                .collect(),
            hidden: home
                .hidden
                .into_iter()
                .map(|hidden| {
                    let mark = match hidden.trail {
                        Some(trail) => Mark::Symbol(hidden.path, trail),
                        None => Mark::Node(hidden.path),
                    };
                    (mark, hidden.name)
                })
                .collect(),
            nudges: home
                .nudges
                .into_iter()
                .map(|(key, [x, y])| (key, (x, y)))
                .collect(),
            groups: home
                .groups
                .into_iter()
                .map(|group| {
                    Group::new(group.id, group.name, group.members)
                        .collapsed(group.collapsed)
                        .hidden(group.hidden)
                })
                .collect(),
        }
    }
}

impl From<&Arrangement> for Home {
    fn from(home: &Arrangement) -> Self {
        Self {
            root: home.root.clone(),
            open: home
                .open
                .iter()
                .map(|node| Opened {
                    path: node.path().to_path_buf(),
                    dir: node.is_dir(),
                })
                .collect(),
            dives: home
                .dives
                .iter()
                .map(|(file, trail)| Dived {
                    file: file.clone(),
                    trail: trail.clone(),
                })
                .collect(),
            hidden: home
                .hidden
                .iter()
                .map(|(mark, name)| {
                    let (path, trail) = match mark {
                        Mark::Node(path) => (path.clone(), None),
                        Mark::Symbol(file, trail) => (file.clone(), Some(trail.clone())),
                    };
                    Hidden {
                        path,
                        trail,
                        name: name.clone(),
                    }
                })
                .collect(),
            nudges: home
                .nudges
                .iter()
                .map(|(key, (x, y))| (key.clone(), [*x, *y]))
                .collect(),
            groups: home
                .groups
                .iter()
                .map(|group| Grouped {
                    id: group.id(),
                    name: group.name().to_owned(),
                    members: group.members().to_vec(),
                    collapsed: group.is_collapsed(),
                    hidden: group.is_hidden(),
                })
                .collect(),
        }
    }
}

pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn config() -> io::Result<Self> {
        Ok(Self::new(config_dir()?.join("workspaces.json")))
    }
}

// Where corvo keeps what it saves: `$XDG_CONFIG_HOME/corvo`, or `~/.config/corvo`.
pub(crate) fn config_dir() -> io::Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => std::env::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory"))?
            .join(".config"),
    };
    Ok(base.join("corvo"))
}

impl Store for JsonStore {
    fn load(&self) -> io::Result<Vec<Workspace>> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let file: File = serde_json::from_str(&text).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {err}", self.path.display()),
            )
        })?;
        Ok(file
            .workspaces
            .into_iter()
            .map(|entry| {
                Workspace::new(entry.id, entry.name, entry.projects).arranged(entry.home.into())
            })
            .collect())
    }

    fn save(&self, items: &[Workspace]) -> io::Result<()> {
        let file = File {
            workspaces: items
                .iter()
                .map(|item| Entry {
                    id: item.id(),
                    name: item.name().to_owned(),
                    projects: item.projects().to_vec(),
                    home: item.home().into(),
                })
                .collect(),
        };
        let text = serde_json::to_string_pretty(&file).map_err(io::Error::other)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, &self.path)
    }
}
