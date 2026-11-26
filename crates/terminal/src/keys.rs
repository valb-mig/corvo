// The bytes a key sends to the program in the terminal, as xterm sends them.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    // The modifier number of xterm: 1, plus 1 for shift, 2 for alt, 4 for ctrl.
    fn code(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }
}

// What a key sends: `key` is its name (`enter`, `up`, `f5`, `a`), `text` what it types with the
// keyboard's layout and shift (`A`, `ç`), and `app_cursor` whether the program asked for the
// arrows of application mode, as vim and less do. None for a key that sends nothing.
pub fn encode(key: &str, text: Option<&str>, mods: Mods, app_cursor: bool) -> Option<Vec<u8>> {
    let code = mods.code();
    let bytes = |s: &str| Some(s.as_bytes().to_vec());
    match key {
        "enter" => bytes(if mods.alt { "\x1b\r" } else { "\r" }),
        "tab" if mods.shift => bytes("\x1b[Z"),
        "tab" => bytes(if mods.alt { "\x1b\t" } else { "\t" }),
        "backspace" if mods.ctrl => bytes("\x08"),
        "backspace" => bytes(if mods.alt { "\x1b\x7f" } else { "\x7f" }),
        "escape" => bytes("\x1b"),
        "space" if mods.ctrl => Some(vec![0]),
        "space" => bytes(if mods.alt { "\x1b " } else { " " }),
        "up" | "down" | "right" | "left" | "home" | "end" => {
            let letter = match key {
                "up" => 'A',
                "down" => 'B',
                "right" => 'C',
                "left" => 'D',
                "home" => 'H',
                _ => 'F',
            };
            let seq = if code > 1 {
                format!("\x1b[1;{code}{letter}")
            } else if app_cursor {
                format!("\x1bO{letter}")
            } else {
                format!("\x1b[{letter}")
            };
            Some(seq.into_bytes())
        }
        "insert" | "delete" | "pageup" | "pagedown" => {
            let number = match key {
                "insert" => 2,
                "delete" => 3,
                "pageup" => 5,
                _ => 6,
            };
            Some(tilde(number, code))
        }
        "f1" | "f2" | "f3" | "f4" => {
            let letter = match key {
                "f1" => 'P',
                "f2" => 'Q',
                "f3" => 'R',
                _ => 'S',
            };
            let seq = if code > 1 {
                format!("\x1b[1;{code}{letter}")
            } else {
                format!("\x1bO{letter}")
            };
            Some(seq.into_bytes())
        }
        "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" => {
            let number = match key {
                "f5" => 15,
                "f6" => 17,
                "f7" => 18,
                "f8" => 19,
                "f9" => 20,
                "f10" => 21,
                "f11" => 23,
                _ => 24,
            };
            Some(tilde(number, code))
        }
        _ => typed(key, text, mods),
    }
}

// `ESC [ n ~`, with the modifiers when there are some.
fn tilde(number: u8, code: u8) -> Vec<u8> {
    if code > 1 {
        format!("\x1b[{number};{code}~").into_bytes()
    } else {
        format!("\x1b[{number}~").into_bytes()
    }
}

// A key that types: ctrl turns it into its control byte, alt sends an escape before it.
fn typed(key: &str, text: Option<&str>, mods: Mods) -> Option<Vec<u8>> {
    let mut chars = key.chars();
    let single = chars.next().filter(|_| chars.as_str().is_empty());
    let mut out = Vec::new();
    if mods.alt {
        out.push(0x1b);
    }
    if mods.ctrl {
        out.push(control(single?)?);
        return Some(out);
    }
    match (text, single) {
        (Some(text), _) if !text.is_empty() => out.extend_from_slice(text.as_bytes()),
        (_, Some(c)) if mods.alt => {
            let mut buf = [0; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
        _ => return None,
    }
    Some(out)
}

// The control byte of a key held with ctrl: `ctrl-c` is 3, `ctrl-[` is escape.
fn control(c: char) -> Option<u8> {
    match c.to_ascii_lowercase() {
        c @ 'a'..='z' => Some(c as u8 - b'a' + 1),
        '@' | '2' | ' ' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '/' | '-' | '7' => Some(0x1f),
        '8' | '?' => Some(0x7f),
        _ => None,
    }
}
