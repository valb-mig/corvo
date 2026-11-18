use std::rc::Rc;

use gpui_kit::component::IconName;
use gpui_kit::component::list::ListItem;
use gpui_kit::{App, ElementId, IntoElement, ParentElement as _, RenderOnce, SharedString, Window};

use crate::button::Button;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Row {
    id: SharedString,
    label: SharedString,
    selected: bool,
    on_click: Option<Handler>,
    on_remove: Option<Handler>,
}

impl Row {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            selected: false,
            on_click: None,
            on_remove: None,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    pub fn on_remove(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Row {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut item = ListItem::new(ElementId::Name(self.id.clone()))
            .selected(self.selected)
            .child(self.label);
        if let Some(f) = self.on_click {
            item = item.on_click(move |_, window, cx| f(window, cx));
        }
        if let Some(f) = self.on_remove {
            let id = ElementId::Name(format!("remove:{}", self.id).into());
            item = item.suffix(move |_, _| {
                let f = f.clone();
                Button::ghost(id.clone())
                    .icon(IconName::Close)
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        f(window, cx);
                    })
            });
        }
        item
    }
}
