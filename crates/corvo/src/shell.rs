use std::collections::HashMap;
use std::path::{Path, PathBuf};

use corvo_domain::server::Registry;
use corvo_domain::{Share, Workbench, Workspaces, languages};
use corvo_fs::LocalFs;
use corvo_lsp::{Hub, Note};
use corvo_ui::keys::Save;
use corvo_ui::language_icon::LanguageIcon;
use corvo_ui::layout::Split;
use corvo_ui::notice;
use corvo_ui::statusbar::{Action, Popup, StatusBar};
use futures::StreamExt as _;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyView, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, Styled as _, Subscription, Window, div,
};
use lsp_types::Position;

use crate::pane::code::{self, Code};
use crate::pane::files::{self, Files};
use crate::pane::hidden::Hidden;
use crate::pane::overview::{self, Overview};
use crate::pane::stack::Stack;

enum Center {
    Code(Entity<Code>),
}

impl Center {
    fn view(&self) -> AnyView {
        match self {
            Center::Code(code) => code.clone().into(),
        }
    }
}

pub struct Shell {
    focus: FocusHandle,
    spaces: Entity<Workspaces>,
    id: u64,
    bench: Entity<Workbench>,
    hub: Entity<Hub>,
    center: Center,
    files: Entity<Files>,
    home: Entity<Overview>,
    hidden: Entity<Hidden>,
    hiding: bool,
    // The languages of each project looked over, and the panel listing those of the project open
    // in the home.
    tongues: HashMap<PathBuf, Vec<Share>>,
    stack: Entity<Stack>,
    stacking: bool,
    _subs: Vec<Subscription>,
}

impl Shell {
    pub fn new(
        spaces: Entity<Workspaces>,
        id: u64,
        bench: Workbench,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let registry = Registry::default();
        let (hub, mut notes) = Hub::new(registry.clone());
        let hub = cx.new(|_| hub);
        let bench = cx.new(|_| bench);
        let home = cx.new(|_| Overview::new(bench.clone(), registry));
        let code = cx.new(|_| Code::new(bench.clone(), hub.clone(), home.clone()));
        let files = cx.new(|cx| Files::new(bench.clone(), window, cx));
        let hidden = cx.new(|_| Hidden::new(bench.clone(), home.clone()));
        let stack = cx.new(|_| Stack::default());
        let subs = vec![
            cx.subscribe_in(&files, window, Self::on_files),
            cx.subscribe_in(&code, window, Self::on_code),
            cx.subscribe_in(&home, window, Self::on_home),
            // The status bar counts what the home hides.
            cx.observe(&home, |_, _, cx| cx.notify()),
        ];

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
            center: Center::Code(code),
            files,
            home,
            hidden,
            hiding: false,
            tongues: HashMap::new(),
            stack,
            stacking: false,
            _subs: subs,
        };
        let first = shell.linked(cx).into_iter().next();
        shell.show(first, window, cx);
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
        let name = self
            .spaces
            .read(cx)
            .get(self.id)
            .map(|space| space.name().to_owned())
            .unwrap_or_default();
        self.home
            .update(cx, |home, cx| home.chart(name, &linked, window, cx));
    }

    fn rescope(&mut self, cx: &mut Context<Self>) {
        let linked = self.linked(cx);
        let active = self.bench.read(cx).project().map(Path::to_path_buf);
        self.hub.update(cx, |hub, _| hub.scope(linked, active));
    }

    // Shows the home, with every card back in place and the graph centred.
    fn reorganize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Center::Code(code) = &self.center;
        code.update(cx, |code, cx| code.go_home(window, cx));
        self.home.update(cx, |home, cx| home.reorganize(cx));
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
        match (event, &self.center) {
            (files::Event::Pick(path), Center::Code(code)) => {
                code.update(cx, |code, cx| code.open(path, window, cx));
            }
            (files::Event::Jump(path, line, column), Center::Code(code)) => {
                let at = Position::new(*line, *column);
                code.update(cx, |code, cx| code.reveal(path, Some(at), window, cx));
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
            overview::Event::Saved(path, text) => {
                let Center::Code(code) = &self.center;
                code.update(cx, |code, cx| code.reload(path, text, window, cx));
                self.files
                    .update(cx, |files, cx| files.saved(path, window, cx));
            }
            // The file tree follows the project open in the home.
            overview::Event::Isolated(root) => {
                if let Some(root) = root {
                    self.show(Some(root.clone()), window, cx);
                }
                self.weigh(cx);
            }
            overview::Event::Link => self.browse(window, cx),
            overview::Event::Unlink(path) => self.unlink(path, window, cx),
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
            }
            code::Event::Saved(path) => {
                self.files
                    .update(cx, |files, cx| files.saved(path, window, cx));
                self.home
                    .update(cx, |home, cx| home.saved(path, window, cx));
            }
        }
    }

    fn on_note(&mut self, note: Note, window: &mut Window, cx: &mut Context<Self>) {
        match note {
            Note::Diagnostics(path, items) => match &self.center {
                Center::Code(code) => code.update(cx, |code, cx| code.diagnose(&path, items, cx)),
            },
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
        match &self.center {
            Center::Code(code) => code.update(cx, |code, cx| code.save(window, cx)),
        }
    }
}

impl Shell {
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
        let side = v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .child(div().flex_1().min_h_0().child(self.files.clone()));

        let count = self.bench.read(cx).atlas().hidden().count();
        let label = match count {
            0 => "Nothing hidden".to_owned(),
            n => format!("{n} hidden"),
        };
        let shell = cx.entity().downgrade();
        let tidy = cx.entity().downgrade();
        let mut status = StatusBar::new()
            .item(
                Action::new("reorganize", Glyph::LayoutDashboard, "Reorganize the graph").on_click(
                    move |window, cx| {
                        tidy.update(cx, |shell, cx| shell.reorganize(window, cx))
                            .ok();
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
            .size_full()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Split::new(self.center.view(), side)),
            )
            .child(status)
    }
}
