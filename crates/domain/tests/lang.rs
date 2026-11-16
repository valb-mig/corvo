use std::path::Path;

use corvo_domain::Lang;

#[test]
fn detects_by_extension() {
    assert_eq!(Lang::of(Path::new("src/main.rs")), Lang::Rust);
    assert_eq!(Lang::of(Path::new("Cargo.toml")), Lang::Toml);
    assert_eq!(Lang::of(Path::new("app.TSX")), Lang::Tsx);
    assert_eq!(Lang::of(Path::new("README.md")), Lang::Markdown);
}

#[test]
fn falls_back_to_plain() {
    assert_eq!(Lang::of(Path::new("LICENSE")), Lang::Plain);
    assert_eq!(Lang::of(Path::new("photo.png")), Lang::Plain);
}
