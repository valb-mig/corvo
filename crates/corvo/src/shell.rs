use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use corvo_domain::server::Registry;
use corvo_domain::{
    Chats, Script, Share, Status, Uuid, Workbench, Workspaces, agents, languages, scripts,
};
use corvo_fs::{JsonChats, JsonOptions, LocalFs};
use corvo_git::GitCli;
use corvo_lsp::{Hub, Note};
use corvo_ui::drawer::Drawer;
use corvo_ui::keys::{
    Dismiss, OpenPalette, Save, ShowChat, ShowCode, ShowGit, ShowTerminal, TogglePanel,
};
use corvo_ui::language_icon::LanguageIcon;
use corvo_ui::statusbar::{Action, Popup, StatusBar};
use corvo_ui::window_bar::{PaletteField, WindowBar};
use corvo_ui::{motion, notice};
use futures::StreamExt as _;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnimationExt as _, AnyElement, AnyView, App, AppContext as _, Context, ElementId, Entity,
    FocusHandle, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    PathPromptOptions, Render, StyleRefinement, Styled as _, Subscription, Window, div, px,
};
use lsp_types::Position;

use crate::agents::{self, Agents};
use crate::palette::{self, Listed, Offer, Search};
use crate::pane::chats::{self, ChatWindow};
use crate::pane::code::{self, Code};
use crate::pane::dock::{self, Dock, Project};
use crate::pane::files::{self, Files};
use crate::pane::git::{self, GitWindow};
use crate::pane::hidden::Hidden;
use crate::pane::overview::{self, Overview};
use crate::pane::scripts::{self, Scripts};
use crate::pane::stack::Stack;
use crate::pane::terminals::Terminals;
use crate::repos::{self, Repos};

// How often git is read again while the window is in front, for what the agent or a terminal
// changed.
const POLL: Duration = Duration::from_secs(3);

// The room above and below a window over the home, and above its palette, under the bar.
const GAP: f32 = 32.;
const UNDER_BAR: f32 = 40.;
// How far a window rises into place as it opens, and its palette drops.
const RISE: f32 = 12.;
const DROP: f32 = 8.;

pub struct Shell {
    focus: FocusHandle,
    spaces: Entity<Workspaces>,
    id: Uuid,
    bench: Entity<Workbench>,
    hub: Entity<Hub>,
    // The open files, over the home while shown, with the file tree beside them, the active
    // file's outline nested under its row.
    code: Entity<Code>,
    files: Entity<Files>,
    home: Entity<Overview>,
    hidden: Entity<Hidden>,
    hiding: bool,
    // The languages of each project looked over, and the panel listing those of the project open
    // in the home.
    tongues: HashMap<PathBuf, Vec<Share>>,
    stack: Entity<Stack>,
    stacking: bool,
    // The chats of the workspace: the agent bar always over the content, and the chat window,
    // which takes turns with the code over the home.
    agents: Entity<Agents>,
    dock: Entity<Dock>,
    chats: Entity<ChatWindow>,
    chatting: bool,
    // The repositories of the linked projects, and the git window, which also takes turns with
    // the code over the home.
    repos: Entity<Repos>,
    git: Entity<GitWindow>,
    gitting: bool,
    // The terminals of the workspace, in a window that takes turns with the others over the home,
    // and the scripts of the project in front, which the status bar runs in them.
    terminals: Entity<Terminals>,
    terming: bool,
    scripts: Entity<Scripts>,
    scripting: bool,
    // The palette of the window in front, what had the keys before it opened, and the files of
    // each project for the code's palette, walked the first time it is asked.
    palette: Entity<CommandState>,
    paletting: bool,
    before: Option<FocusHandle>,
    index: HashMap<PathBuf, Rc<Vec<Listed>>>,
    search: Search,
    // The panel floating on the right of the code, with the file tree: opened and closed with
    // ctrl-b or the button of the window's bar.
    panel: bool,
    _subs: Vec<Subscription>,
}

