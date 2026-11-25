use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::thread;

use serde_json::{Value, json};

pub type Reply = Box<dyn FnOnce(Result<Value, String>) + Send>;

// What the agent sends on its own: news of a session, a question, or the end of it all.
pub enum Incoming {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Closed,
}

// One agent process, spoken to in JSON-RPC, one message per line over its stdio. Replies to what
// was asked run on the reader's thread; the process goes with the last handle to it.
pub struct Conn {
    out: mpsc::Sender<String>,
    next: AtomicU64,
    pending: Mutex<HashMap<u64, Reply>>,
    child: Mutex<Option<Child>>,
}

impl Conn {
    pub fn spawn(
        program: &Path,
        args: &[String],
        cwd: &Path,
        log: Stdio,
        on: impl Fn(Incoming) + Send + 'static,
    ) -> io::Result<Arc<Self>> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log);
        orphan_guard(&mut cmd);
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(broken)?;
        let stdout = child.stdout.take().ok_or_else(broken)?;

        let (out, queue) = mpsc::channel();
        thread::spawn(move || write(stdin, queue));
        let conn = Arc::new(Self {
            out,
            next: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            child: Mutex::new(Some(child)),
        });
        let weak = Arc::downgrade(&conn);
        thread::spawn(move || read(weak, stdout, on));
        Ok(conn)
    }

    pub fn call(&self, method: &str, params: Value, then: Reply) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        lock(&self.pending).insert(id, then);
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if self.out.send(msg.to_string()).is_err()
            && let Some(then) = lock(&self.pending).remove(&id)
        {
            then(Err("the agent stopped".to_owned()));
        }
    }

    pub fn notify(&self, method: &str, params: Value) {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let _ = self.out.send(msg.to_string());
    }

    pub fn respond(&self, id: Value, result: Result<Value, (i64, String)>) {
        let msg = match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message },
            }),
        };
        let _ = self.out.send(msg.to_string());
    }

    fn resolve(&self, id: u64, result: Result<Value, String>) {
        let then = lock(&self.pending).remove(&id);
        if let Some(then) = then {
            then(result);
        }
    }

    fn fail_all(&self) {
        let pending: Vec<Reply> = lock(&self.pending).drain().map(|(_, then)| then).collect();
        for then in pending {
            then(Err("the agent stopped".to_owned()));
        }
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        if let Some(mut child) = lock(&self.child).take() {
            let _ = child.kill();
            thread::spawn(move || child.wait());
        }
    }
}

fn read(conn: Weak<Conn>, stdout: ChildStdout, on: impl Fn(Incoming)) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = msg.get("method").and_then(Value::as_str).map(str::to_owned);
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match (method, msg.get("id").cloned()) {
            (Some(method), Some(id)) => on(Incoming::Request { id, method, params }),
            (Some(method), None) => on(Incoming::Notification { method, params }),
            (None, Some(id)) => {
                let Some(id) = id.as_u64() else { continue };
                let result = match msg.get("error") {
                    Some(error) => Err(error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("the agent failed")
                        .to_owned()),
                    None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                };
                if let Some(conn) = conn.upgrade() {
                    conn.resolve(id, result);
                }
            }
            (None, None) => {}
        }
    }
    if let Some(conn) = conn.upgrade() {
        conn.fail_all();
    }
    on(Incoming::Closed);
}

fn write(mut stdin: ChildStdin, queue: mpsc::Receiver<String>) {
    for line in queue {
        if writeln!(stdin, "{line}").is_err() || stdin.flush().is_err() {
            break;
        }
    }
}

// The agent goes with corvo, even when corvo goes without a goodbye.
fn orphan_guard(cmd: &mut Command) {
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
}

pub fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn broken() -> io::Error {
    io::Error::other("agent pipe unavailable")
}
