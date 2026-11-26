use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

// The colors of the terminal, picked for the black of the app: the 16 of ANSI, the cube and
// greys of the 256, and the text, background and cursor. A program may set its own, which win.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(hex: u32) -> Self {
        Self {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
        }
    }

    // As a 0xRRGGBB number.
    pub fn hex(self) -> u32 {
        (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }

    fn dim(self) -> Self {
        let dim = |c: u8| (c as f32 * 0.66) as u8;
        Self {
            r: dim(self.r),
            g: dim(self.g),
            b: dim(self.b),
        }
    }
}

impl From<alacritty_terminal::vte::ansi::Rgb> for Rgb {
    fn from(rgb: alacritty_terminal::vte::ansi::Rgb) -> Self {
        Self {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        }
    }
}

impl From<Rgb> for alacritty_terminal::vte::ansi::Rgb {
    fn from(rgb: Rgb) -> Self {
        Self {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        }
    }
}

pub const FOREGROUND: Rgb = Rgb::new(0xe4e4e7);
pub const BACKGROUND: Rgb = Rgb::new(0x0a0a0a);
pub const CURSOR: Rgb = Rgb::new(0xe4e4e7);

const ANSI: [Rgb; 16] = [
    Rgb::new(0x3f3f46),
    Rgb::new(0xf87171),
    Rgb::new(0x4ade80),
    Rgb::new(0xfacc15),
    Rgb::new(0x60a5fa),
    Rgb::new(0xc084fc),
    Rgb::new(0x22d3ee),
    Rgb::new(0xd4d4d8),
    Rgb::new(0x71717a),
    Rgb::new(0xfca5a5),
    Rgb::new(0x86efac),
    Rgb::new(0xfde047),
    Rgb::new(0x93c5fd),
    Rgb::new(0xd8b4fe),
    Rgb::new(0x67e8f9),
    Rgb::new(0xfafafa),
];

// The color at a place of the palette: the 16 of ANSI, the 6×6×6 cube, the 24 greys, then the
// named ones past 255 (text, background, cursor, the dim ones).
pub fn palette(index: usize) -> Rgb {
    const STEP: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match index {
        0..16 => ANSI[index],
        16..232 => {
            let at = index - 16;
            Rgb {
                r: STEP[at / 36],
                g: STEP[at / 6 % 6],
                b: STEP[at % 6],
            }
        }
        232..256 => {
            let grey = 8 + 10 * (index - 232) as u8;
            Rgb {
                r: grey,
                g: grey,
                b: grey,
            }
        }
        _ => match index {
            i if i == NamedColor::Foreground as usize => FOREGROUND,
            i if i == NamedColor::Background as usize => BACKGROUND,
            i if i == NamedColor::Cursor as usize => CURSOR,
            i if i == NamedColor::BrightForeground as usize => Rgb::new(0xffffff),
            i if i == NamedColor::DimForeground as usize => FOREGROUND.dim(),
            i if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize).contains(&i) => {
                ANSI[i - NamedColor::DimBlack as usize].dim()
            }
            _ => FOREGROUND,
        },
    }
}

// A color of a cell as painted: what a program set for its place first, the palette otherwise.
pub(crate) fn resolve(color: Color, colors: &Colors) -> Rgb {
    let index = match color {
        Color::Spec(rgb) => return rgb.into(),
        Color::Indexed(index) => index as usize,
        Color::Named(named) => named as usize,
    };
    colors[index].map_or_else(|| palette(index), Rgb::from)
}

// The dim of a color: a dim ANSI color has its own place, any other is darkened.
pub(crate) fn dim(color: Color, colors: &Colors) -> Rgb {
    match color {
        Color::Named(named) if (named as usize) < 8 => {
            resolve(Color::Named(DIM[named as usize]), colors)
        }
        _ => resolve(color, colors).dim(),
    }
}

const DIM: [NamedColor; 8] = [
    NamedColor::DimBlack,
    NamedColor::DimRed,
    NamedColor::DimGreen,
    NamedColor::DimYellow,
    NamedColor::DimBlue,
    NamedColor::DimMagenta,
    NamedColor::DimCyan,
    NamedColor::DimWhite,
];
