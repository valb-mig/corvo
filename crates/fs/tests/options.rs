use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use corvo_domain::port::OptionStore;
use corvo_domain::{Choice, Options, Pick};
use corvo_fs::JsonOptions;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("corvo-options-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn pick(id: &str, name: &str, description: Option<&str>) -> Pick {
    Pick {
        id: id.into(),
        name: name.into(),
        description: description.map(Into::into),
    }
}

fn options() -> Options {
    Options {
        model: Some(Choice {
            id: "model".into(),
            current: "opus".into(),
            values: vec![pick("opus", "Opus", None), pick("haiku", "Haiku", None)],
        }),
        effort: None,
        mode: Some(Choice {
            id: "mode".into(),
            current: "default".into(),
            values: vec![pick("default", "Manual", Some("Always ask"))],
        }),
    }
}

#[test]
fn keeps_what_each_agent_offered() {
    let dir = scratch("round");
    let store = JsonOptions::new(dir.join("agents.json"));
    let known = HashMap::from([("claude".to_owned(), options())]);
    store.save(&known).unwrap();
    assert_eq!(store.load().unwrap(), known);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn nothing_kept_loads_as_nothing() {
    let dir = scratch("empty");
    let store = JsonOptions::new(dir.join("agents.json"));
    assert!(store.load().unwrap().is_empty());
}
