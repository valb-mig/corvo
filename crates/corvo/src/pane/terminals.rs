use std::cell::Cell as Slot;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use corvo_domain::Script;
use corvo_terminal::{Event as Note, Launch, Mods, Pick, Size, Terminal, encode};
use corvo_ui::drawer::{self, Drawer};
use corvo_ui::{field, notice};
use futures::StreamExt as _;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, CursorStyle, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyBinding, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Render, ScrollEvent,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, WeakEntity, Window, actions,
    div, prelude::FluentBuilder as _, px,
};

use crate::pane::dock::Project;
use crate::pane::screen::{self, Inset, Layout};

// The keys the terminal takes before the window does: Tab would move the focus, ctrl-c copy.
actions!(terminal, [SendTab, SendBackTab, Interrupt, Paste]);

const CONTEXT: &str = "Terminal";

// Room left under the screen for the agent bar, which floats over the foot of every window.
const AGENT_BAR: f32 = 88.;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", SendTab, Some(CONTEXT)),
        KeyBinding::new("shift-tab", SendBackTab, Some(CONTEXT)),
        KeyBinding::new("ctrl-c", Interrupt, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-v", Paste, Some(CONTEXT)),
        KeyBinding::new("shift-insert", Paste, Some(CONTEXT)),
    ]);
}

struct Tab {
    id: u64,
    name: String,
    // What the program in it calls it, when it does.
    title: Option<String>,
    session: Arc<Terminal>,
    _events: Task<()>,
}

// The terminals of the workspace in a window over the home: the screen of the one picked, and the
// list of them in a panel floating on its right, opened and closed with ctrl-b or the button of
// the window's bar, with a button for a new one. It takes turns with the code, the chats and git
// over the home. A terminal goes when its shell ends, or when the dev closes it.
pub struct Terminals {
    project: Project,
    tabs: Vec<Tab>,
    shown: Option<u64>,
    next: u64,
    panel: bool,
    focus: FocusHandle,
    layout: Rc<Slot<Layout>>,
    renaming: Option<(u64, Entity<field::State>, Subscription)>,
    // A drag picking text is under way.
    selecting: bool,
    // What the wheel turned short of a line.
    wheel: f32,
}

impl Terminals {
    pub fn new(project: Project, cx: &mut Context<Self>) -> Self {
        Self {
            project,
            tabs: Vec::new(),
            shown: None,
            next: 1,
            panel: true,
            focus: cx.focus_handle(),
            layout: Rc::default(),
            renaming: None,
            selecting: false,
            wheel: 0.,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    pub fn has_panel(&self) -> bool {
        self.panel
    }

    // Opens the list of terminals, or closes it; tells whether it is open now.
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) -> bool {
        self.panel = !self.panel;
        if !self.panel {
            self.renamed(cx);
        }
        cx.notify();
        self.panel
    }

    // How many terminals there are, for the window's bar.
    pub fn subtitle(&self) -> String {
        match self.tabs.len() {
            0 => "No terminal".to_owned(),
            1 => "1 terminal".to_owned(),
            n => format!("{n} terminals"),
        }
    }

    // The terminals, each with its id, name and what tells it apart.
    pub fn list(&self) -> Vec<(u64, String, String)> {
        self.tabs
            .iter()
            .map(|tab| (tab.id, tab.name.clone(), detail(tab, tab.session.running())))
            .collect()
    }

    // What runs in front of a shell, by terminal: the name of the terminal and of the program.
    pub fn running(&self) -> Vec<(String, String)> {
        self.tabs
            .iter()
            .filter_map(|tab| Some((tab.name.clone(), tab.session.running()?)))
            .collect()
    }

    // Opened with none, a terminal starts in the project in front.
    pub fn ensure(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            self.open(None, None, window, cx);
        }
    }

