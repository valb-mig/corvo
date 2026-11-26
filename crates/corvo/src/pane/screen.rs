use std::cell::Cell as Slot;
use std::rc::Rc;
use std::sync::Arc;

use corvo_terminal::{CURSOR, Cell, Frame, Half, Rgb, Shape, Size as Cells, Terminal};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, BorderStyle, Bounds, Font, FontStyle, FontWeight, Hsla, IntoElement, Pixels, Point,
    ShapedLine, SharedString, Size, Styled as _, TextAlign, TextRun, UnderlineStyle, Window,
    canvas, fill, font, outline, point, px, rgb, size,
};

// The size of the terminal's text, and its line height to it.
const FONT_SIZE: f32 = 13.;
const LINE: f32 = 1.4;

// Where the grid was last painted, to find the cell under the mouse.
#[derive(Clone, Copy, Debug, Default)]
pub struct Layout {
    origin: (f32, f32),
    cell: (f32, f32),
    cols: usize,
    rows: usize,
}

impl Layout {
    pub fn line_height(&self) -> f32 {
        self.cell.1.max(1.)
    }

    // The cell under a point of the window, and the half of it the point is in; a point off the
    // grid takes the nearest cell.
    pub fn cell_at(&self, at: Point<Pixels>) -> Option<(usize, usize, Half)> {
        if self.cols == 0 || self.rows == 0 || self.cell.0 <= 0. {
            return None;
        }
        let x = (f32::from(at.x) - self.origin.0) / self.cell.0;
        let y = (f32::from(at.y) - self.origin.1) / self.cell.1;
        let (last_col, last_row) = (self.cols - 1, self.rows - 1);
        let row = (y.floor().max(0.) as usize).min(last_row);
        let (col, half) = if x < 0. {
            (0, Half::Left)
        } else if x >= self.cols as f32 {
            (last_col, Half::Right)
        } else {
            let half = if x.fract() < 0.5 {
                Half::Left
            } else {
                Half::Right
            };
            (x.floor() as usize, half)
        };
        Some((row, col, half))
    }
}

// The room left around the grid: the panel on the right, the agent bar at the foot.
#[derive(Clone, Copy, Debug)]
pub struct Inset {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

struct Painted {
    rects: Vec<(Bounds<Pixels>, Hsla)>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    cursor: Option<Mark>,
    line_height: Pixels,
}

struct Mark {
    bounds: Bounds<Pixels>,
    hollow: bool,
    // The character under a block cursor, in the background's color.
    under: Option<(Point<Pixels>, ShapedLine)>,
}

// The screen of a terminal, fitted to the room it is given: its size in cells follows the room,
// and the terminal is told whenever it changes.
pub fn screen(
    session: Arc<Terminal>,
    layout: Rc<Slot<Layout>>,
    focused: bool,
    inset: Inset,
) -> impl IntoElement {
    canvas(
        move |bounds, window, cx| prepare(&session, &layout, focused, inset, bounds, window, cx),
        |_, painted, window, cx| paint(painted, window, cx),
    )
    .size_full()
}

fn prepare(
    session: &Terminal,
    layout: &Slot<Layout>,
    focused: bool,
    inset: Inset,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> Painted {
    let theme = cx.theme();
    let (background, selection) = (theme.background, theme.selection);
    let base = font(theme.mono_font_family.clone());
    let font_size = px(FONT_SIZE);
    let system = window.text_system().clone();
    let id = system.resolve_font(&base);
    let cell_width = system
        .advance(id, font_size, 'm')
        .map(|advance| f32::from(advance.width))
        .unwrap_or(8.)
        .max(1.);
    let line_height = (FONT_SIZE * LINE).round();

    let origin = (
        f32::from(bounds.origin.x) + inset.left,
        f32::from(bounds.origin.y) + inset.top,
    );
    let width = f32::from(bounds.size.width) - inset.left - inset.right;
    let height = f32::from(bounds.size.height) - inset.top - inset.bottom;
    let cols = (width / cell_width).floor().clamp(2., u16::MAX as f32) as u16;
    let rows = (height / line_height).floor().clamp(1., u16::MAX as f32) as u16;
    session.resize(Cells::new(cols, rows).cells(cell_width.round() as u16, line_height as u16));
    let frame = session.frame();
    layout.set(Layout {
        origin,
        cell: (cell_width, line_height),
        cols: frame.cols,
        rows: frame.rows,
    });

    let at = |row: usize, col: usize| {
        point(
            px(origin.0 + col as f32 * cell_width),
            px(origin.1 + row as f32 * line_height),
        )
    };
    let cell = size(px(cell_width), px(line_height));
    let rects = backgrounds(&frame, selection, at, cell);
    let shape = |text: String, cell: &Cell, color: Hsla| {
        let run = TextRun {
            len: text.len(),
            font: styled(&base, cell),
            color,
            background_color: None,
            underline: cell.underline.then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(color),
                wavy: false,
            }),
            strikethrough: cell.strike.then_some(gpui_kit::StrikethroughStyle {
                thickness: px(1.),
                color: Some(color),
            }),
        };
        system.shape_line(
            SharedString::from(text),
            font_size,
            &[run],
            Some(px(cell_width)),
        )
    };