impl Shell {
    pub fn new(
        spaces: Entity<Workspaces>,
        id: Uuid,
        mut bench: Workbench,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The home comes back as it was left.
        if let Some(space) = spaces.read(cx).get(id) {
            bench.restore(space.projects(), space.home());
        }
        let root = bench.atlas().root().map(Path::to_path_buf);
        let registry = Registry::default();
        let (hub, mut notes) = Hub::new(registry.clone());
        let hub = cx.new(|_| hub);
        let bench = cx.new(|_| bench);
        let repos = cx.new(|_| Repos::new(Arc::new(GitCli)));
        let home = cx.new(|cx| Overview::new(bench.clone(), repos.clone(), cx));
        let code =
            cx.new(|cx| Code::new(bench.clone(), hub.clone(), home.clone(), repos.clone(), cx));
        let files = cx.new(|cx| Files::new(bench.clone(), repos.clone(), window, cx));
        let git = cx.new(|cx| GitWindow::new(repos.clone(), cx));
        let hidden = cx.new(|_| Hidden::new(bench.clone(), home.clone()));
        let stack = cx.new(|_| Stack::default());

        let (chat_list, trouble) = load_chats(id);
        let (agent_hub, mut agent_events) = corvo_agent::Hub::new(agents());
        let store = Box::new(option_store());
        let agents = cx.new(|_| Agents::new(agent_hub, chat_list, store));
        let project = focused(bench.clone(), spaces.clone(), id);
        let name = spaces
            .read(cx)
            .get(id)
            .map(|space| space.name().to_owned())
            .unwrap_or_default();
        // The agent is started while the dev looks around, so the first chat only waits on its
        // session.
        let start = project(cx);
        agents.read(cx).warm(&start);
        let dock = cx.new(|cx| Dock::new(agents.clone(), project.clone(), window, cx));
        let chats = cx.new(|cx| ChatWindow::new(agents.clone(), project.clone(), name.into(), cx));
        let terminals = cx.new(|cx| Terminals::new(project, cx));
        let scripts = cx.new(|_| Scripts::default());
        let palette = cx.new(|cx| CommandState::new(window, cx));
        if let Some(trouble) = trouble {
            notice::error(trouble, window, cx);
        }
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = agent_events.next().await {
                // A step done or a reply that ended may have changed files.
                let touched = match &event {
                    corvo_agent::Event::Ended { .. } => true,
                    corvo_agent::Event::Step { patch, .. } => patch.status == Some(Status::Done),
                    _ => false,
                };
                let alive = this.update(cx, |shell, cx| {
                    shell
                        .agents
                        .update(cx, |agents, cx| agents.apply(event, cx));
                    if touched {
                        shell.repos.update(cx, |repos, cx| repos.refresh(cx));
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        let subs = vec![
            cx.subscribe_in(&agents, window, Self::on_agents),
            cx.subscribe_in(&dock, window, Self::on_dock),
            cx.subscribe_in(&files, window, Self::on_files),
            cx.subscribe_in(&code, window, Self::on_code),
            cx.subscribe_in(&home, window, Self::on_home),
            cx.subscribe_in(&git, window, Self::on_git),
            cx.subscribe_in(&chats, window, Self::on_chats),
            cx.subscribe_in(&scripts, window, Self::on_scripts),
            // The terminal window's bar counts its terminals, and so does the status bar.
            cx.observe(&terminals, |_, _, cx| cx.notify()),
            // The chat window's bar counts its chats.
            cx.observe(&chats, |_, _, cx| cx.notify()),
            cx.subscribe(&repos, Self::on_moved),
            // The status bar shows the branch, and the home colors what changed.
            cx.observe(&repos, |this, _, cx| {
                this.home.update(cx, |_, cx| cx.notify());
                cx.notify();
            }),
            // Back in front, git may have moved on.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.repos.update(cx, |repos, cx| repos.refresh(cx));
                }
            }),
            // The status bar counts what the home hides.
            cx.observe(&home, |_, _, cx| cx.notify()),
            // The code comes over the home, or goes; it takes the place of the chat and git
            // windows.
            cx.observe(&code, |this, code, cx| {
                if code.read(cx).is_shown() {
                    if this.chatting {
                        this.chatting = false;
                        this.dock
                            .update(cx, |dock, cx| dock.set_chatting(false, cx));
                    }
                    this.gitting = false;
                    this.terming = false;
                }
                cx.notify();
            }),
        ];

        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let alive = this.update_in(cx, |shell, window, cx| {
                    if !window.is_window_active() {
                        return;
                    }
                    shell.repos.update(cx, |repos, cx| repos.refresh(cx));
                    // The patch shown may have changed with the file, its change still the same.
                    if shell.gitting {
                        shell.git.update(cx, |git, cx| git.reread(cx));
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.spawn_in(window, async move |this, cx| {
            while let Some(note) = notes.next().await {
                let alive = this.update_in(cx, |shell, window, cx| shell.on_note(note, window, cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        // The keys start on the home, so Space held moves its graph; the window's own keys still
        // reach the window around it.
        let focus = cx.focus_handle();
        home.update(cx, |home, cx| home.focus(window, cx));

        let mut shell = Self {
            focus,
            spaces,
            id,
            bench,
            hub,
            code,
            files,
            home,
            hidden,
            hiding: false,
            tongues: HashMap::new(),
            stack,
            stacking: false,
            agents,
            dock,
            chats,
            chatting: false,
            repos,
            git,
            gitting: false,
            terminals,
            terming: false,
            scripts,
            scripting: false,
            palette,
            paletting: false,
            before: None,
            index: HashMap::new(),
            search: Search::default(),
            panel: false,
            _subs: subs,
        };
        let first = root.clone().or_else(|| shell.linked(cx).into_iter().next());
        shell.show(first, window, cx);
        if root.is_some() {
            shell.weigh(cx);
        }
        shell
    }

    pub fn bench(&self) -> &Entity<Workbench> {
        &self.bench
    }

    // What runs in front of a shell, by terminal: the name of the terminal and of the program.
    pub fn running(&self, cx: &App) -> Vec<(String, String)> {
        self.terminals.read(cx).running()
    }

    fn linked(&self, cx: &Context<Self>) -> Vec<PathBuf> {
        self.spaces
            .read(cx)
            .get(self.id)
            .map(|space| space.projects().to_vec())
            .unwrap_or_default()
    }

    fn show(&mut self, project: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self
            .bench
            .update(cx, |bench, _| bench.show(project.as_deref()));
        if let (Err(err), Some(project)) = (shown, &project) {
            notice::error(format!("{}: {err}", project.display()), window, cx);
        }
        self.rescope(cx);
        self.files.update(cx, |files, cx| files.refresh(cx));
        self.chart(window, cx);
        self.read_scripts(cx);
    }

    // Reads the scripts of the project in front off the main thread, showing those read before
    // meanwhile.
    fn read_scripts(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.bench.read(cx).project().map(Path::to_path_buf) else {
            self.scripts.update(cx, |scripts, cx| scripts.clear(cx));
            return;
        };
        let task = cx.background_executor().spawn({
            let root = root.clone();
            async move { scripts(&LocalFs, &root) }
        });
        cx.spawn(async move |this, cx| {
            let found = task.await;
            this.update(cx, |this, cx| {
                // The project in front may have changed while they were read.
                if this.bench.read(cx).project() == Some(root.as_path()) {
                    this.scripts
                        .update(cx, |scripts, cx| scripts.set(root, found, cx));
                }
            })
            .ok();
        })
        .detach();
    }

    // Counts the languages of the project open in the home off the main thread, showing what was
    // counted before meanwhile.
    fn weigh(&mut self, cx: &mut Context<Self>) {
        self.restack(cx);
        let Some(root) = self.bench.read(cx).atlas().root().map(Path::to_path_buf) else {
            return;
        };
        let task = cx.background_executor().spawn({
            let root = root.clone();
            async move { languages(&LocalFs, &root) }
        });
        cx.spawn(async move |this, cx| {
            let shares = task.await;
            this.update(cx, |this, cx| {
                this.tongues.insert(root, shares);
                this.restack(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn restack(&mut self, cx: &mut Context<Self>) {
        let shares = self
            .bench
            .read(cx)
            .atlas()
            .root()
            .and_then(|root| self.tongues.get(root))
            .cloned()
            .unwrap_or_default();
        self.stack.update(cx, |stack, cx| stack.set(shares, cx));
    }

    fn chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let linked = self.linked(cx);
        self.home
            .update(cx, |home, cx| home.chart(&linked, window, cx));
    }

    fn rescope(&mut self, cx: &mut Context<Self>) {
        let linked = self.linked(cx);
        self.repos
            .update(cx, |repos, cx| repos.track(linked.clone(), cx));
        let active = self.bench.read(cx).project().map(Path::to_path_buf);
        self.hub.update(cx, |hub, _| hub.scope(linked, active));
    }

    // Puts the code away, back to the home.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.paletting = false;
        if self.code.read(cx).is_shown() {
            self.code.update(cx, |code, cx| code.dismiss(window, cx));
        }
    }

    // Brings the code over the home, with the tabs left open, or puts it away.
    fn toggle_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.code.read(cx).is_shown() {
            self.dismiss(window, cx);
        } else {
            self.show_code(window, cx);
        }
    }

    // Brings the code over the home, with the tabs left open, or with none and the file tree
    // open to pick one.
    fn show_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.code.update(cx, |code, cx| code.show(window, cx));
        // With no tab, the outline has no file to follow.
        let active = self
            .bench
            .read(cx)
            .tabs()
            .active()
            .map(|doc| doc.path().to_path_buf());
        if active.is_none() {
            self.panel = true;
        }
        let line = active
            .as_deref()
            .and_then(|path| self.code.read(cx).line(path, cx));
        self.files.update(cx, |files, cx| {
            files.follow(active.as_deref(), line, window, cx)
        });
    }

    fn on_show_code(&mut self, _: &ShowCode, window: &mut Window, cx: &mut Context<Self>) {
        if !self.code.read(cx).is_shown() {
            self.show_code(window, cx);
        }
    }

    fn on_toggle_panel(&mut self, _: &TogglePanel, window: &mut Window, cx: &mut Context<Self>) {
        if !self.toggle_panel(window, cx) {
            cx.propagate();
        }
    }

    // Opens the panel of the window in front, or closes it; with only the home, there is none.
    // Closed, it hands the keys back to the window, should they have been in it.
    fn toggle_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let keep = self.paletting;
        if self.code.read(cx).is_shown() {
            self.panel = !self.panel;
            if !self.panel && !keep {
                self.code.update(cx, |code, cx| code.focus(window, cx));
            }
        } else if self.chatting {
            let open = self.chats.update(cx, |chats, cx| chats.toggle_panel(cx));
            if !open && !keep {
                self.dock.update(cx, |dock, cx| dock.focus(window, cx));
            }
        } else if self.gitting {
            let open = self.git.update(cx, |git, cx| git.toggle_panel(cx));
            if !open && !keep {
                self.rest(window, cx);
            }
        } else if self.terming {
            let open = self
                .terminals
                .update(cx, |terminals, cx| terminals.toggle_panel(cx));
            if !open && !keep {
                self.terminals
                    .update(cx, |terminals, cx| terminals.focus(window, cx));
            }
        } else {
            return false;
        }
        cx.notify();
        true
    }

    // What the button of a window's panel does.
    fn panel_button(&self, cx: &Context<Self>) -> impl Fn(&mut Window, &mut App) + 'static {
        let shell = cx.entity().downgrade();
        move |window, cx| {
            shell
                .update(cx, |shell, cx| {
                    shell.toggle_panel(window, cx);
                })
                .ok();
        }
    }

    fn on_dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        if self.paletting {
            self.close_palette(window, cx);
        } else if self.code.read(cx).is_shown() {
            self.dismiss(window, cx);
        } else if self.chatting {
            self.close_chat(window, cx);
        } else if self.gitting {
            self.close_git(window, cx);
        } else if self.terming {
            self.close_terminal(window, cx);
        } else {
            cx.propagate();
        }
    }

    // Files moved on disk: the home and the file tree read again the folders above them.
    fn on_moved(&mut self, _: Entity<Repos>, event: &repos::Event, cx: &mut Context<Self>) {
        let repos::Event(moved) = event;
        self.home.update(cx, |home, cx| home.uncover(moved, cx));
        self.files.update(cx, |files, cx| files.refresh(cx));
        self.read_scripts(cx);
        // The files listed for the palette are walked again when next asked.
        self.index.clear();
        if self.paletting {
            self.walk(cx);
        }
    }

    fn on_open_palette(&mut self, _: &OpenPalette, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_palette(window, cx);
    }

    fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.paletting {
            self.close_palette(window, cx);
        } else {
            self.open_palette(window, cx);
        }
    }

    // Opens the palette of the window in front, with nothing typed; with only the home, none.
    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !(self.code.read(cx).is_shown() || self.chatting || self.gitting || self.terming) {
            return;
        }
        self.before = window.focused(cx);
        self.paletting = true;
        self.palette
            .update(cx, |palette, cx| palette.set_query("", window, cx));
        let palette = self.palette.clone();
        palette.update(cx, |palette, cx| palette.focus(window, cx));
        self.walk(cx);
        cx.notify();
    }

    // Closes the palette, the keys going back where they were.
    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.paletting {
            return;
        }
        self.paletting = false;
        match self.before.take() {
            Some(before) => before.focus(window, cx),
            None => self.rest(window, cx),
        }
        cx.notify();
    }

    // Walks the files of the project in front off the main thread, unless they were walked.
    fn walk(&mut self, cx: &mut Context<Self>) {
        const MOST: usize = 20_000;
        let Some(root) = self.bench.read(cx).project().map(Path::to_path_buf) else {
            return;
        };
        if self.index.contains_key(&root) {
            return;
        }
        // Empty while it walks, so it walks once.
        self.index.insert(root.clone(), Rc::default());
        let task = cx.background_executor().spawn({
            let root = root.clone();
            // Each path as the palette shows and searches it, made here rather than on each
            // key typed.
            async move {
                corvo_fs::files(&root, MOST)
                    .into_iter()
                    .map(|path| Listed::new(&root, path))
                    .collect::<Vec<_>>()
            }
        });
        cx.spawn(async move |this, cx| {
            let files = task.await;
            this.update(cx, |this, cx| {
                this.index.insert(root, Rc::new(files));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_show_git(&mut self, _: &ShowGit, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_git(window, cx);
    }

    fn on_show_chat(&mut self, _: &ShowChat, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_chat(window, cx);
    }

    fn on_show_terminal(&mut self, _: &ShowTerminal, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_terminal(window, cx);
    }

    // The terminal window comes over the home in the code's place, or goes.
    fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terming {
            self.close_terminal(window, cx);
        } else {
            self.show_terminal(window, cx);
        }
    }

    // Brings the terminal window over the home, with a terminal in the project in front when
    // there is none, the keys in it.
    fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terming {
            self.dismiss(window, cx);
            if self.chatting {
                self.close_chat(window, cx);
            }
            self.gitting = false;
            self.terming = true;
        }
        self.terminals.update(cx, |terminals, cx| {
            terminals.ensure(window, cx);
            terminals.focus(window, cx);
        });
        cx.notify();
    }

    fn close_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terming = false;
        self.paletting = false;
        self.rest(window, cx);
        cx.notify();
    }

    // A script picked in the status bar runs in a terminal of its own, the terminal window
    // coming over the home to show it.
    fn on_scripts(
        &mut self,
        _: &Entity<Scripts>,
        event: &scripts::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            scripts::Event::Run(script) => {
                self.scripting = false;
                self.run_script(script, window, cx);
            }
        }
    }

    fn run_script(&mut self, script: &Script, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.scripts.read(cx).root().map(Path::to_path_buf) else {
            return;
        };
        self.terminals
            .update(cx, |terminals, cx| terminals.run(script, &root, window, cx));
        self.show_terminal(window, cx);
    }

    // The git window comes over the home in the code's place, or goes.
    fn toggle_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.gitting {
            self.close_git(window, cx);
            return;
        }
        self.dismiss(window, cx);
        if self.chatting {
            self.close_chat(window, cx);
        }
        self.terming = false;
        self.gitting = true;
        self.repos.update(cx, |repos, cx| repos.refresh(cx));
        self.git.update(cx, |git, cx| git.reread(cx));
        cx.notify();
    }

    // The keys go back to the window: into the terminal in the terminal window, to the home when
    // nothing is over it, so Space held moves its graph.
    fn rest(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terming {
            self.terminals
                .update(cx, |terminals, cx| terminals.focus(window, cx));
        } else if self.code.read(cx).is_shown() || self.chatting || self.gitting {
            self.focus.focus(window, cx);
        } else {
            self.home.update(cx, |home, cx| home.focus(window, cx));
        }
    }

    fn close_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.gitting = false;
        self.paletting = false;
        self.rest(window, cx);
        cx.notify();
    }

    // An image sent in a chat opens in the code, in the chat window's place.
    fn on_chats(
        &mut self,
        _: &Entity<ChatWindow>,
        event: &chats::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            chats::Event::Open(path) => {
                self.close_chat(window, cx);
                self.code.update(cx, |code, cx| code.open(path, window, cx));
            }
        }
    }

    fn on_git(
        &mut self,
        _: &Entity<GitWindow>,
        event: &git::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            git::Event::Open(path) => {
                self.gitting = false;
                self.code.update(cx, |code, cx| code.open(path, window, cx));
            }
        }
    }

    // The chat window comes over the home in the code's place, or goes.
    fn toggle_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chatting {
            self.close_chat(window, cx);
            return;
        }
        self.dismiss(window, cx);
        self.gitting = false;
        self.terming = false;
        self.chatting = true;
        self.dock.update(cx, |dock, cx| {
            dock.set_chatting(true, cx);
            dock.focus(window, cx);
        });
        self.chats.update(cx, |chats, cx| chats.shown(cx));
        cx.notify();
    }

    fn close_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chatting = false;
        self.paletting = false;
        self.dock
            .update(cx, |dock, cx| dock.set_chatting(false, cx));
        self.rest(window, cx);
        cx.notify();
    }

