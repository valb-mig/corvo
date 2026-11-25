use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use corvo_domain::{Doc, Lang, Mention, Symbol, SymbolKind, Workbench};
use corvo_ui::editor;
use corvo_ui::file_icon::FileIcon;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, FontWeight, IntoElement,
    ParentElement as _, SharedString, Styled as _, Subscription, Window, div, px, rems,
};

// Sizes inside go in rems, so they follow the graph's zoom; the heights handed to the graph are
// the same at zoom 1, a rem being 16 pixels there.
const REM: f32 = 16.;
const HEADER: f32 = 1.75;
const ROW: f32 = 1.25;
const CODE_ROWS: usize = 30;
const MENTION_ROWS: usize = 40;
const CODE_WIDTH: f32 = 600.;
const MENTIONS_WIDTH: f32 = 480.;

// A code panel is known by its file, its symbol's name and the line it starts on.
pub type Key = (PathBuf, String, u32);

pub fn key(file: &Path, symbol: &Symbol) -> Key {
    (
        file.to_path_buf(),
        symbol.name().to_owned(),
        symbol.first_line(),
    )
}

pub enum Panel {
    Code,
    Mentions,
}

// What an open symbol drops down: a function or a type its code, what holds a value the lines
// that mention it. A container (impl, class, module, interface) drops nothing: its members do.
pub fn panel(kind: SymbolKind) -> Option<Panel> {
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Struct | SymbolKind::Enum => {
            Some(Panel::Code)
        }
        SymbolKind::Type => Some(Panel::Code),
        SymbolKind::Field
        | SymbolKind::Variant
        | SymbolKind::Constant
        | SymbolKind::Variable
        | SymbolKind::Parameter => Some(Panel::Mentions),
        SymbolKind::Impl | SymbolKind::Class | SymbolKind::Module | SymbolKind::Interface => None,
    }
}

// The lines of a symbol in an editor of their own. Saving writes them back over the same lines
// of the file; it is locked while the file has unsaved changes in a tab.
pub struct Code {
    file: PathBuf,
    first: u32,
    last: u32,
    original: String,
    state: Entity<editor::State>,
    locked: bool,
    _sub: Subscription,
}

