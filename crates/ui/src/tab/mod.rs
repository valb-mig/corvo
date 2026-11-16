use std::rc::Rc;

use gpui_kit::component::IconName;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{
    App, ElementId, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window,
    div,
};

use crate::button::Button;

type Pick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(Clone)]
pub struct Item {
    id: SharedString,
    label: SharedString,
    dirty: bool,
}

impl Item {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, dirty: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            dirty,
        }
    }
}

#[derive(IntoElement)]
pub struct Bar {
    id: ElementId,
    items: Vec<Item>,
    active: Option<usize>,
    on_pick: Option<Pick>,
    on_close: Option<Pick>,
}

impl Bar {
    pub fn new(id: impl Into<ElementId>, items: Vec<Item>) -> Self {
        Self {
            id: id.into(),
            items,
            active: None,
            on_pick: None,
            on_close: None,
        }
    }

    pub fn active(mut self, ix: Option<usize>) -> Self {
        self.active = ix;
        self
    }

    pub fn on_pick(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_pick = Some(Rc::new(f));
        self
    }

    pub fn on_close(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Bar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let warning = cx.theme().warning;
        let on_close = self.on_close;

        let tabs = self.items.into_iter().enumerate().map(|(ix, item)| {
            let close = on_close.clone();
            let tab = Tab::new().label(item.label).suffix(
                Button::ghost(ElementId::Name(format!("close:{}", item.id).into()))
                    .icon(IconName::Close)
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        if let Some(close) = &close {
                            close(ix, window, cx);
                        }
                    }),
            );
            if item.dirty {
                tab.prefix(div().text_color(warning).child("●"))
            } else {
                tab
            }
        });

        let mut bar = TabBar::new(self.id).small().menu(true).children(tabs);
        if let Some(ix) = self.active {
            bar = bar.selected_index(ix);
        }
        if let Some(pick) = self.on_pick {
            bar = bar.on_click(move |ix, window, cx| pick(*ix, window, cx));
        }
        h_flex().w_full().child(bar)
    }
}
