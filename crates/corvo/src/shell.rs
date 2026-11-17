use corvo_domain::Workbench;
use corvo_domain::server::Registry;
use corvo_lsp::{Hub, Note};
use corvo_ui::keys::Save;
use corvo_ui::layout::Split;
use corvo_ui::notice;
use futures::StreamExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};

use crate::pane::code::Code;
use crate::pane::files::{self, Files};

pub struct Shell {
    focus: FocusHandle,
    hub: Entity<Hub>,
    code: Entity<Code>,
    files: Entity<Files>,
    _subs: Vec<Subscription>,
}

impl Shell {
    pub fn new(bench: Workbench, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (hub, mut notes) = Hub::new(Registry::default(), bench.tree().root());
        let hub = cx.new(|_| hub);
        let bench = cx.new(|_| bench);
        let code = cx.new(|_| Code::new(bench.clone(), hub.clone()));
        let files = cx.new(|cx| Files::new(bench, window, cx));
        let subs = vec![cx.subscribe_in(&files, window, Self::on_files)];

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

        Self {
            focus,
            hub,
            code,
            files,
            _subs: subs,
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
            files::Event::Pick(path) => {
                self.code.update(cx, |code, cx| code.open(path, window, cx));
            }
        }
    }

    fn on_note(&mut self, note: Note, window: &mut Window, cx: &mut Context<Self>) {
        match note {
            Note::Diagnostics(path, items) => {
                self.code
                    .update(cx, |code, cx| code.diagnose(&path, items, cx));
            }
            Note::Missing { server, hint } => {
                notice::warn(format!("{server} not found. Install: {hint}"), window, cx);
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

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::save))
            .size_full()
            .child(Split::new(self.code.clone(), self.files.clone()))
    }
}