impl Code {
    pub fn open<V: 'static>(
        bench: &Entity<Workbench>,
        file: &Path,
        symbol: &Symbol,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> io::Result<Self> {
        let (first, last) = (symbol.first_line(), symbol.last_line());
        let bench = bench.read(cx);
        let original = bench.excerpt(file, first, last)?;
        let locked = bench.tabs().get(file).is_some_and(Doc::is_dirty);
        let text = original.clone();
        let state = cx.new(|cx| {
            let mut state = editor::snippet(Lang::of(file), text, window, cx);
            state.set_readonly(locked, cx);
            state
        });
        let sub = cx.subscribe(&state, |_, _, event: &editor::Event, cx| {
            if matches!(event, editor::Event::Change) {
                cx.notify();
            }
        });
        Ok(Self {
            file: file.to_path_buf(),
            first,
            last,
            original,
            state,
            locked,
            _sub: sub,
        })
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.state.focus_handle(cx).contains_focused(window, cx)
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn lines(&self) -> (u32, u32) {
        (self.first, self.last)
    }

    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn edited(&self, cx: &App) -> String {
        self.state.read(cx).value().to_string()
    }

    pub fn is_dirty(&self, cx: &App) -> bool {
        *self.state.read(cx).value() != *self.original
    }

    pub fn saved(&mut self, edited: String) {
        self.last = self.first + edited.matches('\n').count() as u32;
        self.original = edited;
    }

    pub fn element(&self, cx: &App) -> (AnyElement, f32, f32) {
        let theme = cx.theme();
        let rows = (self.last - self.first + 1) as usize;
        let height = (HEADER + rows.min(CODE_ROWS) as f32 * ROW + 0.75) * REM;
        let status = if self.locked {
            Some(("Unsaved in its tab", theme.warning))
        } else if self.is_dirty(cx) {
            Some(("● Ctrl+S to save", theme.warning))
        } else {
            None
        };
        let meta = format!("lines {}–{}", self.first + 1, self.last + 1);
        let panel = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(header(&self.file, meta, status, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .py_1()
                    .child(editor::view(&self.state)),
            );
        (panel.into_any_element(), CODE_WIDTH, height)
    }
}

// The lines where a name comes, inside the function that holds it or in the whole file.
pub struct Mentions {
    file: PathBuf,
    holder: Option<String>,
    found: Vec<Mention>,
}

impl Mentions {
    pub fn find(
        bench: &Workbench,
        file: &Path,
        symbol: &Symbol,
        parent: Option<&Symbol>,
    ) -> io::Result<Self> {
        let holder = parent
            .filter(|parent| matches!(parent.kind(), SymbolKind::Function | SymbolKind::Method));
        let lines = holder.map(|holder| (holder.first_line(), holder.last_line()));
        Ok(Self {
            file: file.to_path_buf(),
            holder: holder.map(|holder| holder.name().to_owned()),
            found: bench.mentions(file, symbol.name(), lines)?,
        })
    }

    pub fn element(&self, cx: &App) -> (AnyElement, f32, f32) {
        let count = self.found.len();
        let mut meta = match count {
            1 => "1 line".to_owned(),
            n => format!("{n} lines"),
        };
        if let Some(holder) = &self.holder {
            meta.push_str(&format!(" in {holder}"));
        }
        let shown = count.min(MENTION_ROWS);
        let more = count - shown;
        let rows = shown.max(1) + usize::from(more > 0);
        let height = (HEADER + rows as f32 * ROW + 1.) * REM;

        let mut body = v_flex().flex_1().min_h_0().py_2();
        if count == 0 {
            body = body.child(note("Not mentioned", cx));
        }
        for mention in &self.found[..shown] {
            body = body.child(line(mention, cx));
        }
        if more > 0 {
            body = body.child(note(&format!("{more} more"), cx));
        }
        let panel = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(header(&self.file, meta, None, cx))
            .child(body);
        (panel.into_any_element(), MENTIONS_WIDTH, height)
    }
}

fn header(
    file: &Path,
    meta: String,
    status: Option<(&str, gpui_kit::Hsla)>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let name: SharedString = file
        .file_name()
        .map_or_else(|| file.to_string_lossy(), |name| name.to_string_lossy())
        .into_owned()
        .into();
    h_flex()
        .flex_shrink_0()
        .h(rems(HEADER))
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .child(FileIcon::new(name.clone()).size(px(14.)))
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(name))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(meta),
        )
        .child(div().flex_1())
        .children(
            status.map(|(text, color)| div().text_xs().text_color(color).child(text.to_owned())),
        )
}

fn line(mention: &Mention, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    h_flex()
        .h(rems(ROW))
        .px_3()
        .gap_3()
        .font_family(theme.mono_font_family.clone())
        .text_xs()
        .whitespace_nowrap()
        .child(
            div()
                .w(rems(2.))
                .flex_shrink_0()
                .text_right()
                .text_color(theme.muted_foreground)
                .child((mention.line() + 1).to_string()),
        )
        .child(
            h_flex().min_w_0().overflow_hidden().children(
                pieces(mention.text(), mention.marks())
                    .into_iter()
                    .map(|(text, marked)| {
                        let piece = div().child(text);
                        if marked {
                            piece
                                .px(rems(0.0625))
                                .rounded_sm()
                                .bg(theme.blue.opacity(0.3))
                                .text_color(theme.foreground)
                        } else {
                            piece
                        }
                    }),
            ),
        )
}

fn note(text: &str, cx: &App) -> impl IntoElement {
    div()
        .h(rems(ROW))
        .px_3()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_owned())
}

// The text cut at the marks, each piece told whether it is marked.
fn pieces(text: &str, marks: &[Range<usize>]) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut at = 0;
    for mark in marks {
        let (start, end) = (mark.start.min(text.len()), mark.end.min(text.len()));
        if start < at || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        if start > at {
            out.push((text[at..start].to_owned(), false));
        }
        out.push((text[start..end].to_owned(), true));
        at = end;
    }
    if at < text.len() {
        out.push((text[at..].to_owned(), false));
    }
    out
}
