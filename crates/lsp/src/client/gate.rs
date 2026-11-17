use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

use futures::channel::oneshot;
use lsp_server::{Message, Request, RequestId, ResponseError};
use lsp_types::request::{DocumentDiagnosticRequest, Request as _};
use lsp_types::{DiagnosticServerCapabilities, ServerCapabilities, Uri};
use serde_json::{Value, json};

pub enum Waiter {
    Reply(oneshot::Sender<Result<Value, ResponseError>>),
    Pull {
        path: PathBuf,
        version: i32,
        current: Arc<AtomicI32>,
    },
}

pub struct Pull {
    pub uri: Uri,
    pub path: PathBuf,
    pub current: Arc<AtomicI32>,
}

#[derive(Default)]
struct Gate {
    open: bool,
    queue: Vec<Message>,
    pulls: Vec<Pull>,
}

pub struct Shared {
    out: Sender<Message>,
    gate: Mutex<Gate>,
    pending: Mutex<HashMap<RequestId, Waiter>>,
    caps: Mutex<Option<ServerCapabilities>>,
    next: AtomicI32,
}

impl Shared {
    pub fn new(out: Sender<Message>) -> Self {
        Self {
            out,
            gate: Mutex::default(),
            pending: Mutex::default(),
            caps: Mutex::default(),
            next: AtomicI32::new(1),
        }
    }

    pub fn raw(&self, msg: Message) {
        let _ = self.out.send(msg);
    }

    pub fn send(&self, msg: Message) {
        let mut gate = lock(&self.gate);
        if gate.open {
            self.raw(msg);
        } else {
            gate.queue.push(msg);
        }
    }

    pub fn id(&self) -> RequestId {
        RequestId::from(self.next.fetch_add(1, Ordering::Relaxed))
    }

    pub fn open(&self, caps: ServerCapabilities) {
        *lock(&self.caps) = Some(caps);
        let pulls = {
            let mut gate = lock(&self.gate);
            gate.open = true;
            for msg in gate.queue.drain(..) {
                let _ = self.out.send(msg);
            }
            std::mem::take(&mut gate.pulls)
        };
        for pull in pulls {
            self.pull(pull);
        }
    }

    pub fn pull(&self, pull: Pull) {
        {
            let mut gate = lock(&self.gate);
            if !gate.open {
                gate.pulls.push(pull);
                return;
            }
        }
        let Some(identifier) = self.identifier() else {
            return;
        };
        let id = self.id();
        let mut params = json!({ "textDocument": { "uri": pull.uri } });
        if let Some(identifier) = identifier {
            params["identifier"] = json!(identifier);
        }
        self.wait(
            id.clone(),
            Waiter::Pull {
                path: pull.path,
                version: pull.current.load(Ordering::Relaxed),
                current: pull.current,
            },
        );
        self.raw(Message::Request(Request::new(
            id,
            DocumentDiagnosticRequest::METHOD.into(),
            params,
        )));
    }

    fn identifier(&self) -> Option<Option<String>> {
        match lock(&self.caps).as_ref()?.diagnostic_provider.as_ref()? {
            DiagnosticServerCapabilities::Options(options) => Some(options.identifier.clone()),
            DiagnosticServerCapabilities::RegistrationOptions(options) => {
                Some(options.diagnostic_options.identifier.clone())
            }
        }
    }

    pub fn wait(&self, id: RequestId, waiter: Waiter) {
        lock(&self.pending).insert(id, waiter);
    }

    pub fn take(&self, id: &RequestId) -> Option<Waiter> {
        lock(&self.pending).remove(id)
    }

    pub fn close(&self) {
        lock(&self.pending).clear();
        let mut gate = lock(&self.gate);
        gate.queue.clear();
        gate.pulls.clear();
    }

    pub fn caps(&self) -> MutexGuard<'_, Option<ServerCapabilities>> {
        lock(&self.caps)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
