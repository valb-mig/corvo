use std::path::PathBuf;

use corvo_domain::{Finder, Found, Hit, Query};

fn query(text: &str) -> Query {
    Query {
        text: text.into(),
        ..Query::default()
    }
}

fn finder(query: Query) -> Finder {
    query.finder().unwrap().unwrap()
}

fn marked(hit: &Hit) -> &str {
    &hit.preview[hit.marks[0].clone()]
}

#[test]
fn an_empty_query_finds_nothing_and_is_no_error() {
    assert!(query("").finder().unwrap().is_none());
}

#[test]
fn case_is_ignored_unless_asked() {
    let text = "Foo foo FOO";

    assert_eq!(finder(query("foo")).find(text, 10)[0].count, 3);
    let exact = Query {
        case: true,
        ..query("foo")
    };
    assert_eq!(finder(exact).find(text, 10)[0].count, 1);
}

#[test]
fn places_on_one_line_make_one_hit_with_each_marked() {
    let hits = finder(query("ab")).find("ab x ab\nab", 10);

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].count, 2);
    assert_eq!(hits[0].marks, [0..2, 5..7]);
    assert_eq!(hits[0].range, 0..2);
    assert_eq!(hits[1].line, 1);
    assert_eq!(hits[1].range, 8..10);
}

#[test]
fn a_place_cut_out_of_a_long_line_is_counted_but_not_marked() {
    let line = format!("needle{}needle", "x".repeat(400));

    let hits = finder(query("needle")).find(&line, 10);

    assert_eq!(hits[0].count, 2);
    assert_eq!(hits[0].marks.len(), 1);
}

#[test]
fn a_whole_word_leaves_out_what_holds_it() {
    let text = "let row = rows[0]; arrow";
    let word = Query {
        word: true,
        ..query("row")
    };

    let hits = finder(word).find(text, 10);

    assert_eq!(hits[0].count, 1);
    assert_eq!(hits[0].range, 4..7);
}

#[test]
fn plain_text_is_not_read_as_an_expression() {
    let hits = finder(query("a.b(")).find("a.b( axb(", 10);

    assert_eq!(hits[0].count, 1);
}

#[test]
fn an_expression_finds_what_it_describes() {
    let regex = Query {
        regex: true,
        ..query(r"fn \w+")
    };

    let hits = finder(regex).find("fn one() {}\nfn two() {}", 10);

    let found: Vec<_> = hits.iter().map(marked).collect();
    assert_eq!(found, ["fn one", "fn two"]);
}

#[test]
fn an_expression_that_does_not_hold_says_why() {
    let broken = Query {
        regex: true,
        ..query("(unclosed")
    };

    let err = broken.finder().unwrap_err();

    assert!(err.to_lowercase().contains("unclosed"), "{err}");
    assert!(!err.contains('\n'));
}

#[test]
fn an_expression_matching_empty_text_is_refused() {
    for text in ["a*", "^", "$"] {
        let empty = Query {
            regex: true,
            ..query(text)
        };
        assert!(empty.finder().is_err(), "{text}");
    }
}

#[test]
fn what_matches_only_between_characters_is_no_hit() {
    let edge = finder(Query {
        regex: true,
        ..query("\\b")
    });

    assert!(edge.find("a b", 10).is_empty());
    assert_eq!(edge.replace("a b", "|"), ("a b".to_owned(), 0));
}

#[test]
fn hits_tell_their_line_and_show_it_without_its_indent() {
    let text = "first\n    let needle = 1;\r\nlast needle";

    let hits = finder(query("needle")).find(text, 10);

    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[0].preview, "let needle = 1;");
    assert_eq!(marked(&hits[0]), "needle");
    assert_eq!(&text[hits[0].range.clone()], "needle");
    assert_eq!(hits[1].line, 2);
    assert_eq!(hits[1].preview, "last needle");
}

#[test]
fn a_long_line_is_cut_around_the_hit() {
    let line = format!("{}needle{}", "x".repeat(300), "y".repeat(500));

    let hits = finder(query("needle")).find(&line, 10);

    let hit = &hits[0];
    assert!(hit.preview.starts_with('…'));
    assert!(hit.preview.len() < 300);
    assert_eq!(marked(hit), "needle");
}

#[test]
fn accents_before_a_hit_keep_its_mark_on_it() {
    let line = format!("{}ação", "é".repeat(60));

    let hits = finder(query("ação")).find(&line, 10);

    assert_eq!(marked(&hits[0]), "ação");
}

#[test]
fn hits_stop_at_the_most_asked() {
    assert_eq!(finder(query("a")).find("aaaaa", 3)[0].count, 3);
}

#[test]
fn replace_counts_and_keeps_plain_text_as_it_is() {
    let (text, count) = finder(query("a")).replace("a-a-b", "$1");

    assert_eq!(count, 2);
    assert_eq!(text, "$1-$1-b");
}

#[test]
fn replace_puts_an_expressions_groups_in() {
    let regex = Query {
        regex: true,
        ..query(r"(\w+)\.unwrap\(\)")
    };

    let (text, count) = finder(regex).replace("a.unwrap() + b.unwrap()", "$1?");

    assert_eq!(count, 2);
    assert_eq!(text, "a? + b?");
}

#[test]
fn found_stops_taking_lines_at_the_most_places() {
    let mut found = Found::default();
    let hits = finder(query("a")).find("a\na a\na", 10);

    assert!(found.add(PathBuf::from("/one"), hits.clone(), 6));
    assert!(!found.add(PathBuf::from("/two"), hits, 6));

    // Four places in the first file; in the second, its line of two would go over the most and
    // is left out whole, with what follows it.
    assert_eq!(found.hits, 5);
    assert!(found.capped);
    assert_eq!(found.files[1].1.len(), 1);
}
