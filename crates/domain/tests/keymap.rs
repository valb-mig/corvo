use corvo_domain::keymap::{Binding, Command, Keymap, Scope};

#[test]
fn save_is_global_by_default() {
    let keymap = Keymap::default();
    let save = keymap
        .bindings()
        .iter()
        .find(|b| b.command() == Command::Save);
    assert_eq!(save.map(Binding::scope), Some(Scope::Global));
}

#[test]
fn bind_replaces_same_chord_in_same_scope() {
    let mut keymap = Keymap::default();
    keymap.bind(Binding::new("ctrl-z", Command::Redo, Scope::Editor));

    let hits: Vec<_> = keymap
        .bindings()
        .iter()
        .filter(|b| b.chord() == "ctrl-z")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].command(), Command::Redo);
}
