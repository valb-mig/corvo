use std::rc::Rc;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, Icon, v_flex};
use gpui_kit::{
    App, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};

use gpui_kit::component::tooltip::Tooltip;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

// The width of a rail, with its border.
pub const WIDTH: f32 = 49.;

// A screen of a window, as its rail shows it: an icon, a name, a key, and what it counts.
pub struct Stop {
    id: ElementId,
    glyph: Glyph,
    label: SharedString,
    key: Option<SharedString>,
    count: Option<usize>,
    // A dot in its corner, in this color: something going on there.
    dot: Option<Hsla>,
    // It wants the dev: shown in the warning's color, apart from the others.
    alarm: bool,
    on_click: Handler,
}

impl Stop {
    pub fn new(
        id: impl Into<ElementId>,
        glyph: Glyph,
        label: impl Into<SharedString>,
        f: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            glyph,
            label: label.into(),
            key: None,
            count: None,
            dot: None,
            alarm: false,
            on_click: Rc::new(f),
        }
    }

    pub fn key(mut self, key: impl Into<SharedString>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = (count > 0).then_some(count);
        self
    }

    pub fn dot(mut self, color: Option<Hsla>) -> Self {
        self.dot = color;
        self
    }

    pub fn alarm(mut self, alarm: bool) -> Self {
        self.alarm = alarm;
        self
    }
}

// The icons down the left of a window, one per screen it holds, the one shown lit. Only icons:
// a screen's name shows on hover, with its key.
#[derive(IntoElement)]
pub struct Rail {
    stops: Vec<Stop>,
    shown: usize,
}

impl Rail {
    pub fn new(stops: Vec<Stop>, shown: usize) -> Self {
        Self { stops, shown }
    }
}

impl RenderOnce for Rail {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let (accent, warning) = (theme.blue, theme.warning);
        let mut rail = v_flex()
            .w(px(WIDTH))
            .h_full()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .py_2()
            .border_r_1()
            .border_color(theme.border);
        let mut alarmed = false;
        for (ix, stop) in self.stops.into_iter().enumerate() {
            let on = ix == self.shown;
            let tone = if stop.alarm { warning } else { accent };
            if stop.alarm && !alarmed {
                alarmed = true;
                rail = rail.child(div().w(px(20.)).h(px(1.)).my_1().bg(theme.border));
            }
            let muted = theme.muted_foreground;
            let fg = theme.foreground;
            let bg = theme.background;
            let hover = theme.muted;
            let label = stop.label.clone();
            let key = stop.key.clone();
            let click = stop.on_click.clone();
            let badge = stop.count.map(|count| {
                let text = if count > 99 {
                    "99+".to_owned()
                } else {
                    count.to_string()
                };
                div()
                    .absolute()
                    .right(px(0.))
                    .bottom(px(0.))
                    .min_w(px(16.))
                    .h(px(16.))
                    .px(px(4.))
                    .rounded_full()
                    .border_2()
                    .border_color(bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(9.5))
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .when(on || stop.alarm, |b| b.bg(tone).text_color(bg))
                    .when(!(on || stop.alarm), |b| {
                        b.bg(theme.secondary).text_color(fg)
                    })
                    .child(text)
            });
            let dot = stop.dot.map(|color| {
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(5.))
                    .size(px(7.))
                    .rounded_full()
                    .border_1()
                    .border_color(bg)
                    .bg(color)
            });
            let button = div()
                .id(stop.id)
                .relative()
                .size(px(36.))
                .rounded_md()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(if stop.alarm { warning } else { muted })
                .when(!on, |b| b.hover(|b| b.bg(hover).text_color(fg)))
                .when(on, |b| b.bg(tone.opacity(0.14)).text_color(tone))
                .tooltip(move |window, cx| {
                    let text = match &key {
                        Some(key) => format!("{label} ({key})"),
                        None => label.to_string(),
                    };
                    Tooltip::new(text).build(window, cx)
                })
                .on_click(move |_, window, cx| click(window, cx))
                .child(Icon::new(stop.glyph).size(px(18.)))
                .children(badge)
                .children(dot);
            // A bar on the rail's edge marks the screen shown.
            let marker = on.then(|| {
                div()
                    .absolute()
                    .left(px(-6.))
                    .top(px(9.))
                    .w(px(2.))
                    .h(px(18.))
                    .rounded_full()
                    .bg(tone)
            });
            rail = rail.child(div().relative().children(marker).child(button));
        }
        rail
    }
}