    pub fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(None, None, window, cx);
        self.focus(window, cx);
    }

    // A terminal of its own in a folder, as a folder's card on the home asks.
    pub fn open_in(&mut self, dir: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.open(Some(dir.to_path_buf()), None, window, cx);
        self.focus(window, cx);
    }

    // Runs a script in a terminal of its own, named after it, in the project it belongs to. One
    // that needs arguments is left typed for the dev to finish.
    pub fn run(
        &mut self,
        script: &Script,
        root: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = script.name().to_owned();
        if let Some(session) = self.open(Some(root.to_path_buf()), Some(name), window, cx) {
            session.type_line(script.command(), !script.asks());
        }
        self.focus(window, cx);
    }

    pub fn show(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.iter().any(|tab| tab.id == id) {
            self.shown = Some(id);
            self.selecting = false;
            self.focus(window, cx);
            cx.notify();
        }
    }

    // Starts the dev's shell in a folder, the project in front by default, and shows it.
    fn open(
        &mut self,
        cwd: Option<PathBuf>,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<Terminal>> {
        let cwd = cwd.unwrap_or_else(|| (self.project)(cx));
        let size = self.size();
        let (session, mut events) = match Terminal::spawn(Launch::shell(cwd, size)) {
            Ok(started) => started,
            Err(err) => {
                notice::error(format!("Terminal: {err}"), window, cx);
                return None;
            }
        };
        let session = Arc::new(session);
        let id = self.next;
        self.next += 1;
        let name = self.unique(name.unwrap_or_else(shell));
        let task = cx.spawn(async move |this, cx| {
            while let Some(note) = events.next().await {
                // What piled up while the screen was drawn goes in one go.
                let mut notes = vec![note];
                while let Ok(note) = events.try_recv() {
                    notes.push(note);
                }
                if this
                    .update(cx, |this, cx| this.heard(id, notes, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.tabs.push(Tab {
            id,
            name,
            title: None,
            session: session.clone(),
            _events: task,
        });
        self.shown = Some(id);
        cx.notify();
        Some(session)
    }

    // The size the screen was last painted at, for a new terminal to start at.
    fn size(&self) -> Size {
        self.tabs
            .iter()
            .find(|tab| Some(tab.id) == self.shown)
            .map(|tab| tab.session.size())
            .unwrap_or_else(|| Size::new(80, 24))
    }

    // A name no other terminal has: `zsh`, then `zsh 2`.
    fn unique(&self, name: String) -> String {
        let taken = |candidate: &str| self.tabs.iter().any(|tab| tab.name == candidate);
        if !taken(&name) {
            return name;
        }
        (2..)
            .map(|n| format!("{name} {n}"))
            .find(|candidate| !taken(candidate))
            .unwrap_or(name)
    }

    fn heard(&mut self, id: u64, notes: Vec<Note>, cx: &mut Context<Self>) {
        for note in notes {
            match note {
                Note::Wakeup | Note::Bell => {}
                Note::Title(title) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                        tab.title = title;
                    }
                }
                Note::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                Note::Exited(_) => {
                    self.close(id, cx);
                    return;
                }
            }
        }
        cx.notify();
    }

    // Closes a terminal, hanging up on its shell; the one next to it shows in its place.
    pub fn close(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(at) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        self.tabs.remove(at);
        if self
            .renaming
            .as_ref()
            .is_some_and(|(which, _, _)| *which == id)
        {
            self.renaming = None;
        }
        if self.shown == Some(id) {
            self.shown = self
                .tabs
                .get(at)
                .or_else(|| self.tabs.last())
                .map(|tab| tab.id);
            self.selecting = false;
        }
        cx.notify();
    }

    fn rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.name.clone())
        else {
            return;
        };
        let state = cx.new(|cx| field::state("Terminal name", window, cx));
        state.update(cx, |state, cx| {
            state.set_value(name, window, cx);
            state.select_all(window, cx);
            state.focus(window, cx);
        });
        let sub = cx.subscribe_in(&state, window, |this, _, event, window, cx| {
            if matches!(event, field::Event::PressEnter { .. } | field::Event::Blur) {
                this.renamed(cx);
                this.focus(window, cx);
            }
        });
        self.renaming = Some((id, state, sub));
        cx.notify();
    }

    // The name typed in goes to the terminal; a blank one leaves it as it was.
    fn renamed(&mut self, cx: &mut Context<Self>) {
        let Some((id, state, _)) = self.renaming.take() else {
            return;
        };
        let name = state.read(cx).value().trim().to_owned();
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id)
            && !name.is_empty()
        {
            tab.name = name;
        }
        cx.notify();
    }

    fn session(&self) -> Option<Arc<Terminal>> {
        self.tabs
            .iter()
            .find(|tab| Some(tab.id) == self.shown)
            .map(|tab| tab.session.clone())
    }

    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform {
            return;
        }
        let mods = Mods {
            ctrl: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
        };
        let app_cursor = session.modes().app_cursor;
        if let Some(bytes) = encode(
            &keystroke.key,
            keystroke.key_char.as_deref(),
            mods,
            app_cursor,
        ) {
            session.input(bytes);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn send(&mut self, bytes: &'static [u8], cx: &mut Context<Self>) {
        if let Some(session) = self.session() {
            session.input(bytes);
            cx.notify();
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let text = cx.read_from_clipboard().and_then(|item| item.text());
        if let (Some(session), Some(text)) = (self.session(), text) {
            session.paste(&text);
            cx.notify();
        }
    }

    // A press puts the keys in the terminal and starts picking text: a cell at a time, a word on
    // a double click, whole lines on a triple one.
    fn press(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus(window, cx);
        let (Some(session), Some((row, col, half))) =
            (self.session(), self.layout.get().cell_at(event.position))
        else {
            return;
        };
        let pick = match event.click_count {
            2 => Pick::Word,
            n if n >= 3 => Pick::Lines,
            _ => Pick::Cells,
        };
        session.select(row, col, half, pick);
        self.selecting = true;
        cx.notify();
    }

    fn drag(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if let (Some(session), Some((row, col, half))) =
            (self.session(), self.layout.get().cell_at(event.position))
        {
            session.extend(row, col, half);
            cx.notify();
        }
    }

    // What was picked is copied as the mouse lets go; a click that picked nothing lets go of
    // what was picked before.
    fn release(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.selecting) {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        match session.selected() {
            Some(text) => {
                cx.write_to_primary(ClipboardItem::new_string(text.clone()));
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            None => session.unselect(),
        }
        cx.notify();
    }

    // The wheel goes through what scrolled off the top; on the screen of a full screen program,
    // which has none, it sends the arrows when the program asks for them.
    fn wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let line = self.layout.get().line_height();
        self.wheel += f32::from(event.delta.pixel_delta(px(line)).y);
        let lines = (self.wheel / line).trunc() as i32;
        if lines == 0 {
            return;
        }
        self.wheel -= lines as f32 * line;
        let modes = session.modes();
        if modes.alt_screen {
            let key = if lines > 0 { "up" } else { "down" };
            if let Some(arrow) = modes
                .alt_scroll
                .then(|| encode(key, None, Mods::default(), modes.app_cursor))
                .flatten()
            {
                session.write(arrow.repeat(lines.unsigned_abs() as usize));
            }
        } else {
            session.scroll(lines);
        }
        cx.notify();
    }

    fn panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let view = cx.entity().downgrade();
        let adder = view.clone();
        let header = h_flex()
            .px_3()
            .pt_2()
            .pb_1()
            .gap_1()
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("TERMINALS"),
            )
            .child(
                Button::new("new-terminal")
                    .ghost()
                    .xsmall()
                    .icon(Glyph::Plus)
                    .tooltip("New terminal")
                    .on_click(move |_, window, cx| {
                        adder
                            .update(cx, |this, cx| this.new_terminal(window, cx))
                            .ok();
                    }),
            );
        let rows: Vec<AnyElement> = self
            .tabs
            .iter()
            .map(|tab| self.row(tab, &view, cx))
            .collect();
        Drawer::new("terminal-panel").child(header).child(
            div()
                .id("terminal-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_2()
                .pb_3()
                .children(rows),
        )
    }

    fn row(&self, tab: &Tab, view: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let id = tab.id;
        let on = Some(id) == self.shown;
        let running = tab.session.running();
        let name: AnyElement = match self.renaming.as_ref().filter(|(which, _, _)| *which == id) {
            Some((_, state, _)) => Input::new(state).xsmall().into_any_element(),
            None => div()
                .min_w_0()
                .truncate()
                .text_sm()
                .child(tab.name.clone())
                .into_any_element(),
        };
        let second = h_flex()
            .gap_1()
            .min_w_0()
            .text_xs()
            .text_color(theme.muted_foreground)
            .children(running.is_some().then(|| {
                div()
                    .flex_shrink_0()
                    .size(px(6.))
                    .rounded_full()
                    .bg(theme.success)
            }))
            .child(div().min_w_0().truncate().child(detail(tab, running)));

        let menu = {
            let view = view.clone();
            move |menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                let (rename, close) = (view.clone(), view.clone());
                menu.item(PopupMenuItem::new("Rename").icon(Glyph::Pencil).on_click(
                    move |_, window, cx| {
                        rename
                            .update(cx, |this, cx| this.rename(id, window, cx))
                            .ok();
                    },
                ))
                .separator()
                .item(
                    PopupMenuItem::new("Close")
                        .icon(Glyph::Close)
                        .on_click(move |_, _, cx| {
                            close.update(cx, |this, cx| this.close(id, cx)).ok();
                        }),
                )
            }
        };
        let (pick, closer) = (view.clone(), view.clone());
        h_flex()
            .id(("terminal", id))
            .group("terminal-row")
            .gap_2()
            .px_2()
            .py(px(7.))
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|row| row.bg(theme.muted))
            .when(on, |row| row.bg(theme.secondary))
            .on_click(move |_, window, cx| {
                pick.update(cx, |this, cx| this.show(id, window, cx)).ok();
            })
            .context_menu(move |popup, window, cx| menu(popup, window, cx))
            .child(
                Icon::new(Glyph::Terminal)
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(name)
                    .child(second),
            )
            .child(
                div()
                    .invisible()
                    .group_hover("terminal-row", |close| close.visible())
                    .child(
                        Button::new(("close-terminal", id))
                            .ghost()
                            .xsmall()
                            .icon(Glyph::Close)
                            .tooltip("Close")
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                closer.update(cx, |this, cx| this.close(id, cx)).ok();
                            }),
                    ),
            )
            .into_any_element()
    }

    fn empty(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let view = cx.entity().downgrade();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child("No terminal open")
            .child(
                Button::new("open-terminal")
                    .outline()
                    .small()
                    .icon(Glyph::Plus)
                    .label("New terminal")
                    .on_click(move |_, window, cx| {
                        view.update(cx, |this, cx| this.new_terminal(window, cx))
                            .ok();
                    }),
            )
    }
}

