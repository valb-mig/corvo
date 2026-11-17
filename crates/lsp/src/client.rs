mod caps;
mod gate;
mod io;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicI32;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use corvo_domain::server::Spec;
use futures::channel::mpsc::UnboundedSender;
use futures::channel::oneshot;
use lsp_server::{Message, Notification, Request, RequestId};
use lsp_types::notification::{Exit, Notification as _};
use lsp_types::request::{Initialize, Request as _, Shutdown};
use lsp_types::{ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri};

use crate::note::Note;
use crate::{cache, profile};

use gate::{Pull, Shared, Waiter};

pub struct Client {
    server: String,
    root: PathBuf,
    child: Option<Child>,
    shared: Arc<Shared>,
}

impl Client {
    pub fn spawn(spec: &Spec, root: &Path, notes: UnboundedSender<Note>) -> std::io::Result<Self> {
        let profile = profile::of(spec.name());
        let mut cmd = Command::new(spec.command());
        cmd.args(spec.arguments())
            .args(profile.args(root))
            .envs(profile.env())
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log(spec.name()));
        io::orphan_guard(&mut cmd);

        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(broken)?;
        let stdout = child.stdout.take().ok_or_else(broken)?;

        let (out, queue) = mpsc::channel();
        let shared = Arc::new(Shared::new(out));
        thread::spawn(move || io::write(stdin, queue));

        let init = caps::initialize(root, profile.options());
        shared.raw(Message::Request(Request::new(
            RequestId::from(0),
            Initialize::METHOD.into(),
            init,
        )));

        let reader = io::Reader {
            shared: shared.clone(),
            notes,
            settings: profile.settings(),
            server: spec.name().to_owned(),
            root: root.to_path_buf(),
        };
        thread::spawn(move || reader.run(stdout));

        Ok(Self {
            server: spec.name().to_owned(),
            root: root.to_path_buf(),
            child: Some(child),
            shared,
        })
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn request<R>(
        &self,
        params: R::Params,
    ) -> impl Future<Output = anyhow::Result<R::Result>> + use<R>
    where
        R: lsp_types::request::Request,
    {
        let id = self.shared.id();
        let (tx, rx) = oneshot::channel();
        self.shared.wait(id.clone(), Waiter::Reply(tx));
        self.shared
            .send(Message::Request(Request::new(id, R::METHOD.into(), params)));
        async move {
            let value = rx
                .await
                .map_err(|_| anyhow!("language server stopped"))?
                .map_err(|err| anyhow!(err.message))?;
            Ok(serde_json::from_value(value)?)
        }
    }

    pub fn notify<N>(&self, params: N::Params)
    where
        N: lsp_types::notification::Notification,
    {
        self.shared.send(Message::Notification(Notification::new(
            N::METHOD.into(),
            params,
        )));
    }

    pub fn pull(&self, uri: &Uri, path: &Path, current: &Arc<AtomicI32>) {
        self.shared.pull(Pull {
            uri: uri.clone(),
            path: path.to_path_buf(),
            current: current.clone(),
        });
    }

    pub fn incremental(&self) -> bool {
        self.caps(|caps| match &caps.text_document_sync {
            Some(TextDocumentSyncCapability::Kind(kind)) => {
                *kind == TextDocumentSyncKind::INCREMENTAL
            }
            Some(TextDocumentSyncCapability::Options(options)) => {
                options.change == Some(TextDocumentSyncKind::INCREMENTAL)
            }
            None => false,
        })
        .unwrap_or(false)
    }

    pub fn triggers(&self) -> Vec<String> {
        self.caps(|caps| {
            caps.completion_provider
                .as_ref()
                .and_then(|provider| provider.trigger_characters.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn caps<T>(&self, f: impl FnOnce(&ServerCapabilities) -> T) -> Option<T> {
        self.shared.caps().as_ref().map(f)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let id = self.shared.id();
        self.shared.raw(Message::Request(Request::new(
            id,
            Shutdown::METHOD.into(),
            (),
        )));
        self.shared.raw(Message::Notification(Notification::new(
            Exit::METHOD.into(),
            (),
        )));
        if let Some(mut child) = self.child.take() {
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(2));
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

fn log(server: &str) -> Stdio {
    let dir = cache::dir().join("lsp");
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::File::create(dir.join(format!("{server}.log"))))
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null())
}

fn broken() -> std::io::Error {
    std::io::Error::other("language server pipe unavailable")
}
