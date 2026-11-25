use corvo_ui::file_icon::{Look, Mark, Tone, look};
use gpui_kit::assets::IconName as Glyph;

#[test]
fn languages_get_letters() {
    assert_eq!(look("main.rs"), Look::Letters("Rs", Tone::Orange));
    assert_eq!(look("index.TS"), Look::Letters("TS", Tone::Blue));
    assert_eq!(look("User.php"), Look::Letters("php", Tone::Indigo));
}

#[test]
fn whole_names_win_over_extensions() {
    assert_eq!(
        look("package.json"),
        Look::Glyph(Glyph::Package, Tone::Green)
    );
    assert_eq!(look("data.json"), Look::Glyph(Glyph::Braces, Tone::Yellow));
    assert_eq!(look("CLAUDE.md"), Look::Mark(Mark::Claude, Tone::Claude));
    assert_eq!(look("README.md"), Look::Glyph(Glyph::BookOpen, Tone::Sky));
    assert_eq!(look("notes.md"), Look::Mark(Mark::Markdown, Tone::Sky));
    assert_eq!(look("justfile"), Look::Plain(Glyph::Hammer));
}

#[test]
fn patterns_cover_env_tests_and_ignores() {
    let key = Look::Glyph(Glyph::KeyRound, Tone::Yellow);
    let flask = Look::Glyph(Glyph::FlaskConical, Tone::Green);
    assert_eq!(look(".env"), key);
    assert_eq!(look(".env.local"), key);
    assert_eq!(look("prod.env"), key);
    assert_eq!(look("app.test.tsx"), flask);
    assert_eq!(look("api.spec.mjs"), flask);
    assert_eq!(look("test_api.py"), flask);
    assert_eq!(look("store_test.go"), flask);
    assert_eq!(look("contest.ts"), Look::Letters("TS", Tone::Blue));
    assert_eq!(look(".prettierignore"), Look::Plain(Glyph::EyeOff));
    assert_eq!(look("LICENSE"), Look::Plain(Glyph::Scale));
}

#[test]
fn unknown_files_get_the_plain_leaf() {
    assert_eq!(look("Cargo.lock"), Look::Plain(Glyph::FileLock));
    assert_eq!(look("mystery"), Look::Plain(Glyph::File));
    assert_eq!(look(".hidden"), Look::Plain(Glyph::File));
}

#[test]
fn known_folders_get_a_corner_badge() {
    use corvo_ui::file_icon::{Badge, badge};
    assert_eq!(
        badge("src"),
        Some(Badge::Glyph(Glyph::Code, Some(Tone::Blue)))
    );
    assert_eq!(
        badge(".github"),
        Some(Badge::Glyph(Glyph::GitBranch, Some(Tone::Orange)))
    );
    assert_eq!(
        badge("Tests"),
        Some(Badge::Glyph(Glyph::FlaskConical, Some(Tone::Green)))
    );
    assert_eq!(badge("utils"), Some(Badge::Glyph(Glyph::Wrench, None)));
    assert_eq!(badge(".claude"), Some(Badge::Claude));
    assert_eq!(badge("crates"), None);
}