// What runs in a terminal, or the folder it is in.
fn detail(tab: &Tab, running: Option<String>) -> String {
    if let Some(program) = running {
        return format!("Running {program}");
    }
    let folder = tab
        .session
        .cwd()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| tab.session.cwd().display().to_string());
    match &tab.title {
        Some(title) if !title.is_empty() => format!("{folder} · {title}"),
        _ => folder,
    }
}

// The name of the dev's shell, as a terminal is called when it runs nothing else.
fn shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|shell| {
            Path::new(&shell)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "shell".to_owned())
}

impl Render for Terminals {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        let right = if self.panel {
            f32::from(drawer::room())
        } else {
            14.
        };
        let inset = Inset {
            left: 14.,
            top: 10.,
            right,
            bottom: AGENT_BAR,
        };
        let body = match self.session() {
            Some(session) => {
                screen::screen(session, self.layout.clone(), focused, inset).into_any_element()
            }
            None => self.empty(cx).into_any_element(),
        };
        let grid = div()
            .id("terminal-screen")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::key))
            .on_action(cx.listener(|this, _: &SendTab, _, cx| this.send(b"\t", cx)))
            .on_action(cx.listener(|this, _: &SendBackTab, _, cx| this.send(b"\x1b[Z", cx)))
            .on_action(cx.listener(|this, _: &Interrupt, _, cx| this.send(b"\x03", cx)))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::press))
            .on_mouse_move(cx.listener(Self::drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::release))
            .on_scroll(cx.listener(Self::wheel))
            .child(body);
        let panel = self.panel.then(|| self.panel(cx));
        div().relative().size_full().child(grid).children(panel)
    }
}
