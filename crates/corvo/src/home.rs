use corvo_domain::{Uuid, Workspaces};
use corvo_fs::JsonChats;
use corvo_ui::button::Button;
use corvo_ui::list::Row;
use corvo_ui::{field, notice};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Window, px,
};

pub enum Event {
    Enter(Uuid),
}

pub struct Home {
    spaces: Entity<Workspaces>,
    name: Entity<field::State>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for Home {}

impl Home {
    pub fn new(spaces: Entity<Workspaces>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| field::state("New workspace", window, cx));
        let subs = vec![cx.subscribe_in(&name, window, Self::on_name)];
        Self {
            spaces,
            name,
            _subs: subs,
        }
    }

    fn on_name(
        &mut self,
        _: &Entity<field::State>,
        event: &field::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, field::Event::PressEnter { .. }) {
            self.add(window, cx);
        }
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value();
        match self.spaces.update(cx, |spaces, _| spaces.add(&name)) {
            Ok(_) => self
                .name
                .update(cx, |state, cx| state.set_value("", window, cx)),
            Err(err) => notice::error(err.to_string(), window, cx),
        }
        cx.notify();
    }

    // The workspace goes with its chats.
    fn remove(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = self.spaces.update(cx, |spaces, _| spaces.remove(id)) {
            notice::error(err.to_string(), window, cx);
        } else if let Err(err) = JsonChats::config().and_then(|chats| chats.forget(id)) {
            notice::error(format!("chats: {err}"), window, cx);
        }
        cx.notify();
    }
}

impl Render for Home {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let rows = self.spaces.read(cx).items().iter().map(|space| {
            let id = space.id();
            let enter = view.clone();
            let remove = view.clone();
            Row::new(format!("workspace:{id}"), space.name().to_owned())
                .on_click(move |_, cx| {
                    enter.update(cx, |_, cx| cx.emit(Event::Enter(id))).ok();
                })
                .on_remove(move |window, cx| {
                    remove
                        .update(cx, |home, cx| home.remove(id, window, cx))
                        .ok();
                })
        });
        let add = view.clone();

        v_flex().size_full().items_center().justify_center().child(
            v_flex()
                .w(px(360.))
                .gap(px(12.))
                .child(v_flex().text_size(px(28.)).child("corvo"))
                .child(v_flex().children(rows))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(v_flex().flex_1().child(field::view(&self.name)))
                        .child(Button::primary("add").label("Add").on_click(
                            move |_, window, cx| {
                                add.update(cx, |home, cx| home.add(window, cx)).ok();
                            },
                        )),
                ),
        )
    }
}
