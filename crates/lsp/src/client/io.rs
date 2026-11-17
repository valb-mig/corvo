use std::io::{BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{ChildStdin, ChildStdout, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::Receiver;

use futures::channel::mpsc::UnboundedSender;
use lsp_server::{Message, Notification, Request, RequestId, Response, ResponseError};
use lsp_types::notification::{Initialized, Notification as _, PublishDiagnostics};
use lsp_types::request::{Request as _, WorkspaceConfiguration};
use lsp_types::{
    ConfigurationParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    InitializeResult, InitializedParams, PublishDiagnosticsParams,
};
use serde_json::Value;

use super::gate::{Shared, Waiter};
use crate::note::Note;
use crate::uri;

pub struct Reader {
    pub shared: Arc<Shared>,
    pub notes: UnboundedSender<Note>,
    pub settings: Option<Value>,
    pub server: String,
    pub root: PathBuf,
}

impl Reader {
    pub fn run(self, stdout: ChildStdout) {
        let mut input = BufReader::new(stdout);
        while let Ok(Some(msg)) = Message::read(&mut input) {
            match msg {
                Message::Response(res) if res.id == RequestId::from(0) => self.ready(res),
                Message::Response(res) => self.resolve(res),
                Message::Request(req) => self.answer(req),
                Message::Notification(note) => self.forward(note),
            }
        }
        self.shared.close();
        if self.shared.stopping() {
            return;
        }
        let _ = self.notes.unbounded_send(Note::Exited {
            server: self.server,
            root: self.root,
        });
    }

    fn ready(&self, res: Response) {
        let init = res
            .response_result
            .map_err(|err| err.message)
            .and_then(|value| {
                serde_json::from_value::<InitializeResult>(value).map_err(|err| err.to_string())
            });
        match init {
            Ok(init) => {
                self.shared.raw(Message::Notification(Notification::new(
                    Initialized::METHOD.into(),
                    InitializedParams {},
                )));
                self.shared.open(init.capabilities);
            }
            Err(message) => {
                self.shared.close();
                let _ = self.notes.unbounded_send(Note::Failed {
                    server: self.server.clone(),
                    message,
                });
            }
        }
    }

    fn resolve(&self, res: Response) {
        match self.shared.take(&res.id) {
            Some(Waiter::Reply(tx)) => {
                let _ = tx.send(res.response_result);
            }
            Some(Waiter::Pull {
                path,
                version,
                current,
            }) => self.pulled(path, version, &current, res.response_result),
            None => {}
        }
    }

    fn pulled(
        &self,
        path: PathBuf,
        version: i32,
        current: &AtomicI32,
        result: Result<Value, ResponseError>,
    ) {
        if current.load(Ordering::Relaxed) != version {
            return;
        }
        let report = result
            .ok()
            .and_then(|value| serde_json::from_value::<DocumentDiagnosticReportResult>(value).ok());
        if let Some(DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(full))) =
            report
        {
            let items = full.full_document_diagnostic_report.items;
            let _ = self.notes.unbounded_send(Note::Diagnostics(path, items));
        }
    }

    fn answer(&self, req: Request) {
        let result = if req.method == WorkspaceConfiguration::METHOD {
            self.configuration(&req.params)
        } else {
            Value::Null
        };
        self.shared
            .raw(Message::Response(Response::new_ok(req.id, result)));
    }

    fn configuration(&self, params: &Value) -> Value {
        let Ok(params) = serde_json::from_value::<ConfigurationParams>(params.clone()) else {
            return Value::Null;
        };
        params
            .items
            .iter()
            .map(|item| {
                item.section
                    .as_deref()
                    .and_then(|section| self.settings.as_ref()?.pointer(&pointer(section)).cloned())
                    .unwrap_or(Value::Null)
            })
            .collect()
    }

    fn forward(&self, note: Notification) {
        if note.method != PublishDiagnostics::METHOD {
            return;
        }
        let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(note.params) else {
            return;
        };
        if let Some(path) = uri::to_path(&params.uri) {
            let _ = self
                .notes
                .unbounded_send(Note::Diagnostics(path, params.diagnostics));
        }
    }
}

pub fn write(mut stdin: ChildStdin, queue: Receiver<Message>) {
    for msg in queue {
        if msg.write(&mut stdin).is_err() || stdin.flush().is_err() {
            break;
        }
    }
}

pub fn orphan_guard(cmd: &mut Command) {
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
}

fn pointer(section: &str) -> String {
    format!("/{}", section.replace('.', "/"))
}
