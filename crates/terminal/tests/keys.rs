use corvo_terminal::{Mods, encode};

const NONE: Mods = Mods {
    ctrl: false,
    alt: false,
    shift: false,
};
const CTRL: Mods = Mods {
    ctrl: true,
    alt: false,
    shift: false,
};
const ALT: Mods = Mods {
    ctrl: false,
    alt: true,
    shift: false,
};
const SHIFT: Mods = Mods {
    ctrl: false,
    alt: false,
    shift: true,
};

fn sends(key: &str, text: Option<&str>, mods: Mods) -> Option<Vec<u8>> {
    encode(key, text, mods, false)
}

#[test]
fn typed_text_goes_as_it_is_written() {
    assert_eq!(sends("a", Some("a"), NONE), Some(b"a".to_vec()));
    assert_eq!(sends("a", Some("A"), SHIFT), Some(b"A".to_vec()));
    assert_eq!(sends("c", Some("ç"), NONE), Some("ç".as_bytes().to_vec()));
}

#[test]
fn ctrl_sends_the_control_byte() {
    assert_eq!(sends("c", Some("c"), CTRL), Some(vec![3]));
    assert_eq!(sends("d", None, CTRL), Some(vec![4]));
    assert_eq!(sends("l", None, CTRL), Some(vec![12]));
    assert_eq!(sends("[", None, CTRL), Some(vec![0x1b]));
    assert_eq!(sends("space", None, CTRL), Some(vec![0]));
}

#[test]
fn alt_sends_escape_first() {
    assert_eq!(sends("b", None, ALT), Some(b"\x1bb".to_vec()));
    assert_eq!(sends("b", Some("b"), ALT), Some(b"\x1bb".to_vec()));
    assert_eq!(sends("backspace", None, ALT), Some(b"\x1b\x7f".to_vec()));
}

#[test]
fn editing_keys_send_what_a_shell_reads() {
    assert_eq!(sends("enter", None, NONE), Some(b"\r".to_vec()));
    assert_eq!(sends("backspace", None, NONE), Some(b"\x7f".to_vec()));
    assert_eq!(sends("tab", None, NONE), Some(b"\t".to_vec()));
    assert_eq!(sends("tab", None, SHIFT), Some(b"\x1b[Z".to_vec()));
    assert_eq!(sends("escape", None, NONE), Some(b"\x1b".to_vec()));
    assert_eq!(sends("delete", None, NONE), Some(b"\x1b[3~".to_vec()));
}

#[test]
fn arrows_follow_the_cursor_mode() {
    assert_eq!(encode("up", None, NONE, false), Some(b"\x1b[A".to_vec()));
    assert_eq!(encode("up", None, NONE, true), Some(b"\x1bOA".to_vec()));
    assert_eq!(encode("home", None, NONE, false), Some(b"\x1b[H".to_vec()));
}

#[test]
fn modifiers_go_in_the_sequence() {
    assert_eq!(sends("left", None, CTRL), Some(b"\x1b[1;5D".to_vec()));
    assert_eq!(
        encode("right", None, ALT, true),
        Some(b"\x1b[1;3C".to_vec())
    );
    assert_eq!(sends("pageup", None, SHIFT), Some(b"\x1b[5;2~".to_vec()));
    assert_eq!(sends("f5", None, CTRL), Some(b"\x1b[15;5~".to_vec()));
}

#[test]
fn function_keys_send_their_sequences() {
    assert_eq!(sends("f1", None, NONE), Some(b"\x1bOP".to_vec()));
    assert_eq!(sends("f4", None, NONE), Some(b"\x1bOS".to_vec()));
    assert_eq!(sends("f5", None, NONE), Some(b"\x1b[15~".to_vec()));
    assert_eq!(sends("f12", None, NONE), Some(b"\x1b[24~".to_vec()));
}

#[test]
fn a_key_that_types_nothing_sends_nothing() {
    assert_eq!(sends("capslock", None, NONE), None);
    assert_eq!(sends("f13", None, NONE), None);
}
