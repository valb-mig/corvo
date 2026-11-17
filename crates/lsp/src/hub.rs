use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use corvo_domain::Lang;
use corvo_domain::server::{Registry, root};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
};
use lsp_types::request::{Completion, GotoDefinition, HoverRequest};
use lsp_types::{
    CompletionContext, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, LocationLink, Position,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, Uri, VersionedTextDocumentIdentifier,
};

use crate::{Client, Note, lang, uri};

type Key = (String, PathBuf);

struct Track {
    key: Key,
    uri: Uri,
    version: Arc<AtomicI32>,
}

pub struct Hub {
    registry: Registry,
    workspace: PathBuf,
    clients: HashMap<Key, Arc<Client>>,
    docs: HashMap<PathBuf, Track>,
    missing: HashSet<String>,
    notes: UnboundedSender<Note>,
}

impl Hub {
    pub fn new(
        registry: Registry,
        workspace: impl Into<PathBuf>,
    ) -> (Self, UnboundedReceiver<Note>) {
        let (notes, rx) = unbounded();
        let hub = Self {
            registry,
            workspace: workspace.into(),
            clients: HashMap::new(),
            docs: HashMap::new(),
            missing: HashSet::new(),
            notes,
        };
        (hub, rx)
    }

    pub fn open(&mut self, path: &Path, lang: Lang, text: &str) {
        let Some(uri) = uri::from_path(path) else {
            return;
        };
        let Some((key, client)) = self.client(path, lang) else {
            return;
        };
        client.notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: lang::id(lang).into(),
                version: 0,
                text: text.to_owned(),
            },
        });
        let version = Arc::new(AtomicI32::new(0));
        client.pull(&uri, path, &version);
        self.docs
            .insert(path.to_path_buf(), Track { key, uri, version });
    }

    pub fn change(
        &mut self,
        path: &Path,
        changes: Vec<TextDocumentContentChangeEvent>,
        full: impl FnOnce() -> String,
    ) {
        let Some(track) = self.docs.get_mut(path) else {
            return;
        };
        let Some(client) = self.clients.get(&track.key) else {
            return;
        };
        let version = track.version.fetch_add(1, Ordering::Relaxed) + 1;
        let content_changes = if client.incremental() {
            changes
        } else {
            vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: full(),
            }]
        };
        client.notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: track.uri.clone(),
                version,
            },
            content_changes,
        });
        client.pull(&track.uri, path, &track.version);
    }

    pub fn save(&self, path: &Path) {
        if let Some((track, client)) = self.tracked(path) {
            client.notify::<DidSaveTextDocument>(DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: track.uri.clone(),
                },
                text: None,
            });
            client.pull(&track.uri, path, &track.version);
        }
    }

    pub fn close(&mut self, path: &Path) {
        if let Some((track, client)) = self.tracked(path) {
            client.notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier {
                    uri: track.uri.clone(),
                },
            });
        }
        self.docs.remove(path);
    }

    pub fn complete(
        &self,
        path: &Path,
        pos: Position,
        context: Option<CompletionContext>,
    ) -> Option<impl Future<Output = anyhow::Result<Option<CompletionResponse>>> + use<>> {
        let (track, client) = self.tracked(path)?;
        Some(client.request::<Completion>(CompletionParams {
            text_document_position: at(&track.uri, pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context,
        }))
    }

    pub fn hover(
        &self,
        path: &Path,
        pos: Position,
    ) -> Option<impl Future<Output = anyhow::Result<Option<Hover>>> + use<>> {
        let (track, client) = self.tracked(path)?;
        Some(client.request::<HoverRequest>(HoverParams {
            text_document_position_params: at(&track.uri, pos),
            work_done_progress_params: Default::default(),
        }))
    }

    pub fn definition(
        &self,
        path: &Path,
        pos: Position,
    ) -> Option<impl Future<Output = anyhow::Result<Vec<LocationLink>>> + use<>> {
        let (track, client) = self.tracked(path)?;
        let request = client.request::<GotoDefinition>(GotoDefinitionParams {
            text_document_position_params: at(&track.uri, pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        });
        Some(async move { Ok(request.await?.map(links).unwrap_or_default()) })
    }

    pub fn triggers(&self, path: &Path) -> Vec<String> {
        self.tracked(path)
            .map(|(_, client)| client.triggers())
            .unwrap_or_default()
    }

    pub fn reap(&mut self) {
        let used: HashSet<Key> = self.docs.values().map(|track| track.key.clone()).collect();
        self.clients.retain(|key, _| used.contains(key));
    }

    pub fn exited(&mut self, server: &str, root: &Path) {
        let key = (server.to_owned(), root.to_path_buf());
        self.clients.remove(&key);
        self.docs.retain(|_, track| track.key != key);
    }

    fn client(&mut self, path: &Path, lang: Lang) -> Option<(Key, Arc<Client>)> {
        let spec = self.registry.find(lang)?;
        let root = root(path, spec.root_markers(), &self.workspace);
        let key = (spec.name().to_owned(), root);
        if let Some(client) = self.clients.get(&key) {
            return Some((key, client.clone()));
        }
        match Client::spawn(spec, &key.1, self.notes.clone()) {
            Ok(client) => {
                let client = Arc::new(client);
                self.clients.insert(key.clone(), client.clone());
                Some((key, client))
            }
            Err(_) => {
                if self.missing.insert(spec.name().to_owned()) {
                    let _ = self.notes.unbounded_send(Note::Missing {
                        server: spec.command().to_owned(),
                        hint: spec.install_hint().to_owned(),
                    });
                }
                None
            }
        }
    }

    fn tracked(&self, path: &Path) -> Option<(&Track, &Arc<Client>)> {
        let track = self.docs.get(path)?;
        let client = self.clients.get(&track.key)?;
        Some((track, client))
    }
}

fn at(uri: &Uri, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        position,
    }
}

fn links(response: GotoDefinitionResponse) -> Vec<LocationLink> {
    let link = |location: lsp_types::Location| LocationLink {
        origin_selection_range: None,
        target_uri: location.uri,
        target_range: location.range,
        target_selection_range: location.range,
    };
    match response {
        GotoDefinitionResponse::Scalar(location) => vec![link(location)],
        GotoDefinitionResponse::Array(locations) => locations.into_iter().map(link).collect(),
        GotoDefinitionResponse::Link(links) => links,
    }
}
