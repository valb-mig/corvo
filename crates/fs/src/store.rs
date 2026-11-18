use std::io;
use std::path::PathBuf;

use corvo_domain::Workspace;
use corvo_domain::port::Store;
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
}

pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn config() -> io::Result<Self> {
        let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
            Some(dir) => PathBuf::from(dir),
            None => std::env::home_dir()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory"))?
                .join(".config"),
        };
        Ok(Self::new(base.join("corvo").join("workspaces.json")))
    }
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
            .map(|entry| Workspace::new(entry.id, entry.name, entry.projects))
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
