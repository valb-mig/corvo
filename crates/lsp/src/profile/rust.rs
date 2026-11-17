use serde_json::{Value, json};

use super::Profile;

pub struct Rust;

impl Profile for Rust {
    fn options(&self) -> Option<Value> {
        Some(json!({
            "cargo": { "targetDir": true },
            "check": { "extraArgs": ["-j", "4"] },
            "cachePriming": { "numThreads": 2 },
        }))
    }

    fn settings(&self) -> Option<Value> {
        self.options()
            .map(|options| json!({ "rust-analyzer": options }))
    }
}
