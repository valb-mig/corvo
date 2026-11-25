use std::rc::Rc;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::component::{Icon, IconName};
use gpui_kit::{
    App, ElementId, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window,
    div, px,
};

use crate::button::Button;
use crate::file_icon::FileIcon;

type Pick = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type Home = Rc<dyn Fn(&mut Window, &mut App)>;

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
    home: Option<(bool, Home)>,
    on_pick: Option<Pick>,
    on_close: Option<Pick>,
}

impl Bar {
    pub fn new(id: impl Into<ElementId>, items: Vec<Item>) -> Self {
        Self {
            id: id.into(),
            items,
            active: None,
            home: None,
            on_pick: None,
            on_close: None,
        }
    }

    pub fn active(mut self, ix: Option<usize>) -> Self {
        self.active = ix;
        self
    }

    // A pinned first tab, with no label and no close, that is not one of the items.
    pub fn home(mut self, on: bool, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.home = Some((on, Rc::new(f)));
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
            let name = item.label.clone();
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
            tab.prefix(
                h_flex()
                    .gap_1()
                    .child(FileIcon::new(name).size(px(14.)))
                    .children(item.dirty.then(|| div().text_color(warning).child("●"))),
            )
        });

        let (home, go) = match self.home {
            Some((on, go)) => (Some(on), Some(go)),
            None => (None, None),
        };
        let shift = usize::from(home.is_some());
        let pinned = home.map(|_| Tab::new().prefix(Icon::new(Glyph::House).small()));
        let selected = match home {
            Some(true) => Some(0),
            _ => self.active.map(|ix| ix + shift),
        };

        let mut bar = TabBar::new(self.id).small().children(pinned).children(tabs);
        if let Some(ix) = selected {
            bar = bar.selected_index(ix);
        }
        let pick = self.on_pick;
        bar = bar.on_click(move |ix, window, cx| match (ix.checked_sub(shift), &go) {
            (None, Some(go)) => go(window, cx),
            (Some(ix), _) => {
                if let Some(pick) = &pick {
                    pick(ix, window, cx);
                }
            }
            (None, None) => {}
        });
        h_flex().w_full().child(bar)
    }
}
