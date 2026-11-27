use std::rc::Rc;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::{
    AnimationExt as _, App, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, SharedString, Styled as _, Window, div, px,
};

use crate::motion;

// How wide the bar is, and its gap to the edges of the code it floats over.
pub const WIDTH: f32 = 400.;
const INSET: f32 = 8.;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

// A button that stays pressed while what it stands for is on, as matching case.
pub fn toggle(
    id: impl Into<ElementId>,
    glyph: Glyph,
    on: bool,
    tooltip: impl Into<SharedString>,
    f: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .compact()
        .icon(glyph)
        .selected(on)
        .toggled(on)
        .tooltip(tooltip.into())
        .on_click(move |_, window, cx| f(window, cx))
}

// A button with only an icon, small enough for a line of a field.
pub fn icon(
    id: impl Into<ElementId>,
    glyph: Glyph,
    tooltip: impl Into<SharedString>,
    enabled: bool,
    f: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .icon(glyph)
        .disabled(!enabled)
        .tooltip(tooltip.into())
        .on_click(move |_, window, cx| f(window, cx))
}

// A field as the bars of search have it: low, with what toggles how it matches at its end.
pub fn field(state: &Entity<InputState>) -> Input {
    Input::new(state).small().focus_bordered(true).shadow_none()
}

// What the bar shows of the matches: where the current one is among them, or that there is none.
pub enum Count {
    // Nothing typed yet.
    Idle,
    At(usize, usize),
    None,
}

// The bar that finds in the open file, floating at the top right of the code: the query with
// the case toggle at its end, where the current match is among them, the way to the one before
// or after, and, opened by the chevron, a second line to replace them.
#[derive(IntoElement)]
pub struct FindBar {
    query: Entity<InputState>,
    replace: Entity<InputState>,
    case: bool,
    replacing: bool,
    count: Count,
    // How far from the right edge it keeps, so a drawer there leaves it in sight.
    right: gpui_kit::Pixels,
    on_case: Option<Handler>,
    on_toggle: Option<Handler>,
    on_prev: Option<Handler>,
    on_next: Option<Handler>,
    on_replace: Option<Handler>,
    on_replace_all: Option<Handler>,
    on_close: Option<Handler>,
}

impl FindBar {
    pub fn new(query: &Entity<InputState>, replace: &Entity<InputState>) -> Self {
        Self {
            query: query.clone(),
            replace: replace.clone(),
            case: false,
            replacing: false,
            count: Count::Idle,
            right: px(INSET),
            on_case: None,
            on_toggle: None,
            on_prev: None,
            on_next: None,
            on_replace: None,
            on_replace_all: None,
            on_close: None,
        }
    }

    pub fn case(mut self, case: bool) -> Self {
        self.case = case;
        self
    }

    pub fn replacing(mut self, replacing: bool) -> Self {
        self.replacing = replacing;
        self
    }

    pub fn count(mut self, count: Count) -> Self {
        self.count = count;
        self
    }

    // Keeps clear of a drawer this wide on the right.
    pub fn clear_of(mut self, room: gpui_kit::Pixels) -> Self {
        self.right = room + px(INSET);
        self
    }

    pub fn on_case(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_case = Some(Rc::new(f));
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    pub fn on_prev(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_prev = Some(Rc::new(f));
        self
    }

    pub fn on_next(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_next = Some(Rc::new(f));
        self
    }

    pub fn on_replace(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_replace = Some(Rc::new(f));
        self
    }

    pub fn on_replace_all(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_replace_all = Some(Rc::new(f));
        self
    }

    pub fn on_close(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
}

fn run(handler: &Option<Handler>) -> impl Fn(&mut Window, &mut App) + 'static {
    let handler = handler.clone();
    move |window, cx| {
        if let Some(f) = &handler {
            f(window, cx);
        }
    }
}

impl RenderOnce for FindBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let found = matches!(self.count, Count::At(..));
        let (label, color) = match self.count {
            Count::Idle => (SharedString::default(), theme.muted_foreground),
            Count::At(at, of) => (format!("{at} of {of}").into(), theme.muted_foreground),
            Count::None => ("No results".into(), theme.danger),
        };
        let chevron = if self.replacing {
            Glyph::ChevronDown
        } else {
            Glyph::ChevronRight
        };
        let find = h_flex()
            .gap_1()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(field(&self.query).suffix(toggle(
                        "find-case",
                        Glyph::CaseSensitive,
                        self.case,
                        "Match case",
                        run(&self.on_case),
                    ))),
            )
            .child(
                div()
                    .w(px(64.))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(color)
                    .truncate()
                    .child(label),
            )
            .child(icon(
                "find-prev",
                Glyph::ArrowUp,
                "Previous (Shift+Enter)",
                found,
                run(&self.on_prev),
            ))
            .child(icon(
                "find-next",
                Glyph::ArrowDown,
                "Next (Enter)",
                found,
                run(&self.on_next),
            ))
            .child(icon(
                "find-close",
                Glyph::Close,
                "Close (Esc)",
                true,
                run(&self.on_close),
            ));
        let replace = self.replacing.then(|| {
            h_flex()
                .gap_1()
                .child(div().flex_1().min_w_0().child(field(&self.replace)))
                // As wide as the count above, so both fields end on the same line.
                .child(div().w(px(64.)).flex_shrink_0())
                .child(icon(
                    "find-replace",
                    Glyph::Replace,
                    "Replace",
                    found,
                    run(&self.on_replace),
                ))
                .child(icon(
                    "find-replace-all",
                    Glyph::ReplaceAll,
                    "Replace all",
                    found,
                    run(&self.on_replace_all),
                ))
                // The room the close button takes on the line above.
                .child(div().w(px(22.)).flex_shrink_0())
        });

        h_flex()
            .id("find-bar")
            .absolute()
            .top(px(INSET))
            .right(self.right)
            .w(px(WIDTH))
            .items_start()
            .gap_1()
            .p_1()
            .occlude()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .shadow_lg()
            .child(icon(
                "find-toggle",
                chevron,
                "Replace (Ctrl+H)",
                true,
                run(&self.on_toggle),
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(find)
                    .children(replace),
            )
            .with_animation("find-bar", motion::opening(), |bar, delta| {
                bar.top(px(INSET - 6. * (1. - delta))).opacity(delta)
            })
    }
}
