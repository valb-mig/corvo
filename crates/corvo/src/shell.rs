use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use corvo_domain::server::Registry;
use corvo_domain::{Chats, Share, Uuid, Workbench, Workspaces, agents, languages};
use corvo_fs::{JsonChats, JsonOptions, LocalFs};
use corvo_git::GitCli;
use corvo_lsp::{Hub, Note};
use corvo_ui::keys::{Dismiss, Save, ShowCode, ShowGit, ToggleTree};
use corvo_ui::language_icon::LanguageIcon;
use corvo_ui::notice;
use corvo_ui::statusbar::{Action, Popup, StatusBar};
use futures::StreamExt as _;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, PathPromptOptions, Render, Styled as _, Subscription, Window,
    div, prelude::FluentBuilder as _, px, relative,
};
use lsp_types::Position;

use crate::agents::{self, Agents};
use crate::pane::chats::{self, ChatWindow};
use crate::pane::code::{self, Code};
use crate::pane::dock::{self, Dock, Project};
use crate::pane::files::{self, Files};
use crate::pane::git::{self, GitWindow};
use crate::pane::hidden::Hidden;
use crate::pane::overview::{self, Overview};
use crate::pane::stack::Stack;
use crate::pane::structure::{self, Structure};
use crate::repos::Repos;

// How often git is read again while the window is in front, for what the agent or a terminal
// changed.
const POLL: Duration = Duration::from_secs(3);

