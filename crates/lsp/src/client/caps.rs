use std::path::Path;

use lsp_types::{
    ClientCapabilities, ClientInfo, CompletionClientCapabilities, CompletionItemCapability,
    DiagnosticClientCapabilities, GeneralClientCapabilities, GotoCapability,
    HoverClientCapabilities, InitializeParams, MarkupKind, PositionEncodingKind,
    PublishDiagnosticsClientCapabilities, TextDocumentClientCapabilities,
    TextDocumentSyncClientCapabilities, WorkspaceClientCapabilities, WorkspaceFolder,
};
use serde_json::Value;

use crate::uri;

pub fn initialize(root: &Path, options: Option<Value>) -> InitializeParams {
    let folder = uri::from_path(root).map(|uri| WorkspaceFolder {
        uri,
        name: root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    });

    #[allow(deprecated)]
    InitializeParams {
        process_id: Some(std::process::id()),
        root_uri: folder.as_ref().map(|folder| folder.uri.clone()),
        workspace_folders: folder.map(|folder| vec![folder]),
        initialization_options: options,
        capabilities: client(),
        client_info: Some(ClientInfo {
            name: "corvo".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
        ..Default::default()
    }
}

fn client() -> ClientCapabilities {
    let markup = Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]);
    ClientCapabilities {
        general: Some(GeneralClientCapabilities {
            position_encodings: Some(vec![
                PositionEncodingKind::UTF32,
                PositionEncodingKind::UTF16,
            ]),
            ..Default::default()
        }),
        text_document: Some(TextDocumentClientCapabilities {
            synchronization: Some(TextDocumentSyncClientCapabilities {
                did_save: Some(true),
                ..Default::default()
            }),
            completion: Some(CompletionClientCapabilities {
                completion_item: Some(CompletionItemCapability {
                    snippet_support: Some(false),
                    documentation_format: markup.clone(),
                    ..Default::default()
                }),
                context_support: Some(true),
                ..Default::default()
            }),
            hover: Some(HoverClientCapabilities {
                content_format: markup,
                ..Default::default()
            }),
            definition: Some(GotoCapability {
                link_support: Some(true),
                ..Default::default()
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities::default()),
            diagnostic: Some(DiagnosticClientCapabilities::default()),
            ..Default::default()
        }),
        workspace: Some(WorkspaceClientCapabilities {
            configuration: Some(true),
            workspace_folders: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    }
}
