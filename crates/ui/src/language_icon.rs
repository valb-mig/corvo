use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    App, Hsla, IntoElement, Pixels, RenderOnce, SharedString, Styled as _, Window, px, rgb,
};

// The logos of languages, from Simple Icons (CC0; see assets/languages/NOTICE.md), drawn in their
// brand color. A brand color too dark to see on a dark background is lifted.

macro_rules! logo {
    ($file:literal) => {
        include_str!(concat!("../assets/languages/", $file, ".svg"))
    };
}

const LOGOS: &[(&str, &str, u32)] = &[
    ("Rust", logo!("rust"), 0x000000),
    ("TypeScript", logo!("typescript"), 0x3178C6),
    ("JavaScript", logo!("javascript"), 0xF7DF1E),
    ("Python", logo!("python"), 0x3776AB),
    ("Java", logo!("openjdk"), 0x000000),
    ("Kotlin", logo!("kotlin"), 0x7F52FF),
    ("PHP", logo!("php"), 0x777BB4),
    ("Go", logo!("go"), 0x00ADD8),
    ("Ruby", logo!("ruby"), 0xCC342D),
    ("Swift", logo!("swift"), 0xF05138),
    ("Dart", logo!("dart"), 0x0175C2),
    ("C", logo!("c"), 0xA8B9CC),
    ("C++", logo!("cplusplus"), 0x00599C),
    ("C#", logo!("dotnet"), 0x512BD4),
    ("Vue", logo!("vuedotjs"), 0x4FC08D),
    ("Svelte", logo!("svelte"), 0xFF3E00),
    ("Astro", logo!("astro"), 0xBC52EE),
    ("HTML", logo!("html5"), 0xE34F26),
    ("CSS", logo!("css"), 0x663399),
    ("Shell", logo!("gnubash"), 0x4EAA25),
    ("Lua", logo!("lua"), 0x000080),
    ("SQL", logo!("sqlite"), 0x003B57),
    ("Elixir", logo!("elixir"), 0x4B275F),
    ("Zig", logo!("zig"), 0xF7A41D),
    ("Scala", logo!("scala"), 0xDC322F),
    ("Haskell", logo!("haskell"), 0x5D4F85),
];

const DARK: f32 = 0.35;

// The logo and brand color of a language, by the name corvo gives it.
pub fn logo(name: &str) -> Option<(&'static str, u32)> {
    LOGOS
        .iter()
        .find(|(known, _, _)| *known == name)
        .map(|&(_, svg, hex)| (svg, hex))
}

// The brand color, lifted when too dark to see.
pub fn tint(hex: u32) -> Hsla {
    let color: Hsla = rgb(hex).into();
    if color.l < DARK {
        Hsla { l: 0.7, ..color }
    } else {
        color
    }
}

#[derive(IntoElement)]
pub struct LanguageIcon {
    name: SharedString,
    size: Pixels,
}

impl LanguageIcon {
    pub fn new(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            size: px(16.),
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for LanguageIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let icon = match logo(&self.name) {
            Some((svg, hex)) => Icon::empty().data(svg.as_bytes()).text_color(tint(hex)),
            None => Icon::new(Glyph::Code).text_color(cx.theme().muted_foreground),
        };
        icon.size(self.size).flex_shrink_0()
    }
}
