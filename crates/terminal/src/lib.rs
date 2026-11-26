// A terminal: a shell on a pseudoterminal, the screen it draws read by alacritty's emulator, and
// what the dev types sent back to it. Nothing here knows the UI: it takes bytes and a size, and
// hands out what to paint as plain cells.

mod color;
mod keys;

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as Raw, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty::{self, Options, Shell};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use futures::channel::mpsc;

pub use color::{BACKGROUND, CURSOR, FOREGROUND, Rgb, palette};
pub use keys::{Mods, encode};

// How much the terminal keeps of what scrolled off its top.
const HISTORY: usize = 10_000;
// How long a line typed in a shell waits for the shell to be ready for it, at most.
const STARTING: Duration = Duration::from_secs(5);

// The size of the screen, in cells and in pixels per cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl Size {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: cols.max(2),
            rows: rows.max(1),
            cell_width: 8,
            cell_height: 16,
        }
    }

    pub fn cells(mut self, width: u16, height: u16) -> Self {
        self.cell_width = width.max(1);
        self.cell_height = height.max(1);
        self
    }

    fn window(self) -> WindowSize {
        WindowSize {
            num_lines: self.rows,
            num_cols: self.cols,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

// What a terminal starts: a program in a folder, the dev's shell when none is named.
#[derive(Clone, Debug)]
pub struct Launch {
    pub cwd: PathBuf,
    pub program: Option<(String, Vec<String>)>,
    pub size: Size,
}

impl Launch {
    pub fn shell(cwd: impl Into<PathBuf>, size: Size) -> Self {
        Self {
            cwd: cwd.into(),
            program: None,
            size,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    // The screen changed.
    Wakeup,
    // The program named the terminal, or took its name back.
    Title(Option<String>),
    Bell,
    // The program asked to copy this to the clipboard.
    Copy(String),
    // The shell ended, with its exit code when it has one.
    Exited(Option<i32>),
}

// How a click picks text: a cell at a time, a word, or whole lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Cells,
    Word,
    Lines,
}

// The half of a cell a point falls in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Half {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Block,
    Underline,
    Beam,
    Hollow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub shape: Shape,
    // It stands on a character two cells wide.
    pub wide: bool,
}

// A cell as painted. `bg` is none where the terminal's own background shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub c: char,
    // Marks that combine with `c`, as accents.
    pub marks: Option<String>,
    pub fg: Rgb,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    // `c` takes this cell and the next.
    pub wide: bool,
    // The second half of a wide character, painted by the cell before.
    pub spacer: bool,
    pub selected: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            marks: None,
            fg: FOREGROUND,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            wide: false,
            spacer: false,
            selected: false,
        }
    }
}

// The screen as it is now, row by row.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub cols: usize,
    pub rows: usize,
    pub lines: Vec<Vec<Cell>>,
    pub cursor: Option<Cursor>,
    // Scrolled up into what went off the top, by so many lines.
    pub scrolled: usize,
}

impl Frame {
    // The text of a row, its trailing spaces cut.
    pub fn text(&self, row: usize) -> String {
        let line: String = self
            .lines
            .get(row)
            .into_iter()
            .flatten()
            .filter(|cell| !cell.spacer)
            .map(|cell| cell.c)
            .collect();
        line.trim_end().to_owned()
    }
}

// What the program asked of the keyboard and mouse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modes {
    // Arrows in application mode, as vim and less ask.
    pub app_cursor: bool,
    // On the screen of a full screen program, which keeps no history.
    pub alt_screen: bool,
    // The wheel sends arrows on that screen.
    pub alt_scroll: bool,
    // A paste goes in between markers, so the shell takes it as text and runs nothing.
    pub bracketed_paste: bool,
}

pub struct Terminal {
    term: Arc<FairMutex<Term<Proxy>>>,
    sender: EventLoopSender,
    size: Arc<Mutex<Size>>,
    exited: Arc<AtomicBool>,
    pid: u32,
    // The controlling side of the pseudoterminal, to ask what runs in front; valid until the
    // shell ends.
    master: RawFd,
    cwd: PathBuf,
}

