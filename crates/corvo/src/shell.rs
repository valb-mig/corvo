use std::path::{Path, PathBuf};

use corvo_domain::server::Registry;
use corvo_domain::{Workbench, Workspaces};
use corvo_lsp::{Hub, Note};
use corvo_ui::dropdown::Dropdown;
use corvo_ui::keys::Save;
use corvo_ui::layout::Split;
use corvo_ui::notice;
use futures::StreamExt as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyView, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};

use crate::pane::code::{self, Code};
use crate::pane::files::{self, Files};
use crate::pane::projects::{self, Projects};

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
    projects: Entity<Projects>,
    menu: bool,
    files: Entity<Files>,
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
        let (hub, mut notes) = Hub::new(Registry::default());
        let hub = cx.new(|_| hub);
        let bench = cx.new(|_| bench);
        let code = cx.new(|_| Code::new(bench.clone(), hub.clone()));
        let projects = cx.new(|_| Projects::new(spaces.clone(), id, bench.clone()));
        let files = cx.new(|cx| Files::new(bench.clone(), window, cx));
        let subs = vec![
            cx.subscribe_in(&projects, window, Self::on_projects),
            cx.subscribe_in(&files, window, Self::on_files),
            cx.subscribe_in(&code, window, Self::on_code),
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
            projects,
            menu: false,
            files,
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
        self.projects.update(cx, |_, cx| cx.notify());
    }

    fn rescope(&mut self, cx: &mut Context<Self>) {
        let linked = self.linked(cx);
        let active = self.bench.read(cx).project().map(Path::to_path_buf);
        self.hub.update(cx, |hub, _| hub.scope(linked, active));
    }

    fn on_projects(
        &mut self,
        _: &Entity<Projects>,
        event: &projects::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            projects::Event::Show(path) | projects::Event::Linked(path) => {
                self.menu = false;
                self.show(Some(path.clone()), window, cx);
            }
            projects::Event::Unlinked(path) => {
                self.bench.update(cx, |bench, _| bench.forget(path));
                let next = match self.bench.read(cx).project() {
                    Some(project) => Some(project.to_path_buf()),
                    None => self.linked(cx).into_iter().next(),
                };
                self.show(next, window, cx);
            }
        }
    }

    fn on_files(
        &mut self,
        _: &Entity<Files>,
        event: &files::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            files::Event::Pick(path) => match &self.center {
                Center::Code(code) => code.update(cx, |code, cx| code.open(path, window, cx)),
            },
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

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let side = v_flex()
            .size_full()
            .bg(cx.theme().sidebar)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .px_1()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Dropdown::new("projects", "Projects", self.projects.clone())
                            .open(self.menu)
                            .on_toggle(move |open, _, cx| {
                                view.update(cx, |shell, cx| {
                                    shell.menu = open;
                                    cx.notify();
                                })
                                .ok();
                            }),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.files.clone()));

        div()
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::save))
            .size_full()
            .child(Split::new(self.center.view(), side))
    }
}
