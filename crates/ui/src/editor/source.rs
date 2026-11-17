use std::rc::Rc;

use anyhow::Result;
use gpui_kit::component::input::{
    CompletionProvider, DefinitionProvider, EditorState, HoverProvider, Rope, RopeExt as _,
};
use gpui_kit::{App, Task, Window};
use lsp_types::{
    CompletionContext, CompletionResponse, Hover, LocationLink, Position, ShowDocumentParams,
};

pub trait Source {
    fn complete(
        &self,
        pos: Position,
        context: CompletionContext,
        cx: &mut App,
    ) -> Task<Result<CompletionResponse>>;

    fn hover(&self, pos: Position, cx: &mut App) -> Task<Result<Option<Hover>>>;

    fn definition(&self, pos: Position, cx: &mut App) -> Task<Result<Vec<LocationLink>>>;

    fn triggers(&self, cx: &App) -> Vec<String>;

    fn reveal(&self, target: &ShowDocumentParams, window: &mut Window, cx: &mut App) -> bool;
}

struct Provider(Rc<dyn Source>);

impl CompletionProvider for Provider {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        trigger: CompletionContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        self.0
            .complete(text.offset_to_position(offset), trigger, cx)
    }

    fn is_completion_trigger(&self, _: usize, new_text: &str, cx: &mut App) -> bool {
        let word = new_text
            .chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        word || self
            .0
            .triggers(cx)
            .iter()
            .any(|trigger| new_text.ends_with(trigger.as_str()))
    }
}

impl HoverProvider for Provider {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Option<Hover>>> {
        self.0.hover(text.offset_to_position(offset), cx)
    }
}

impl DefinitionProvider for Provider {
    fn definitions(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Vec<LocationLink>>> {
        self.0.definition(text.offset_to_position(offset), cx)
    }
}

pub fn attach(state: &mut EditorState, source: Rc<dyn Source>) {
    let provider = Rc::new(Provider(source.clone()));
    let lsp = state.lsp_mut();
    lsp.completion_provider = Some(provider.clone());
    lsp.hover_provider = Some(provider.clone());
    lsp.definition_provider = Some(provider);
    lsp.show_document = Some(Rc::new(move |params, window, cx| {
        source.reveal(params, window, cx)
    }));
}