impl Terminal {
    pub fn spawn(launch: Launch) -> io::Result<(Self, mpsc::UnboundedReceiver<Event>)> {
        let (events, receiver) = mpsc::unbounded();
        let size = Arc::new(Mutex::new(launch.size));
        let writer = Arc::new(Mutex::new(None));
        let exited = Arc::new(AtomicBool::new(false));
        let proxy = Proxy {
            events,
            writer: writer.clone(),
            size: size.clone(),
            exited: exited.clone(),
            code: Arc::new(Mutex::new(None)),
        };

        let options = Options {
            shell: launch
                .program
                .map(|(program, args)| Shell::new(program, args)),
            working_directory: Some(launch.cwd.clone()),
            drain_on_exit: true,
            env: env(),
        };
        let pty = tty::new(&options, launch.size.window(), 0)?;
        let pid = pty.child().id();
        let master = pty.file().as_raw_fd();

        let config = Config {
            scrolling_history: HISTORY,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(
            config,
            &launch.size,
            proxy.clone(),
        )));
        let looping = EventLoop::new(term.clone(), proxy, pty, true, false)?;
        let sender = looping.channel();
        *writer.lock().unwrap_or_else(|e| e.into_inner()) = Some(sender.clone());
        looping.spawn();

        let terminal = Self {
            term,
            sender,
            size,
            exited,
            pid,
            master,
            cwd: launch.cwd,
        };
        Ok((terminal, receiver))
    }

    // The folder it started in.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn size(&self) -> Size {
        *self.size.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    // Sends bytes as they are.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        if !self.has_exited() {
            let _ = self.sender.send(Msg::Input(bytes.into()));
        }
    }

    // Sends what the dev typed, bringing the screen back down to the prompt if it was scrolled.
    pub fn input(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        {
            let mut term = self.term.lock();
            if term.grid().display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
            term.selection = None;
        }
        self.write(bytes);
    }

    // Sends a paste, between the markers when the program asked for them, its line ends as the
    // Enter key sends them otherwise.
    pub fn paste(&self, text: &str) {
        let text = if self.modes().bracketed_paste {
            // A marker inside the paste would end it early.
            let clean = text.replace('\x1b', "");
            format!("\x1b[200~{clean}\x1b[201~")
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r")
        };
        self.input(text.into_bytes());
    }

    // Types a line in the shell and runs it; with `run` false it is left typed, for the dev to
    // finish. A shell still starting gets it once it edits its line, so the line shows once,
    // after the prompt, rather than echoed above it too.
    pub fn type_line(&self, line: &str, run: bool) {
        let mut bytes = line.as_bytes().to_vec();
        if run {
            bytes.push(b'\r');
        }
        if self.editing() {
            self.input(bytes);
            return;
        }
        let (sender, master, exited) = (self.sender.clone(), self.master, self.exited.clone());
        std::thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < STARTING && !exited.load(Ordering::Acquire) && !editing(master)
            {
                std::thread::sleep(Duration::from_millis(15));
            }
            if !exited.load(Ordering::Acquire) {
                let _ = sender.send(Msg::Input(bytes.into()));
            }
        });
    }

    // The shell edits its line itself, as zsh and bash do at their prompt.
    fn editing(&self) -> bool {
        !self.has_exited() && editing(self.master)
    }

    pub fn resize(&self, size: Size) {
        {
            let mut current = self.size.lock().unwrap_or_else(|e| e.into_inner());
            if *current == size {
                return;
            }
            *current = size;
        }
        self.term.lock().resize(size);
        let _ = self.sender.send(Msg::Resize(size.window()));
    }

    // Scrolls through what went off the top: up for positive lines, down for negative.
    pub fn scroll(&self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    pub fn modes(&self) -> Modes {
        let mode = *self.term.lock().mode();
        Modes {
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
            alt_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        }
    }

    // Starts picking text at a cell of the screen as shown.
    pub fn select(&self, row: usize, col: usize, half: Half, pick: Pick) {
        let mut term = self.term.lock();
        let point = point(&term, row, col);
        let kind = match pick {
            Pick::Cells => SelectionType::Simple,
            Pick::Word => SelectionType::Semantic,
            Pick::Lines => SelectionType::Lines,
        };
        term.selection = Some(Selection::new(kind, point, side(half)));
    }

    // Takes the text picked on to a cell.
    pub fn extend(&self, row: usize, col: usize, half: Half) {
        let mut term = self.term.lock();
        let point = point(&term, row, col);
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side(half));
        }
    }

    pub fn unselect(&self) {
        self.term.lock().selection = None;
    }

    // The text picked, if there is some.
    pub fn selected(&self) -> Option<String> {
        let term = self.term.lock();
        let range = term.selection.as_ref()?.to_range(&*term)?;
        // A click with no drag picks nothing.
        if range.start == range.end && term.selection.as_ref()?.ty == SelectionType::Simple {
            return None;
        }
        term.selection_to_string().filter(|text| !text.is_empty())
    }

    // The program running in front of the shell, by its name; none while the shell waits at its
    // prompt.
    pub fn running(&self) -> Option<String> {
        if self.has_exited() {
            return None;
        }
        // SAFETY: the descriptor stays open while the shell runs, which was just checked.
        let group = unsafe { libc::tcgetpgrp(self.master) };
        if group <= 0 || group as u32 == self.pid {
            return None;
        }
        Some(program(group).unwrap_or_else(|| "a process".to_owned()))
    }

    // The screen as it is now.
    pub fn frame(&self) -> Frame {
        let term = self.term.lock();
        let content = term.renderable_content();
        let (cols, rows) = (term.columns(), term.screen_lines());
        let offset = content.display_offset;
        let colors = content.colors;
        let mut lines: Vec<Vec<Cell>> = (0..rows).map(|_| Vec::with_capacity(cols)).collect();
        for indexed in content.display_iter {
            let row = indexed.point.line.0 + offset as i32;
            let Some(line) = usize::try_from(row).ok().and_then(|row| lines.get_mut(row)) else {
                continue;
            };
            let cell = &*indexed;
            let flags = cell.flags;
            let mut fg = if flags.contains(Flags::DIM) {
                color::dim(cell.fg, colors)
            } else {
                color::resolve(cell.fg, colors)
            };
            let default_bg = cell.bg == Color::Named(NamedColor::Background)
                && colors[NamedColor::Background].is_none();
            let mut bg = (!default_bg).then(|| color::resolve(cell.bg, colors));
            if flags.contains(Flags::INVERSE) {
                let back = bg.unwrap_or(BACKGROUND);
                bg = Some(fg);
                fg = back;
            }
            if flags.contains(Flags::HIDDEN) {
                fg = bg.unwrap_or(BACKGROUND);
            }
            let selected = content
                .selection
                .is_some_and(|range| range.contains(indexed.point));
            line.push(Cell {
                c: cell.c,
                marks: cell.zerowidth().map(|marks| marks.iter().collect()),
                fg,
                bg,
                bold: flags.contains(Flags::BOLD),
                italic: flags.contains(Flags::ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                strike: flags.contains(Flags::STRIKEOUT),
                wide: flags.contains(Flags::WIDE_CHAR),
                spacer: flags.contains(Flags::WIDE_CHAR_SPACER),
                selected,
            });
        }
        for line in &mut lines {
            line.resize_with(cols, Cell::default);
        }

        let cursor = content.cursor;
        let row = cursor.point.line.0 + offset as i32;
        let shape = match cursor.shape {
            CursorShape::Block => Some(Shape::Block),
            CursorShape::Underline => Some(Shape::Underline),
            CursorShape::Beam => Some(Shape::Beam),
            CursorShape::HollowBlock => Some(Shape::Hollow),
            CursorShape::Hidden => None,
        };
        let cursor = shape
            .zip(usize::try_from(row).ok().filter(|row| *row < rows))
            .map(|(shape, row)| {
                let col = cursor.point.column.0.min(cols.saturating_sub(1));
                Cursor {
                    row,
                    col,
                    shape,
                    wide: lines[row].get(col).is_some_and(|cell| cell.wide),
                }
            });

        Frame {
            cols,
            rows,
            lines,
            cursor,
            scrolled: offset,
        }
    }
}