    fn on_agents(
        &mut self,
        _: &Entity<Agents>,
        note: &agents::Note,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match note {
            agents::Note::Missing { agent, hint } => {
                let label = if hint.contains(' ') && !hint.starts_with("Node") {
                    "Copy command"
                } else {
                    "Copy"
                };
                notice::warn_with_copy(
                    format!("{agent} not found. Install: {hint}"),
                    label,
                    hint.clone(),
                    window,
                    cx,
                );
            }
            agents::Note::Notice(message) => notice::warn(message.clone(), window, cx),
        }
    }

    fn on_dock(
        &mut self,
        _: &Entity<Dock>,
        event: &dock::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            dock::Event::Chat => self.toggle_chat(window, cx),
        }
    }

    // Keeps how the home is now, to come back to it the same.
    fn arrange(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home = self.bench.read(cx).atlas().arrangement();
        let id = self.id;
        if let Err(err) = self.spaces.update(cx, |spaces, _| spaces.arrange(id, home)) {
            notice::error(err.to_string(), window, cx);
        }
    }

    // Asks for a folder and links it to the workspace.
    fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Link".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = picked.await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.link(path, window, cx);
                    }
                }
                Ok(Ok(None)) | Err(_) => {}
                Ok(Err(err)) => notice::error(err.to_string(), window, cx),
            })
            .ok();
        })
        .detach();
    }

    fn link(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.id;
        match self.spaces.update(cx, |spaces, _| spaces.link(id, &path)) {
            Ok(()) => self.show(Some(path), window, cx),
            Err(err) => notice::error(err.to_string(), window, cx),
        }
    }

    fn unlink(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.id;
        if let Err(err) = self.spaces.update(cx, |spaces, _| spaces.unlink(id, path)) {
            notice::error(err.to_string(), window, cx);
            return;
        }
        self.bench.update(cx, |bench, _| bench.forget(path));
        let next = match self.bench.read(cx).project() {
            Some(project) => Some(project.to_path_buf()),
            None => self.linked(cx).into_iter().next(),
        };
        self.show(next, window, cx);
        self.weigh(cx);
    }

    fn on_files(
        &mut self,
        _: &Entity<Files>,
        event: &files::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            files::Event::Pick(path) => {
                self.code.update(cx, |code, cx| code.open(path, window, cx));
            }
            files::Event::Jump(path, line, column) => {
                let at = Position::new(*line, *column);
                self.code
                    .update(cx, |code, cx| code.reveal(path, Some(at), window, cx));
            }
        }
    }

    fn on_home(
        &mut self,
        _: &Entity<Overview>,
        event: &overview::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            // The file tree follows the project open in the home.
            overview::Event::Isolated(root) => {
                if let Some(root) = root {
                    self.show(Some(root.clone()), window, cx);
                }
                self.weigh(cx);
            }
            overview::Event::Link => self.browse(window, cx),
            overview::Event::Unlink(path) => self.unlink(path, window, cx),
            overview::Event::Arranged => self.arrange(window, cx),
            overview::Event::Open(path) => {
                self.code.update(cx, |code, cx| code.open(path, window, cx));
            }
            overview::Event::Terminal(dir) => {
                self.terminals
                    .update(cx, |terminals, cx| terminals.open_in(dir, window, cx));
                self.show_terminal(window, cx);
            }
        }
    }

    fn on_code(
        &mut self,
        _: &Entity<Code>,
        event: &code::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            code::Event::Active(path) => {
                let owner = self
                    .spaces
                    .read(cx)
                    .get(self.id)
                    .and_then(|space| space.owner(path))
                    .map(Path::to_path_buf);
                if owner.is_some() && owner.as_deref() != self.bench.read(cx).project() {
                    self.show(owner, window, cx);
                }
                let line = self.code.read(cx).line(path, cx);
                self.files.update(cx, |files, cx| {
                    files.follow(Some(path), line, window, cx);
                    files.reveal(path, window, cx);
                });
            }
            code::Event::Saved(path) => {
                self.files
                    .update(cx, |files, cx| files.saved(path, window, cx));
                self.repos.update(cx, |repos, cx| repos.refresh(cx));
            }
            code::Event::Cursor(path, line) => {
                self.files
                    .update(cx, |files, cx| files.cursor(path, *line, cx));
            }
            // The outline has no file left, the file tree opens to pick another, and the keys go
            // back to the window.
            code::Event::Emptied => {
                self.files
                    .update(cx, |files, cx| files.follow(None, None, window, cx));
                self.panel = true;
                self.rest(window, cx);
            }
            // The keys go back to the window, not to the editor gone.
            code::Event::Dismissed => self.rest(window, cx),
        }
    }

    fn on_note(&mut self, note: Note, window: &mut Window, cx: &mut Context<Self>) {
        match note {
            Note::Diagnostics(path, items) => {
                self.code
                    .update(cx, |code, cx| code.diagnose(&path, items, cx));
            }
            Note::Missing { server, hint } => {
                let label = if hint.starts_with("http") {
                    "Copy link"
                } else {
                    "Copy command"
                };
                notice::warn_with_copy(
                    format!("{server} not found. Install: {hint}"),
                    label,
                    hint,
                    window,
                    cx,
                );
            }
            Note::Failed { server, message } => {
                notice::error(format!("{server}: {message}"), window, cx);
            }
            Note::Exited { server, root } => {
                self.hub.update(cx, |hub, _| hub.exited(&server, &root));
                notice::warn(format!("{server} stopped"), window, cx);
            }
        }
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.code.update(cx, |code, cx| code.save(window, cx));
    }
}

