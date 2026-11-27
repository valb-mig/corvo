use corvo_ui::path::tail;

#[test]
fn a_short_path_stays_whole() {
    assert_eq!(tail("corvo/crates/ui", 30), "corvo/crates/ui");
}

#[test]
fn a_long_path_loses_whole_folders_at_the_front() {
    assert_eq!(
        tail("corvo/crates/corvo/src/pane/overview", 24),
        "…/src/pane/overview"
    );
    assert_eq!(tail("corvo/crates/corvo/src/home", 20), "…/corvo/src/home");
}

#[test]
fn a_name_too_long_for_any_cut_keeps_only_itself() {
    assert_eq!(
        tail("p/an-extremely-long-folder-name", 12),
        "…/an-extremely-long-folder-name"
    );
}
