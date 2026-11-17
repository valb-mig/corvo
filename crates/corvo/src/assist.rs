use std::path::PathBuf;

use anyhow::Result;
use corvo_lsp::{Hub, uri};
use corvo_ui::editor::Source;
use gpui_kit::{App, Entity, Task, WeakEntity, Window};
use lsp_types::{
    CompletionContext, CompletionResponse, Hover, LocationLink, Position, ShowDocumentParams,
};

use crate::pane::code::Code;

pub struct Assist {
    hub: Entity<Hub>,
    code: WeakEntity<Code>,
    path: PathBuf,
}

impl Assist {
    pub fn new(hub: Entity<Hub>, code: WeakEntity<Code>, path: PathBuf) -> Self {
        Self { hub, code, path }
    }
}

impl Source for Assist {
    fn complete(
        &self,
        pos: Position,
        context: CompletionContext,
        cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let request = self.hub.read(cx).complete(&self.path, pos, Some(context));
        cx.spawn(async move |_| {
            let response = match request {
                Some(request) => request.await?,
                None => None,
            };
            Ok(response.unwrap_or(CompletionResponse::Array(Vec::new())))
        })
    }

    fn hover(&self, pos: Position, cx: &mut App) -> Task<Result<Option<Hover>>> {
        let request = self.hub.read(cx).hover(&self.path, pos);
        cx.spawn(async move |_| match request {
            Some(request) => request.await,
            None => Ok(None),
        })
    }

    fn definition(&self, pos: Position, cx: &mut App) -> Task<Result<Vec<LocationLink>>> {
        let request = self.hub.read(cx).definition(&self.path, pos);
        cx.spawn(async move |_| match request {
            Some(request) => request.await,
            None => Ok(Vec::new()),
        })
    }

    fn triggers(&self, cx: &App) -> Vec<String> {
        self.hub.read(cx).triggers(&self.path)
    }

    fn reveal(&self, target: &ShowDocumentParams, window: &mut Window, cx: &mut App) -> bool {
        let Some(path) = uri::to_path(&target.uri) else {
            return false;
        };
        if path == self.path {
            return false;
        }
        let at = target.selection.map(|range| range.start);
        self.code
            .update(cx, |code, cx| code.reveal(&path, at, window, cx))
            .is_ok()
    }
}
