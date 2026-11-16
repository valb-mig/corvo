use corvo_domain::Workbench;
use corvo_ui::keys::Save;
use corvo_ui::layout::Split;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};

use crate::pane::code::Code;
use crate::pane::files::{self, Files};

pub struct Shell {
    focus: FocusHandle,
    code: Entity<Code>,
    files: Entity<Files>,
    _subs: Vec<Subscription>,
}

impl Shell {
    pub fn new(bench: Workbench, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let bench = cx.new(|_| bench);
        let code = cx.new(|_| Code::new(bench.clone()));
        let files = cx.new(|cx| Files::new(bench, window, cx));
        let subs = vec![cx.subscribe_in(&files, window, Self::on_files)];

        let focus = cx.focus_handle();
        focus.focus(window, cx);

        Self {
            focus,
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
