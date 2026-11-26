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
fn ctrl_shift_e_shows_the_code_from_anywhere() {
    let keymap = Keymap::default();
    let show = keymap
        .bindings()
        .iter()
        .find(|b| b.chord() == "ctrl-shift-e");
    assert_eq!(show.map(Binding::command), Some(Command::ShowCode));
    assert_eq!(show.map(Binding::scope), Some(Scope::Global));
}

#[test]
fn ctrl_shift_g_shows_git_from_anywhere() {
    let keymap = Keymap::default();
    let show = keymap
        .bindings()
        .iter()
        .find(|b| b.chord() == "ctrl-shift-g");
    assert_eq!(show.map(Binding::command), Some(Command::ShowGit));
    assert_eq!(show.map(Binding::scope), Some(Scope::Global));
}

#[test]
fn ctrl_shift_c_shows_the_chats_from_anywhere() {
    let keymap = Keymap::default();
    let show = keymap
        .bindings()
        .iter()
        .find(|b| b.chord() == "ctrl-shift-c");
    assert_eq!(show.map(Binding::command), Some(Command::ShowChat));
    assert_eq!(show.map(Binding::scope), Some(Scope::Global));
}

#[test]
fn ctrl_k_opens_the_palette_from_anywhere() {
    let keymap = Keymap::default();
    let open = keymap.bindings().iter().find(|b| b.chord() == "ctrl-k");
    assert_eq!(open.map(Binding::command), Some(Command::OpenPalette));
    assert_eq!(open.map(Binding::scope), Some(Scope::Global));
}

#[test]
fn ctrl_b_toggles_the_panel_from_anywhere() {
    let keymap = Keymap::default();
    let toggle = keymap.bindings().iter().find(|b| b.chord() == "ctrl-b");
    assert_eq!(toggle.map(Binding::command), Some(Command::TogglePanel));
    assert_eq!(toggle.map(Binding::scope), Some(Scope::Global));
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