    let mut lines = Vec::new();
    for (row, cells) in frame.lines.iter().enumerate() {
        for (col, text, cell) in runs(cells) {
            lines.push((at(row, col), shape(text, cell, hsla(cell.fg))));
        }
    }

    let cursor = frame.cursor.map(|cursor| {
        let width = if cursor.wide {
            cell_width * 2.
        } else {
            cell_width
        };
        let origin = at(cursor.row, cursor.col);
        let (bounds, block) = match (cursor.shape, focused) {
            (Shape::Block, true) => (Bounds::new(origin, size(px(width), px(line_height))), true),
            (Shape::Beam, _) => (Bounds::new(origin, size(px(2.), px(line_height))), false),
            (Shape::Underline, _) => (
                Bounds::new(
                    point(origin.x, origin.y + px(line_height - 2.)),
                    size(px(width), px(2.)),
                ),
                false,
            ),
            (Shape::Block | Shape::Hollow, _) => {
                (Bounds::new(origin, size(px(width), px(line_height))), false)
            }
        };
        let hollow = matches!(cursor.shape, Shape::Block | Shape::Hollow) && !block;
        let under = block
            .then(|| frame.lines[cursor.row].get(cursor.col))
            .flatten()
            .filter(|cell| cell.c != ' ')
            .map(|cell| {
                let mut text = cell.c.to_string();
                text.extend(cell.marks.iter().flat_map(|marks| marks.chars()));
                (origin, shape(text, cell, background))
            });
        Mark {
            bounds,
            hollow,
            under,
        }
    });

    Painted {
        rects,
        lines,
        cursor,
        line_height: px(line_height),
    }
}

fn paint(painted: Painted, window: &mut Window, cx: &mut App) {
    for (bounds, color) in painted.rects {
        window.paint_quad(fill(bounds, color));
    }
    for (origin, line) in &painted.lines {
        line.paint(
            *origin,
            painted.line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();
    }
    if let Some(mark) = painted.cursor {
        let color = hsla(CURSOR);
        if mark.hollow {
            window.paint_quad(outline(mark.bounds, color, BorderStyle::Solid));
        } else {
            window.paint_quad(fill(mark.bounds, color));
        }
        if let Some((origin, line)) = mark.under {
            line.paint(
                origin,
                painted.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
        }
    }
}

// The fills behind the cells that have a background of their own or are picked, a stretch of
// cells of the same color at a time.
fn backgrounds(
    frame: &Frame,
    selection: Hsla,
    at: impl Fn(usize, usize) -> Point<Pixels>,
    cell: Size<Pixels>,
) -> Vec<(Bounds<Pixels>, Hsla)> {
    let mut rects = Vec::new();
    for (row, cells) in frame.lines.iter().enumerate() {
        let mut stretch: Option<(usize, usize, Hsla)> = None;
        for (col, c) in cells.iter().enumerate() {
            let color = if c.selected {
                Some(selection)
            } else {
                c.bg.map(hsla)
            };
            match (stretch, color) {
                (Some((from, _, open)), Some(color)) if open == color => {
                    stretch = Some((from, col, color));
                }
                (_, color) => {
                    if let Some((from, to, open)) = stretch.take() {
                        rects.push((span(&at, row, from, to, cell), open));
                    }
                    stretch = color.map(|color| (col, col, color));
                }
            }
        }
        if let Some((from, to, open)) = stretch {
            rects.push((span(&at, row, from, to, cell), open));
        }
    }
    rects
}

fn span(
    at: &impl Fn(usize, usize) -> Point<Pixels>,
    row: usize,
    from: usize,
    to: usize,
    cell: Size<Pixels>,
) -> Bounds<Pixels> {
    let width = cell.width * (to - from + 1) as f32;
    Bounds::new(at(row, from), size(width, cell.height))
}

// The text of a row in stretches of one style, each with the column it starts at; a character
// two cells wide stands alone, so the ones after it keep their columns. Blank stretches with
// nothing to draw are left out.
fn runs(cells: &[Cell]) -> Vec<(usize, String, &Cell)> {
    let mut runs: Vec<(usize, String, &Cell)> = Vec::new();
    let mut open: Option<(usize, String, &Cell)> = None;
    for (col, cell) in cells.iter().enumerate() {
        if cell.spacer {
            continue;
        }
        let joins = open
            .as_ref()
            .is_some_and(|(_, _, first)| !cell.wide && !first.wide && same(first, cell));
        if !joins && let Some(run) = open.take() {
            runs.push(run);
        }
        let run = open.get_or_insert_with(|| (col, String::new(), cell));
        run.1.push(cell.c);
        if let Some(marks) = &cell.marks {
            run.1.push_str(marks);
        }
    }
    runs.extend(open);
    runs.retain(|(_, text, cell)| !text.trim().is_empty() || cell.underline || cell.strike);
    runs
}

fn same(a: &Cell, b: &Cell) -> bool {
    a.fg == b.fg
        && a.bold == b.bold
        && a.italic == b.italic
        && a.underline == b.underline
        && a.strike == b.strike
}

fn styled(base: &Font, cell: &Cell) -> Font {
    let mut font = base.clone();
    if cell.bold {
        font.weight = FontWeight::BOLD;
    }
    if cell.italic {
        font.style = FontStyle::Italic;
    }
    font
}

fn hsla(color: Rgb) -> Hsla {
    rgb(color.hex()).into()
}
