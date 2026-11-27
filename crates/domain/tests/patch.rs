use corvo_domain::Patch;

const TWO_HUNKS: &str = "diff --git a/a.rs b/a.rs
index 1..2 100644
--- a/a.rs
+++ b/a.rs
@@ -1,4 +1,4 @@ fn top()
 line 1
-line 2
+line two
 line 3
 line 4
@@ -17,4 +17,5 @@ fn bottom()
 line 17
 line 18
-line 19
+line nineteen
+line 19b
 line 20
";

#[test]
fn reads_the_hunks_and_where_they_start() {
    let patch = Patch::parse(TWO_HUNKS);
    assert_eq!(patch.head.len(), 4);
    assert_eq!(patch.hunks.len(), 2);
    assert_eq!(
        (patch.hunks[1].old_start, patch.hunks[1].new_start),
        (17, 17)
    );
    assert_eq!(patch.hunks[0].scope, "fn top()");
    assert_eq!(patch.hunks[0].first_change(), Some((false, 2)));
    assert!(patch.hunks[1].holds(true, 20));
    assert!(!patch.hunks[1].holds(true, 22));
}

#[test]
fn cuts_out_one_hunk() {
    let one = Patch::parse(TWO_HUNKS).hunk(1).unwrap();
    assert!(one.starts_with("diff --git a/a.rs b/a.rs\n"));
    assert!(one.contains("@@ -17,4 +17,5 @@\n"));
    assert!(!one.contains("line two"));
}

#[test]
fn keeps_only_the_lines_picked() {
    // Only the added `line 19b`, at line 20 of the new side.
    let cut = Patch::parse(TWO_HUNKS)
        .lines(|new_side, line| new_side && line == 20, false)
        .unwrap();
    // The line taken out that was not picked stays as it was, the other one added is left out.
    assert!(
        cut.contains("@@ -17,4 +17,5 @@\n line 17\n line 18\n line 19\n+line 19b\n line 20\n"),
        "{cut}"
    );
    assert!(!cut.contains("line two"));
}

#[test]
fn keeps_only_the_lines_picked_to_apply_backwards() {
    let cut = Patch::parse(TWO_HUNKS)
        .lines(|new_side, line| new_side && line == 20, true)
        .unwrap();
    // Backwards, the other added line must be there to be found: it stays, as context; the
    // line taken out was never there.
    assert!(
        cut.contains(
            "@@ -17,4 +17,5 @@\n line 17\n line 18\n line nineteen\n+line 19b\n line 20\n"
        ),
        "{cut}"
    );
}

#[test]
fn nothing_picked_is_no_patch() {
    assert_eq!(Patch::parse(TWO_HUNKS).lines(|_, _| false, false), None);
}
