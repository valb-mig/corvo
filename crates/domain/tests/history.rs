use corvo_domain::{Commit, Log, LogScope, RefName, Row, graph};

fn commit(hash: &str, parents: &[&str]) -> Commit {
    Commit {
        hash: hash.to_owned(),
        parents: parents.iter().map(|p| (*p).to_owned()).collect(),
        author: "dev".into(),
        email: "dev@x".into(),
        when: 0,
        subject: hash.to_owned(),
        refs: Vec::new(),
        note: None,
    }
}

fn row(lane: usize, through: &[usize], into: &[usize], out: &[usize]) -> Row {
    Row {
        lane,
        through: through.to_vec(),
        into: into.to_vec(),
        out: out.to_vec(),
    }
}

#[test]
fn a_straight_history_stays_in_one_lane() {
    let rows = graph(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);
    assert_eq!(
        rows,
        vec![
            row(0, &[], &[], &[0]),
            row(0, &[], &[0], &[0]),
            row(0, &[], &[0], &[]),
        ]
    );
}

#[test]
fn a_branch_runs_beside_and_joins_the_lane_awaiting_its_parent() {
    // f and e on a branch from b; d and c on main after b.
    let rows = graph(&[
        commit("f", &["e"]),
        commit("d", &["c"]),
        commit("e", &["b"]),
        commit("c", &["b"]),
        commit("b", &["a"]),
        commit("a", &[]),
    ]);
    assert_eq!(rows[0], row(0, &[], &[], &[0]));
    // d starts a lane of its own beside f's.
    assert_eq!(rows[1], row(1, &[0], &[], &[1]));
    assert_eq!(rows[2], row(0, &[1], &[0], &[0]));
    // c's parent is already awaited in the first lane: its line bends over to it.
    assert_eq!(rows[3], row(1, &[0], &[1], &[0]));
    assert_eq!(rows[4], row(0, &[], &[0], &[0]));
    assert_eq!(rows[5], row(0, &[], &[0], &[]));
}

#[test]
fn a_merge_opens_a_lane_for_its_second_parent() {
    // m merges x (a branch off a) into b.
    let rows = graph(&[
        commit("m", &["b", "x"]),
        commit("x", &["a"]),
        commit("b", &["a"]),
        commit("a", &[]),
    ]);
    assert_eq!(rows[0], row(0, &[], &[], &[0, 1]));
    assert_eq!(rows[1], row(1, &[0], &[1], &[1]));
    assert_eq!(rows[2], row(0, &[1], &[0], &[0]));
    assert_eq!(rows[3], row(0, &[], &[0, 1], &[]));
    assert_eq!(rows.iter().map(Row::width).max(), Some(2));
}

#[test]
fn a_parent_off_the_page_leaves_its_lane_running() {
    let rows = graph(&[commit("b", &["a"])]);
    assert_eq!(rows, vec![row(0, &[], &[], &[0])]);
}

#[test]
fn a_freed_lane_is_taken_again() {
    // y's branch ends at a root; z starts after it and takes its column back.
    let rows = graph(&[
        commit("x", &["w"]),
        commit("y", &[]),
        commit("z", &["w"]),
        commit("w", &[]),
    ]);
    assert_eq!(rows[1], row(1, &[0], &[], &[]));
    assert_eq!(rows[2].lane, 1);
}

#[test]
fn refs_read_from_full_names() {
    assert_eq!(
        RefName::from_full("refs/heads/feat/x"),
        Some(RefName::Local("feat/x".into()))
    );
    assert_eq!(
        RefName::from_full("refs/remotes/origin/main"),
        Some(RefName::Remote("origin/main".into()))
    );
    assert_eq!(RefName::from_full("refs/remotes/origin/HEAD"), None);
    assert_eq!(
        RefName::from_full("refs/tags/v1"),
        Some(RefName::Tag("v1".into()))
    );
    assert_eq!(RefName::from_full("refs/stash"), None);
}

#[test]
fn only_a_plain_history_draws_its_graph() {
    assert!(Log::new(LogScope::All, "").draws_graph());
    assert!(!Log::new(LogScope::All, "fix").draws_graph());
    assert!(!Log::new(LogScope::Reflog, "").draws_graph());
    let next = Log::new(LogScope::All, "").next_page();
    assert_eq!((next.skip, next.limit), (Log::PAGE, Log::PAGE));
}
