use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use super::Profile;
use crate::cache;

const JVM: &str = "/usr/lib/jvm";
const MIN: u32 = 21;

pub struct Java;

impl Profile for Java {
    fn args(&self, root: &Path) -> Vec<String> {
        let mut hasher = DefaultHasher::new();
        root.hash(&mut hasher);
        let data = cache::dir()
            .join("jdtls")
            .join(format!("{:x}", hasher.finish()));
        vec!["-data".into(), data.to_string_lossy().into_owned()]
    }

    fn env(&self) -> Vec<(String, String)> {
        let Ok(dirs) = std::fs::read_dir(JVM) else {
            return Vec::new();
        };
        let names: Vec<String> = dirs
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        pick(names.iter().map(String::as_str))
            .map(|name| vec![("JAVA_HOME".into(), format!("{JVM}/{name}"))])
            .unwrap_or_default()
    }
}

pub fn pick<'a>(names: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    names
        .filter_map(|name| version(name).map(|v| (v, name)))
        .filter(|(v, _)| *v >= MIN)
        .max_by_key(|(v, _)| *v)
        .map(|(_, name)| name)
}

fn version(name: &str) -> Option<u32> {
    name.strip_prefix("java-")?.split('-').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::pick;

    #[test]
    fn picks_newest_supported_jdk() {
        let names = [
            "default",
            "java-17-openjdk",
            "java-27-openjdk",
            "java-21-openjdk",
        ];
        assert_eq!(pick(names.into_iter()), Some("java-27-openjdk"));
    }

    #[test]
    fn ignores_old_jdks() {
        let names = ["default", "java-17-openjdk"];
        assert_eq!(pick(names.into_iter()), None);
    }
}