impl Shell {
    // The code over the home: the tabs, and the panel floating on their right with the file tree,
    // the active file's outline nested under its row.
    fn overlay(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        // The tree is drawn again only when it changes, not on each key typed beside it.
        let panel = self.panel.then(|| {
            let files =
                AnyView::from(self.files.clone()).cached(StyleRefinement::default().size_full());
            Drawer::new("code-panel").child(div().flex_1().min_h_0().child(files))
        });
        let body = div()
            .relative()
            .size_full()
            .child(self.code.clone())
            .children(panel);
        let bar = WindowBar::new(Glyph::Code, "Code").panel(
            self.panel,
            "Files (Ctrl+B)",
            self.panel_button(cx),
        );
        let offers = self.paletting.then(|| {
            let query = self.palette.read(cx).query(cx).to_string();
            let files = self
                .bench
                .read(cx)
                .project()
                .and_then(|root| self.index.get(root))
                .cloned()
                .unwrap_or_default();
            palette::code(&self.code, &self.bench, &files, &self.search, &query, cx)
        });
        let ask = ("Open a file", offers);
        self.window("code", bar, body.into_any_element(), ask, Self::dismiss, cx)
    }

    // The chat window over the home; the agent bar stays over its foot.
    fn chat_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let subtitle = self.chats.read(cx).subtitle(cx);
        let bar = WindowBar::new(Glyph::MessageSquare, "Chat")
            .subtitle(subtitle)
            .panel(
                self.chats.read(cx).has_panel(),
                "Chats (Ctrl+B)",
                self.panel_button(cx),
            );
        let body = self.chats.clone().into_any_element();
        let offers = self.paletting.then(|| {
            let query = self.palette.read(cx).query(cx).to_string();
            palette::chats(&self.chats, &self.agents, &query, cx)
        });
        let ask = ("Search chats or start one", offers);
        self.window("chats", bar, body, ask, Self::close_chat, cx)
    }

    fn git_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let bar = WindowBar::new(Glyph::GitBranch, "Git").panel(
            self.git.read(cx).has_panel(),
            "Changes (Ctrl+B)",
            self.panel_button(cx),
        );
        let body = self.git.clone().into_any_element();
        let offers = self.paletting.then(|| {
            let query = self.palette.read(cx).query(cx).to_string();
            palette::git(&self.git, &self.repos, &query, cx)
        });
        let ask = ("Find a changed file", offers);
        self.window("git", bar, body, ask, Self::close_git, cx)
    }

    fn terminal_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let terminals = self.terminals.read(cx);
        let bar = WindowBar::new(Glyph::Terminal, "Terminal")
            .subtitle(terminals.subtitle())
            .panel(
                terminals.has_panel(),
                "Terminals (Ctrl+B)",
                self.panel_button(cx),
            );
        let body = self.terminals.clone().into_any_element();
        let offers = self.paletting.then(|| {
            let query = self.palette.read(cx).query(cx).to_string();
            let scripts = self.scripts.read(cx);
            palette::terminals(
                &self.terminals,
                scripts.scripts(),
                scripts.root(),
                &query,
                cx,
            )
        });
        let ask = ("Find a terminal or run a script", offers);
        self.window("terminal", bar, body, ask, Self::close_terminal, cx)
    }

    // A window over the home, which steps back behind a dimmed layer: its bar along the top, with
    // the field of its palette in the middle, and what it holds under it; the palette, while open,
    // under the bar with what it offers. Esc, the bar's × or a click on the dimmed home puts the
    // window away. Opening, the home dims and the window rises a little into place, fading in;
    // the dimmed layer is the same for every window, so going from one to another only brings the
    // new one in.
    fn window(
        &self,
        id: &'static str,
        bar: WindowBar,
        body: AnyElement,
        (placeholder, offers): (&'static str, Option<Vec<Offer>>),
        close: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let palette = offers.map(|offers| self.palette(placeholder, offers, cx));
        let theme = cx.theme();
        let (dimmed, closer) = (cx.entity().downgrade(), cx.entity().downgrade());
        let opener = cx.entity().downgrade();
        let field = PaletteField::new(placeholder).on_click(move |window, cx| {
            opener
                .update(cx, |shell, cx| shell.toggle_palette(window, cx))
                .ok();
        });
        let scrim = div()
            .id("window-scrim")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(theme.background.opacity(0.75))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                dimmed.update(cx, |shell, cx| close(shell, window, cx)).ok();
            })
            .with_animation("window-dim", motion::opening(), |scrim, delta| {
                scrim.opacity(delta)
            });
        let bar = bar.center(field).on_close(move |window, cx| {
            closer.update(cx, |shell, cx| close(shell, window, cx)).ok();
        });
        let panel = v_flex()
            .id(id)
            .absolute()
            .top(px(GAP))
            .bottom(px(GAP))
            .left(px(48.))
            .right(px(48.))
            .occlude()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(bar)
            .child(div().flex_1().min_h_0().child(body))
            .children(palette)
            // Top and bottom move together, so what the window holds keeps its size: a terminal
            // is not resized on every frame.
            .with_animation(
                ElementId::Name(format!("{id}-rise").into()),
                motion::opening(),
                |panel, delta| {
                    let rise = RISE * (1. - delta);
                    panel
                        .top(px(GAP + rise))
                        .bottom(px(GAP - rise))
                        .opacity(delta)
                },
            );
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(scrim)
            .child(panel)
    }

    // The palette under the bar: a search over what the window offers, the arrows and Enter or a
    // click to pick. A click anywhere else closes it. It drops a little into place as it opens,
    // fading in.
    fn palette(
        &self,
        placeholder: &'static str,
        offers: Vec<Offer>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let items: Vec<CommandItem> = offers.iter().map(item).collect();
        let offers = Rc::new(offers);
        let shell = cx.entity().downgrade();
        let (asker, picker, leaver) = (shell.clone(), shell.clone(), shell.clone());
        let command = Command::new(&self.palette)
            .filterable(false)
            .placeholder(placeholder)
            .bordered(false)
            .max_h(px(380.))
            .items(items)
            .empty(|_, _, _| "Nothing found")
            .on_query(move |_, _, cx| {
                asker.update(cx, |_, cx| cx.notify()).ok();
            })
            .on_confirm(move |at, window, cx| {
                let Some(run) = offers.get(at.row).map(|offer| offer.run.clone()) else {
                    return;
                };
                picker
                    .update(cx, |shell, cx| shell.close_palette(window, cx))
                    .ok();
                run(window, cx);
            })
            // Esc with nothing typed: closed once the key is done with, unless Esc closed it.
            .on_cancel(move |window, cx| {
                let leaver = leaver.clone();
                window.defer(cx, move |window, cx| {
                    leaver
                        .update(cx, |shell, cx| shell.close_palette(window, cx))
                        .ok();
                });
            });
        let catcher = div()
            .id("palette-catcher")
            .absolute()
            .top(px(44.))
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                shell
                    .update(cx, |shell, cx| shell.close_palette(window, cx))
                    .ok();
            });
        let panel = div()
            .absolute()
            .top(px(UNDER_BAR))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(560.))
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.popover)
                    .shadow_lg()
                    .overflow_hidden()
                    .child(command),
            )
            .with_animation("palette-drop", motion::opening(), |panel, delta| {
                panel
                    .top(px(UNDER_BAR - DROP * (1. - delta)))
                    .opacity(delta)
            });
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(catcher)
            .child(panel)
    }

    // The branch of the project in front, and how many files changed in its repository; a
    // click opens the git window.
    fn git_item(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let repos = self.repos.read(cx);
        let repo = match self.bench.read(cx).project() {
            Some(project) => repos.repo_of(project),
            None => repos.repos().first(),
        }?;
        let label = match repo.changes().len() {
            0 => repo.branch().to_owned(),
            n => format!("{} · {n}", repo.branch()),
        };
        let shell = cx.entity().downgrade();
        Some(
            Action::new("git", Glyph::GitBranch, "Git (Ctrl+Shift+G)")
                .label(label)
                .on_click(move |window, cx| {
                    shell
                        .update(cx, |shell, cx| shell.toggle_git(window, cx))
                        .ok();
                }),
        )
    }

    // The scripts of the project in front, when it has some; a click lists them, and a pick runs
    // one in a terminal. Opened, they are read again, for what changed meanwhile.
    fn scripts_item(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        if self.scripts.read(cx).scripts().is_empty() {
            return None;
        }
        let shell = cx.entity().downgrade();
        Some(
            Popup::new("scripts", "Scripts", self.scripts.clone())
                .glyph(Glyph::Play)
                .open(self.scripting)
                .on_toggle(move |open, _, cx| {
                    shell
                        .update(cx, |shell, cx| {
                            shell.scripting = open;
                            if open {
                                shell.read_scripts(cx);
                            }
                            cx.notify();
                        })
                        .ok();
                }),
        )
    }

    // The logos of the main languages of the shown project; a click lists them all, named.
    fn stack_item(&self, shares: &[Share], cx: &mut Context<Self>) -> impl IntoElement + use<> {
        const SHOWN: usize = 3;
        let shell = cx.entity().downgrade();
        let mut item = Popup::new("stack", "", self.stack.clone())
            .anchor(gpui_kit::Anchor::BottomRight)
            .open(self.stacking)
            .on_toggle(move |open, _, cx| {
                shell
                    .update(cx, |shell, cx| {
                        shell.stacking = open;
                        cx.notify();
                    })
                    .ok();
            });
        for share in shares.iter().take(SHOWN) {
            item = item.child(LanguageIcon::new(share.name()).size(gpui_kit::px(14.)));
        }
        if shares.len() > SHOWN {
            item = item.child(format!("+{}", shares.len() - SHOWN));
        }
        item
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.code.read(cx).is_shown();
        let overlay = if shown {
            Some(self.overlay(cx).into_any_element())
        } else if self.chatting {
            Some(self.chat_overlay(cx).into_any_element())
        } else if self.gitting {
            Some(self.git_overlay(cx).into_any_element())
        } else if self.terming {
            Some(self.terminal_overlay(cx).into_any_element())
        } else {
            None
        };

        let count = self.bench.read(cx).atlas().hidden_count();
        let label = match count {
            0 => "Nothing hidden".to_owned(),
            n => format!("{n} hidden"),
        };
        let tip = if shown {
            "Put the code away"
        } else {
            "Open the code"
        };
        let shell = cx.entity().downgrade();
        let coder = cx.entity().downgrade();
        let linker = cx.entity().downgrade();
        let termer = cx.entity().downgrade();
        let terminal_tip = if self.terming {
            "Put the terminal away (Ctrl+Shift+J)"
        } else {
            "Terminal (Ctrl+Shift+J)"
        };
        let mut status = StatusBar::new()
            .item(
                Action::new("code", Glyph::Code, tip).on_click(move |window, cx| {
                    coder
                        .update(cx, |shell, cx| shell.toggle_code(window, cx))
                        .ok();
                }),
            )
            .item(
                Action::new("terminal", Glyph::Terminal, terminal_tip).on_click(
                    move |window, cx| {
                        termer
                            .update(cx, |shell, cx| shell.toggle_terminal(window, cx))
                            .ok();
                    },
                ),
            )
            .item(
                Action::new("link", Glyph::FolderPlus, "Link folder…").on_click(
                    move |window, cx| {
                        linker.update(cx, |shell, cx| shell.browse(window, cx)).ok();
                    },
                ),
            )
            .item(
                Popup::new("hidden", label, self.hidden.clone())
                    .glyph(Glyph::EyeOff)
                    .open(self.hiding)
                    .on_toggle(move |open, _, cx| {
                        shell
                            .update(cx, |shell, cx| {
                                shell.hiding = open;
                                cx.notify();
                            })
                            .ok();
                    }),
            );
        if let Some(item) = self.git_item(cx) {
            status = status.item(item);
        }
        if let Some(item) = self.scripts_item(cx) {
            status = status.item(item);
        }
        if let Some(shares) = self
            .bench
            .read(cx)
            .atlas()
            .root()
            .and_then(|root| self.tongues.get(root))
            .filter(|shares| !shares.is_empty())
        {
            status = status
                .item(div().flex_1())
                .item(self.stack_item(shares, cx));
        }

        v_flex()
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::on_show_code))
            .on_action(cx.listener(Self::on_show_git))
            .on_action(cx.listener(Self::on_show_chat))
            .on_action(cx.listener(Self::on_show_terminal))
            .on_action(cx.listener(Self::on_open_palette))
            .on_action(cx.listener(Self::on_toggle_panel))
            .on_action(cx.listener(Self::on_dismiss))
            .size_full()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    // The home is drawn again only when it changes, not on each key or blink of
                    // the window over it: with many folders open it is most of a frame.
                    .child(
                        AnyView::from(self.home.clone())
                            .cached(StyleRefinement::default().size_full()),
                    )
                    .children(overlay)
                    .child(self.dock.clone()),
            )
            .child(status)
    }
}

