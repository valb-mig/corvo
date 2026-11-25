use corvo_domain::StepKind;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    App, IntoElement, ParentElement as _, Pixels, RenderOnce, Styled as _, Window, div, px,
};

use crate::language_icon::tint;

// The logos of agents (Simple Icons, CC0, and LobeHub Icons, MIT; see assets/agents/NOTICE.md),
// drawn in their brand color, lifted when too dark to see.
macro_rules! logo {
    ($file:literal) => {
        include_str!(concat!("../assets/agents/", $file, ".svg"))
    };
}

fn logo(agent: &str) -> Option<(&'static str, u32)> {
    match agent {
        "claude" => Some((logo!("claudecode"), 0xD97757)),
        "gemini" => Some((logo!("googlegemini"), 0x8E75B2)),
        "opencode" => Some((logo!("opencode"), 0x000000)),
        "codex" => Some((logo!("codex"), 0x000000)),
        _ => None,
    }
}

// The logo of an agent in its color, on a soft square of it.
#[derive(IntoElement)]
pub struct AgentLogo {
    agent: String,
    size: Pixels,
}

impl AgentLogo {
    pub fn new(agent: impl Into<String>) -> Self {
        Self {
            agent: agent.into(),
            size: px(20.),
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for AgentLogo {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (icon, color) = match logo(&self.agent) {
            Some((svg, hex)) => {
                let color = tint(hex);
                (Icon::empty().data(svg.as_bytes()), color)
            }
            None => (Icon::new(Glyph::Bot), cx.theme().muted_foreground),
        };
        div()
            .flex_shrink_0()
            .size(self.size)
            .rounded(self.size * 0.3)
            .flex()
            .items_center()
            .justify_center()
            .bg(color.opacity(0.16))
            .child(icon.size(self.size * 0.62).text_color(color))
    }
}

// The glyph of what a step does.
pub fn step_glyph(kind: StepKind) -> Glyph {
    match kind {
        StepKind::Read => Glyph::FileText,
        StepKind::Edit => Glyph::Pencil,
        StepKind::Delete => Glyph::Trash,
        StepKind::Move => Glyph::FolderInput,
        StepKind::Search => Glyph::Search,
        StepKind::Run => Glyph::Terminal,
        StepKind::Think => Glyph::Brain,
        StepKind::Fetch => Glyph::Globe,
        StepKind::Other => Glyph::Wrench,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    Same,
    Add,
    Del,
}

// The lines of a change, the part that changed with a few kept lines around it.
pub fn diff_lines(old: &str, new: &str) -> Vec<(Sign, String)> {
    const AROUND: usize = 2;
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut lines = Vec::new();
    for line in &old[head.saturating_sub(AROUND)..head] {
        lines.push((Sign::Same, (*line).to_owned()));
    }
    for line in &old[head..old.len() - tail] {
        lines.push((Sign::Del, (*line).to_owned()));
    }
    for line in &new[head..new.len() - tail] {
        lines.push((Sign::Add, (*line).to_owned()));
    }
    let after = new.len() - tail;
    for line in &new[after..(after + AROUND).min(new.len())] {
        lines.push((Sign::Same, (*line).to_owned()));
    }
    lines
}