impl Drop for Terminal {
    // The shell is hung up on, and what it runs with it.
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

// The name of a process as the dev would call it: the script a runtime runs (`pnpm` for
// `node /usr/bin/pnpm`), else the program itself.
fn program(pid: i32) -> Option<String> {
    const RUNTIMES: &[&str] = &[
        "node", "bun", "deno", "python", "python3", "ruby", "perl", "php", "java", "sh", "bash",
        "zsh", "env",
    ];
    let line = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let mut args = line.split(|b| *b == 0).filter(|arg| !arg.is_empty());
    let name = |arg: &[u8]| {
        Path::new(std::ffi::OsStr::from_bytes(arg))
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    };
    let first = name(args.next()?)?;
    if RUNTIMES.contains(&first.as_str())
        && let Some(script) = args.next().filter(|arg| !arg.starts_with(b"-"))
    {
        return name(script);
    }
    Some(first)
}

// Whether the program on the terminal reads its keys one at a time, editing the line itself,
// rather than a line at a time as the terminal echoes it.
fn editing(master: RawFd) -> bool {
    // SAFETY: termios is plain data, filled in by the call when it succeeds.
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: the descriptor is open while the shell runs, as the callers check.
    let read = unsafe { libc::tcgetattr(master, &mut termios) };
    read == 0 && termios.c_lflag & libc::ICANON == 0
}

fn point(term: &Term<Proxy>, row: usize, col: usize) -> Point {
    let offset = term.grid().display_offset() as i32;
    let row = row.min(term.screen_lines().saturating_sub(1)) as i32;
    let col = col.min(term.columns().saturating_sub(1));
    Point::new(Line(row - offset), Column(col))
}

fn side(half: Half) -> Side {
    match half {
        Half::Left => Side::Left,
        Half::Right => Side::Right,
    }
}

// What the programs in the terminal are told about it.
fn env() -> HashMap<String, String> {
    let term = if terminfo("alacritty") {
        "alacritty"
    } else {
        "xterm-256color"
    };
    HashMap::from([
        ("TERM".to_owned(), term.to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ("TERM_PROGRAM".to_owned(), "corvo".to_owned()),
        // Not alacritty's window, so nothing should talk to it as one.
        ("ALACRITTY_WINDOW_ID".to_owned(), String::new()),
        ("WINDOWID".to_owned(), String::new()),
    ])
}

// Whether the system describes a terminal by that name.
fn terminfo(name: &str) -> bool {
    let first = &name[..1];
    let hex = format!("{:x}", name.as_bytes()[0]);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::home_dir() {
        dirs.push(home.join(".terminfo"));
    }
    dirs.extend(
        ["/etc/terminfo", "/lib/terminfo", "/usr/share/terminfo"]
            .iter()
            .map(PathBuf::from),
    );
    dirs.iter()
        .any(|dir| dir.join(first).join(name).exists() || dir.join(&hex).join(name).exists())
}

#[derive(Clone)]
struct Proxy {
    events: mpsc::UnboundedSender<Event>,
    // What answers the program, once the loop is up.
    writer: Arc<Mutex<Option<EventLoopSender>>>,
    size: Arc<Mutex<Size>>,
    exited: Arc<AtomicBool>,
    code: Arc<Mutex<Option<i32>>>,
}

impl Proxy {
    fn answer(&self, text: String) {
        if let Some(writer) = self
            .writer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = writer.send(Msg::Input(text.into_bytes().into()));
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.unbounded_send(event);
    }
}

impl EventListener for Proxy {
    fn send_event(&self, event: Raw) {
        match event {
            Raw::Wakeup | Raw::MouseCursorDirty | Raw::CursorBlinkingChange => {
                self.emit(Event::Wakeup)
            }
            Raw::Title(title) => self.emit(Event::Title(Some(title))),
            Raw::ResetTitle => self.emit(Event::Title(None)),
            Raw::Bell => self.emit(Event::Bell),
            Raw::ClipboardStore(_, text) => self.emit(Event::Copy(text)),
            // A program reading the clipboard could take what it should not see.
            Raw::ClipboardLoad(..) => {}
            Raw::ColorRequest(index, format) => self.answer(format(palette(index).into())),
            Raw::PtyWrite(text) => self.answer(text),
            Raw::TextAreaSizeRequest(format) => {
                let size = *self.size.lock().unwrap_or_else(|e| e.into_inner());
                self.answer(format(size.window()));
            }
            Raw::ChildExit(status) => {
                *self.code.lock().unwrap_or_else(|e| e.into_inner()) = status.code();
            }
            Raw::Exit => {
                self.exited.store(true, Ordering::Release);
                let code = *self.code.lock().unwrap_or_else(|e| e.into_inner());
                self.emit(Event::Exited(code));
            }
        }
    }
}
