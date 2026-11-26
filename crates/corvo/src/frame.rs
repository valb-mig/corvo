use corvo_domain::{Doc, Uuid, Workbench, Workspaces};
use corvo_fs::LocalFs;
use corvo_syntax::TreeSitter;
use corvo_ui::button::Button;
use corvo_ui::notice;
use corvo_ui::titlebar::TitleBar;
use gpui_kit::component::{IconName, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Window, div,
};

use crate::home::{self, Home};
use crate::shell::Shell;

enum Stage {
    Home(Entity<Home>),
    Shell { id: Uuid, shell: Entity<Shell> },
}

pub struct Frame {
    spaces: Entity<Workspaces>,
    stage: Stage,
    _subs: Vec<Subscription>,
}

impl Frame {
    pub fn new(spaces: Workspaces, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let spaces = cx.new(|_| spaces);
        let home = cx.new(|cx| Home::new(spaces.clone(), window, cx));
        let subs = vec![cx.subscribe_in(&home, window, Self::on_home)];
        Self {
            spaces,
            stage: Stage::Home(home),
            _subs: subs,
        }
    }

    fn on_home(
        &mut self,
        _: &Entity<Home>,
        event: &home::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            home::Event::Enter(id) => self.enter(*id, window, cx),
        }
    }

    fn enter(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let spaces = self.spaces.clone();
        let bench = Workbench::new(Box::new(LocalFs), Box::new(TreeSitter));
        let shell = cx.new(|cx| Shell::new(spaces, id, bench, window, cx));
        self.stage = Stage::Shell { id, shell };
        self._subs.clear();
        cx.notify();
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.leave(window, cx) {
            return;
        }
        let home = cx.new(|cx| Home::new(self.spaces.clone(), window, cx));
        self._subs = vec![cx.subscribe_in(&home, window, Self::on_home)];
        self.stage = Stage::Home(home);
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.leave(window, cx) {
            cx.quit();
        }
    }

    fn leave(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Stage::Shell { shell, .. } = &self.stage else {
            return true;
        };
        let dirty = shell
            .read(cx)
            .bench()
            .read(cx)
            .tabs()
            .docs()
            .iter()
            .filter(|doc| doc.is_dirty())
            .map(Doc::name)
            .collect::<Vec<_>>();
        if dirty.is_empty() {
            return true;
        }
        notice::warn(format!("Unsaved changes: {}", dirty.join(", ")), window, cx);
        false
    }

    fn heading(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match &self.stage {
            Stage::Home(_) => vec![div().child("corvo").into_any_element()],
            Stage::Shell { id, .. } => {
                let name = self
                    .spaces
                    .read(cx)
                    .get(*id)
                    .map(|space| space.name().to_owned())
                    .unwrap_or_default();
                let view = cx.entity().downgrade();
                vec![
                    Button::ghost("back")
                        .icon(IconName::ChevronLeft)
                        .on_click(move |_, window, cx| {
                            view.update(cx, |frame, cx| frame.back(window, cx)).ok();
                        })
                        .into_any_element(),
                    div().child(name).into_any_element(),
                ]
            }
        }
    }
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let body = match &self.stage {
            Stage::Home(home) => home.clone().into_any_element(),
            Stage::Shell { shell, .. } => shell.clone().into_any_element(),
        };

        v_flex()
            .size_full()
            .child(
                TitleBar::new()
                    .on_close(move |window, cx| {
                        view.update(cx, |frame, cx| frame.close(window, cx)).ok();
                    })
                    .children(self.heading(cx)),
            )
            .child(div().flex_1().min_h_0().child(body))
    }
}
