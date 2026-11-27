use std::path::PathBuf;

use gpui_kit::{App, Context, IntoElement, Render, Window};

use super::Ctx;
use super::parts::note;
use crate::palette::Offer;

pub struct Worktrees {
    #[allow(dead_code)]
    ctx: Ctx,
    root: Option<PathBuf>,
}

impl Worktrees {
    pub fn new(ctx: Ctx, cx: &mut Context<Self>) -> Self {
        let _ = cx;
        Self { ctx, root: None }
    }

    pub fn show(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        self.root = root;
        cx.notify();
    }

    pub fn offers(&self, _: &str, _: &App) -> Vec<Offer> {
        Vec::new()
    }

    pub fn set_agents(
        &mut self,
        _: gpui_kit::Entity<crate::agents::Agents>,
        _: &mut Context<Self>,
    ) {
    }

    pub fn agent_working(&self, _: &App) -> bool {
        false
    }
}

impl Render for Worktrees {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        note("Soon.", cx)
    }
}
