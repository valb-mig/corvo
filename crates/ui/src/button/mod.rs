mod kind;

use std::rc::Rc;

use gpui_kit::component::IconName;
use gpui_kit::component::button::Button as Base;
use gpui_kit::{App, ClickEvent, ElementId, IntoElement, RenderOnce, SharedString, Window};

pub use kind::{Ghost, Kind};

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    kind: Rc<dyn Kind>,
    icon: Option<IconName>,
    label: Option<SharedString>,
    on_click: Option<Handler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, kind: impl Kind + 'static) -> Self {
        Self {
            id: id.into(),
            kind: Rc::new(kind),
            icon: None,
            label: None,
            on_click: None,
        }
    }

    pub fn ghost(id: impl Into<ElementId>) -> Self {
        Self::new(id, Ghost)
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut base = Base::new(self.id);
        if let Some(icon) = self.icon {
            base = base.icon(icon);
        }
        if let Some(label) = self.label {
            base = base.label(label);
        }
        if let Some(f) = self.on_click {
            base = base.on_click(move |event, window, cx| f(event, window, cx));
        }
        self.kind.style(base)
    }
}
