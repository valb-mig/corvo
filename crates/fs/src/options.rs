use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::PathBuf;

use corvo_domain::port::OptionStore;
use corvo_domain::{Choice, Options, Pick};
use serde::{Deserialize, Serialize};

// What each agent offered last, by agent, in one file.
#[derive(Default, Deserialize, Serialize)]
struct File {
    agents: BTreeMap<String, Kept>,
}

#[derive(Deserialize, Serialize)]
struct Kept {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<KeptChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effort: Option<KeptChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<KeptChoice>,
}

#[derive(Deserialize, Serialize)]
struct KeptChoice {
    id: String,
    current: String,
    values: Vec<KeptPick>,
}

#[derive(Deserialize, Serialize)]
struct KeptPick {
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

impl From<&Options> for Kept {
    fn from(options: &Options) -> Self {
        Self {
            model: options.model.as_ref().map(KeptChoice::from),
            effort: options.effort.as_ref().map(KeptChoice::from),
            mode: options.mode.as_ref().map(KeptChoice::from),
        }
    }
}

impl From<Kept> for Options {
    fn from(kept: Kept) -> Self {
        Self {
            model: kept.model.map(Choice::from),
            effort: kept.effort.map(Choice::from),
            mode: kept.mode.map(Choice::from),
        }
    }
}

impl From<&Choice> for KeptChoice {
    fn from(choice: &Choice) -> Self {
        Self {
            id: choice.id.clone(),
            current: choice.current.clone(),
            values: choice
                .values
                .iter()
                .map(|pick| KeptPick {
                    id: pick.id.clone(),
                    name: pick.name.clone(),
                    description: pick.description.clone(),
                })
                .collect(),
        }
    }
}

impl From<KeptChoice> for Choice {
    fn from(kept: KeptChoice) -> Self {
        Self {
            id: kept.id,
            current: kept.current,
            values: kept
                .values
                .into_iter()
                .map(|pick| Pick {
                    id: pick.id,
                    name: pick.name,
                    description: pick.description,
                })
                .collect(),
        }
    }
}

pub struct JsonOptions {
    path: PathBuf,
}

impl JsonOptions {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn config() -> io::Result<Self> {
        Ok(Self::new(crate::store::config_dir()?.join("agents.json")))
    }
}

impl OptionStore for JsonOptions {
    fn load(&self) -> io::Result<HashMap<String, Options>> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(err) => return Err(err),
        };
        let file: File = serde_json::from_str(&text).map_err(io::Error::other)?;
        Ok(file
            .agents
            .into_iter()
            .map(|(agent, kept)| (agent, kept.into()))
            .collect())
    }

    fn save(&self, known: &HashMap<String, Options>) -> io::Result<()> {
        let file = File {
            agents: known
                .iter()
                .map(|(agent, options)| (agent.clone(), options.into()))
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
