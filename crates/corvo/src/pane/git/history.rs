use std::path::PathBuf;

use gpui_kit::{App, Context, IntoElement, Render, Window};

use super::Ctx;
use super::parts::note;
use crate::palette::Offer;

pub struct History {
    #[allow(dead_code)]
    ctx: Ctx,
    root: Option<PathBuf>,
}

impl History {
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
}

impl Render for History {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        note("Soon.", cx)
    }
}
