use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::port::Fs;
use crate::scan;

// How much each folder of a project holds all told, as the home shows it: what git ignores and
// the vendored and built folders scan skips are left out, so a Rust project is not its target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Weights {
    folders: HashMap<PathBuf, Weight>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weight {
    pub bytes: u64,
    // False when the look stopped before the end of the folder: it holds more than that.
    pub whole: bool,
}

impl Weights {
    pub fn of(&self, folder: &Path) -> Option<Weight> {
        self.folders.get(folder).copied()
    }
}

// How many entries a project is looked over for, and how deep.
const ENTRIES: usize = 200_000;
const DEPTH: usize = 32;

pub fn weigh(fs: &dyn Fs, root: &Path) -> Weights {
    let mut weights = Weights::default();
    let mut left = ENTRIES;
    walk(fs, root, 0, &mut left, &mut weights);
    weights
}

// The weight of a folder, putting down that of every folder in it on the way.
fn walk(fs: &dyn Fs, dir: &Path, depth: usize, left: &mut usize, weights: &mut Weights) -> Weight {
    let mut weight = Weight {
        bytes: 0,
        whole: true,
    };
    let Ok(nodes) = fs.list(dir) else {
        weights.folders.insert(dir.to_path_buf(), weight);
        return weight;
    };
    for node in nodes {
        if *left == 0 {
            weight.whole = false;
            break;
        }
        *left -= 1;
        if !node.is_dir() {
            weight.bytes += fs.size(node.path()).unwrap_or(0);
        } else if depth < DEPTH && !scan::SKIP.contains(&node.name()) {
            let inner = walk(fs, node.path(), depth + 1, left, weights);
            weight.bytes += inner.bytes;
            weight.whole &= inner.whole;
        }
    }
    weights.folders.insert(dir.to_path_buf(), weight);
    weight
}

// A size as the cards show it: bytes, then KB, MB and GB of 1024, one decimal under 10.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024. && unit < UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    match unit {
        0 => format!("{n} B"),
        _ if value < 10. => format!("{value:.1} {}", UNITS[unit]),
        _ => format!("{} {}", value.round() as u64, UNITS[unit]),
    }
}
