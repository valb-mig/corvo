use std::fs;
use std::path::{Path, PathBuf};

use corvo_domain::Lang;
use corvo_domain::server::{Registry, Spec, root};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("corvo-root-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_main_language_has_a_server() {
    let registry = Registry::default();
    for lang in [
        Lang::Rust,
        Lang::JavaScript,
        Lang::TypeScript,
        Lang::Tsx,
        Lang::Php,
        Lang::Java,
        Lang::Kotlin,
    ] {
        assert!(registry.find(lang).is_some(), "{lang:?}");
    }
    assert!(registry.find(Lang::Plain).is_none());
}

#[test]
fn add_overrides_by_name_and_wins() {
    let mut registry = Registry::default();
    registry.add(Spec::new("phpantom", "phpantom_lsp").langs([Lang::Php]));

    assert_eq!(registry.find(Lang::Php).map(Spec::name), Some("phpantom"));
}

#[test]
fn root_is_nearest_marker_inside_workspace() {
    let ws = scratch("nested");
    let app = ws.join("apps/web");
    fs::create_dir_all(app.join("src")).unwrap();
    fs::write(ws.join("package.json"), "{}").unwrap();
    fs::write(app.join("package.json"), "{}").unwrap();
    let markers = vec!["package.json".to_string()];

    let found = root(&app.join("src/main.ts"), &markers, &ws);

    assert_eq!(found, app);
    fs::remove_dir_all(ws).unwrap();
}

#[test]
fn root_falls_back_to_workspace() {
    let ws = scratch("fallback");
    fs::create_dir_all(ws.join("src")).unwrap();
    let markers = vec!["Cargo.toml".to_string()];

    let found = root(&ws.join("src/lib.rs"), &markers, &ws);

    assert_eq!(found, ws);
    fs::remove_dir_all(ws).unwrap();
}

#[test]
fn root_never_leaves_workspace() {
    let ws = scratch("outside");
    let markers = vec!["Cargo.toml".to_string()];

    let found = root(Path::new("/elsewhere/src/lib.rs"), &markers, &ws);

    assert_eq!(found, ws);
    fs::remove_dir_all(ws).unwrap();
}
