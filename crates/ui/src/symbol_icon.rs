use corvo_domain::SymbolKind;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    App, Hsla, IntoElement, ParentElement as _, Pixels, RenderOnce, Styled as _, Window, div, px,
};

// The glyph of a symbol of the outline, tinted by what it is: callables, data, constants and the
// containers around them.

#[derive(IntoElement)]
pub struct SymbolIcon {
    kind: SymbolKind,
    size: Pixels,
}

impl SymbolIcon {
    pub fn new(kind: SymbolKind) -> Self {
        Self {
            kind,
            size: px(16.),
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for SymbolIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex_shrink_0()
            .size(self.size)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Icon::new(glyph(self.kind))
                    .size(self.size * (12. / 16.))
                    .text_color(tint(self.kind, cx)),
            )
    }
}

fn tint(kind: SymbolKind, cx: &App) -> Hsla {
    let theme = cx.theme();
    match kind {
        SymbolKind::Function | SymbolKind::Method => theme.magenta,
        SymbolKind::Field | SymbolKind::Variable | SymbolKind::Variant => theme.cyan,
        SymbolKind::Parameter => theme.green,
        SymbolKind::Constant => theme.yellow,
        SymbolKind::Module | SymbolKind::Impl => theme.muted_foreground,
        _ => theme.blue,
    }
}

fn glyph(kind: SymbolKind) -> Glyph {
    match kind {
        SymbolKind::Module => Glyph::Package,
        SymbolKind::Class => Glyph::Box,
        SymbolKind::Struct => Glyph::Braces,
        SymbolKind::Enum => Glyph::ListTree,
        SymbolKind::Interface => Glyph::Component,
        SymbolKind::Impl => Glyph::Blocks,
        SymbolKind::Function => Glyph::SquareFunction,
        SymbolKind::Method => Glyph::Parentheses,
        SymbolKind::Field => Glyph::Tag,
        SymbolKind::Variant => Glyph::Dot,
        SymbolKind::Constant => Glyph::Hash,
        SymbolKind::Variable => Glyph::Variable,
        SymbolKind::Parameter => Glyph::ArrowRightToLine,
        SymbolKind::Type => Glyph::Type,
    }
}
