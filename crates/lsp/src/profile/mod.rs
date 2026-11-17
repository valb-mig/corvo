mod java;
mod plain;
mod rust;

use std::path::Path;

use serde_json::Value;

pub use java::Java;
pub use plain::Plain;
pub use rust::Rust;

pub trait Profile {
    fn args(&self, _root: &Path) -> Vec<String> {
        Vec::new()
    }

    fn env(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    fn options(&self) -> Option<Value> {
        None
    }

    fn settings(&self) -> Option<Value> {
        None
    }
}

pub fn of(server: &str) -> Box<dyn Profile> {
    match server {
        "rust-analyzer" => Box::new(Rust),
        "jdtls" => Box::new(Java),
        _ => Box::new(Plain),
    }
}