// What each agent offered last, kept between runs.
fn option_store() -> JsonOptions {
    JsonOptions::config()
        .unwrap_or_else(|_| JsonOptions::new(std::env::temp_dir().join("corvo-agents.json")))
}

// The chats kept for the workspace; ones that do not read leave it with none, and say why.
fn load_chats(workspace: Uuid) -> (Chats, Option<String>) {
    let store = || {
        JsonChats::config()
            .unwrap_or_else(|_| JsonChats::new(std::env::temp_dir().join("corvo-chats")))
    };
    match Chats::load(workspace, Box::new(store())) {
        Ok(chats) => (chats, None),
        Err(err) => (
            Chats::empty(workspace, Box::new(store())),
            Some(format!("chats: {err}")),
        ),
    }
}

// The folder a new chat works in: the project isolated in the home, the one the active tab
// belongs to, or the first one of the workspace.
fn focused(bench: Entity<Workbench>, spaces: Entity<Workspaces>, id: Uuid) -> Project {
    Rc::new(move |cx: &App| {
        let open = bench.read(cx);
        open.atlas()
            .root()
            .map(Path::to_path_buf)
            .or_else(|| open.project().map(Path::to_path_buf))
            .or_else(|| {
                spaces
                    .read(cx)
                    .get(id)
                    .and_then(|space| space.projects().first().cloned())
            })
            .or_else(std::env::home_dir)
            .unwrap_or_else(std::env::temp_dir)
    })
}

// An offer as a row of the palette: its icon, its name, and what tells it apart, muted.
fn item(offer: &Offer) -> CommandItem {
    let (glyph, label, detail) = (offer.glyph, offer.label.clone(), offer.detail.clone());
    CommandItem::new().label(label.clone()).child(move |_, cx| {
        let theme = cx.theme();
        h_flex()
            .min_w_0()
            .gap_2()
            .child(Icon::new(glyph).small().text_color(theme.muted_foreground))
            .child(div().flex_shrink_0().child(label.clone()))
            .children(detail.clone().map(|detail| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(detail)
            }))
    })
}