pub struct Shell {
    focus: FocusHandle,
    spaces: Entity<Workspaces>,
    id: Uuid,
    bench: Entity<Workbench>,
    hub: Entity<Hub>,
    // The open files, over the home while shown, with the file tree and the active file's
    // structure beside them.
    code: Entity<Code>,
    files: Entity<Files>,
    structure: Entity<Structure>,
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
    // The file tree beside the code, hidden with ctrl-b.
    tree: bool,
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
        let home = cx.new(|_| Overview::new(bench.clone(), registry, repos.clone()));
        let code = cx.new(|cx| {
            Code::new(bench.clone(), hub.clone(), home.clone(), repos.clone(), cx)
        });
        let files = cx.new(|cx| Files::new(bench.clone(), repos.clone(), window, cx));
        let git = cx.new(|cx| GitWindow::new(repos.clone(), cx));
        let structure = cx.new(|cx| Structure::new(bench.clone(), window, cx));
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
        let chats = cx.new(|cx| ChatWindow::new(agents.clone(), project, name.into(), window, cx));
        if let Some(trouble) = trouble {
            notice::error(trouble, window, cx);
        }
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = agent_events.next().await {
                // A reply that ended may have changed files.
                let ended = matches!(event, corvo_agent::Event::Ended { .. });
                let alive = this.update(cx, |shell, cx| {
                    shell
                        .agents
                        .update(cx, |agents, cx| agents.apply(event, cx));
                    if ended {
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
            cx.subscribe_in(&chats, window, Self::on_chats),
            cx.subscribe_in(&files, window, Self::on_files),
            cx.subscribe_in(&structure, window, Self::on_structure),
            cx.subscribe_in(&code, window, Self::on_code),
            cx.subscribe_in(&home, window, Self::on_home),
            cx.subscribe_in(&git, window, Self::on_git),
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
                }
                cx.notify();
            }),
            // The structure card shows or goes with the active file.
            cx.observe(&structure, |_, _, cx| cx.notify()),
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

        let focus = cx.focus_handle();
        focus.focus(window, cx);

        let mut shell = Self {
            focus,
            spaces,
            id,
            bench,
            hub,
            code,
            files,
            structure,
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
            tree: true,
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

    // Brings the code over the home, with the tabs left open, or with none.
    fn show_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.code.update(cx, |code, cx| code.show(window, cx));
        // With no tab, the structure has no file to follow.
        let active = self
            .bench
            .read(cx)
            .tabs()
            .active()
            .map(|doc| doc.path().to_path_buf());
        self.structure.update(cx, |structure, cx| {
            structure.follow(active.as_deref(), window, cx)
        });
    }

    fn on_show_code(&mut self, _: &ShowCode, window: &mut Window, cx: &mut Context<Self>) {
        if !self.code.read(cx).is_shown() {
            self.show_code(window, cx);
        }
    }

    fn on_toggle_tree(&mut self, _: &ToggleTree, _: &mut Window, cx: &mut Context<Self>) {
        if self.code.read(cx).is_shown() {
            self.tree = !self.tree;
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    fn on_dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        if self.code.read(cx).is_shown() {
            self.dismiss(window, cx);
        } else if self.chatting {
            self.close_chat(window, cx);
        } else if self.gitting {
            self.close_git(window, cx);
        } else {
            cx.propagate();
        }
    }

    fn on_show_git(&mut self, _: &ShowGit, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_git(window, cx);
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
        self.gitting = true;
        self.repos.update(cx, |repos, cx| repos.refresh(cx));
        self.git.update(cx, |git, cx| git.reread(cx));
        cx.notify();
    }

    fn close_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.gitting = false;
        self.focus.focus(window, cx);
        cx.notify();
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
            git::Event::Close => self.close_git(window, cx),
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
        self.dock
            .update(cx, |dock, cx| dock.set_chatting(false, cx));
        self.focus.focus(window, cx);
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

    fn on_chats(
        &mut self,
        _: &Entity<ChatWindow>,
        event: &chats::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            chats::Event::Close => self.close_chat(window, cx),
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
        }
    }

    fn on_structure(
        &mut self,
        _: &Entity<Structure>,
        event: &structure::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            structure::Event::Jump(path, line, column) => {
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
                self.files
                    .update(cx, |files, cx| files.reveal(path, window, cx));
                self.structure
                    .update(cx, |structure, cx| structure.follow(Some(path), window, cx));
            }
            code::Event::Saved(path) => {
                self.structure
                    .update(cx, |structure, cx| structure.saved(path, window, cx));
                self.repos.update(cx, |repos, cx| repos.refresh(cx));
            }
            // The structure has no file left, and the keys go back to the window.
            code::Event::Emptied => {
                self.structure
                    .update(cx, |structure, cx| structure.follow(None, window, cx));
                self.focus.focus(window, cx);
            }
            // The keys go back to the window, not to the editor gone.
            code::Event::Dismissed => self.focus.focus(window, cx),
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
    // The code over the home, which steps back behind a dimmed layer: the tabs, and the sidebar
    // on the right with the file tree over the active file's structure. A click on the dimmed home
    // puts it away.
    fn overlay(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        // A file whose language has no outline leaves the tree the whole sidebar, and the tree
        // hidden leaves it the structure; with neither, there is no sidebar.
        let tree = self.tree;
        let card = self.structure.read(cx).is_shown(cx).then(|| {
            div()
                .map(|el| {
                    if tree {
                        el.flex_shrink_0().h(relative(0.45))
                    } else {
                        el.flex_1().min_h_0()
                    }
                })
                .p_2()
                .child(self.structure.clone())
        });
        let files = tree.then(|| div().flex_1().min_h_0().child(self.files.clone()));
        let side = (files.is_some() || card.is_some()).then(|| {
            v_flex()
                .w(px(280.))
                .h_full()
                .flex_shrink_0()
                .border_l_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().sidebar)
                .children(files)
                .children(card)
        });
        let theme = cx.theme();
        let shell = cx.entity().downgrade();
        let scrim = div()
            .id("scrim")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(theme.background.opacity(0.75))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                shell.update(cx, |shell, cx| shell.dismiss(window, cx)).ok();
            });
        let panel = h_flex()
            .id("code")
            .absolute()
            .top(px(32.))
            .bottom(px(32.))
            .left(px(48.))
            .right(px(48.))
            .occlude()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(div().flex_1().min_w_0().h_full().child(self.code.clone()))
            .children(side);
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(scrim)
            .child(panel)
    }

    // The chat window over the home, as the code comes: the home dimmed behind, a click on it
    // puts the window away. The agent bar stays over its foot.
    fn chat_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let shell = cx.entity().downgrade();
        let scrim = div()
            .id("chat-scrim")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(theme.background.opacity(0.75))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                shell
                    .update(cx, |shell, cx| shell.close_chat(window, cx))
                    .ok();
            });
        let panel = div()
            .id("chats")
            .absolute()
            .top(px(32.))
            .bottom(px(32.))
            .left(px(48.))
            .right(px(48.))
            .occlude()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(self.chats.clone());
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(scrim)
            .child(panel)
    }

    // The git window over the home, as the chat window comes.
    fn git_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let shell = cx.entity().downgrade();
        let scrim = div()
            .id("git-scrim")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(theme.background.opacity(0.75))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                shell
                    .update(cx, |shell, cx| shell.close_git(window, cx))
                    .ok();
            });
        let panel = div()
            .id("git")
            .absolute()
            .top(px(32.))
            .bottom(px(32.))
            .left(px(48.))
            .right(px(48.))
            .occlude()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(self.git.clone());
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(scrim)
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
        let mut status = StatusBar::new()
            .item(
                Action::new("code", Glyph::Code, tip).on_click(move |window, cx| {
                    coder
                        .update(cx, |shell, cx| shell.toggle_code(window, cx))
                        .ok();
                }),
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
            .on_action(cx.listener(Self::on_toggle_tree))
            .on_action(cx.listener(Self::on_dismiss))
            .size_full()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.home.clone())
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
