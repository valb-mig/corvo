use corvo_domain::{mentions, span};

const TEXT: &str = "fn go() {\n    let path = 1;\n    use_it(path, path);\n}\n";

#[test]
fn span_runs_from_the_first_line_to_the_end_of_the_last() {
    let span = span(TEXT, 1, 2).unwrap();
    assert_eq!(&TEXT[span], "    let path = 1;\n    use_it(path, path);");
}

#[test]
fn span_outside_the_text_is_none() {
    assert!(span(TEXT, 2, 9).is_none());
    assert!(span(TEXT, 2, 1).is_none());
}

#[test]
fn span_leaves_out_carriage_returns() {
    let text = "a\r\nb\r\n";
    assert_eq!(&text[span(text, 0, 0).unwrap()], "a");
}

#[test]
fn mentions_group_by_line_without_indent() {
    let ranges: Vec<_> = TEXT
        .match_indices("path")
        .map(|(ix, _)| ix..ix + 4)
        .collect();

    let found = mentions(TEXT, &ranges);

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].line(), 1);
    assert_eq!(found[0].text(), "let path = 1;");
    let mark = 4..8;
    assert_eq!(found[0].marks(), std::slice::from_ref(&mark));
    assert_eq!(found[1].line(), 2);
    assert_eq!(found[1].marks(), [7..11, 13..17]);
}
