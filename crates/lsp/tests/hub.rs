use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{Duration, Instant};

use corvo_domain::Lang;
use corvo_domain::server::{Registry, Spec};
use corvo_lsp::{Hub, Note};
use futures::channel::mpsc::UnboundedReceiver;
use lsp_types::{CompletionResponse, Position, Range, TextDocumentContentChangeEvent};

fn start(registry: Registry) -> (Hub, UnboundedReceiver<Note>) {
    static CACHE: Once = Once::new();
    CACHE.call_once(|| unsafe {
        std::env::set_var(
            "XDG_CACHE_HOME",
            std::env::temp_dir().join("corvo-test-cache"),
        );
    });
    let (mut hub, notes) = Hub::new(registry);
    hub.scope(vec![std::env::temp_dir()], None);
    (hub, notes)
}

fn hub() -> (Hub, UnboundedReceiver<Note>) {
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake.py");
    let spec = Spec::new("fake", "python3")
        .args(["-I".to_string(), fake.to_string_lossy().into_owned()])
        .langs([Lang::Rust]);
    start(Registry::new(vec![spec]))
}

fn next(notes: &mut UnboundedReceiver<Note>) -> Note {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(note) = notes.try_recv() {
            return note;
        }
        assert!(Instant::now() < deadline, "no note from server");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn message(note: Note) -> String {
    match note {
        Note::Diagnostics(_, items) => items[0].message.clone(),
        other => panic!("unexpected {other:?}"),
    }
}

fn file() -> PathBuf {
    std::env::temp_dir().join("corvo-hub.rs")
}

#[test]
fn queues_until_initialized_then_publishes() {
    let (mut hub, mut notes) = hub();
    hub.open(&file(), Lang::Rust, "fn main() {}");

    assert_eq!(message(next(&mut notes)), "opened");
}

#[test]
fn sends_incremental_changes_with_versions() {
    let (mut hub, mut notes) = hub();
    hub.open(&file(), Lang::Rust, "fn main() {}");
    next(&mut notes);

    let at = Position::new(0, 3);
    let change = TextDocumentContentChangeEvent {
        range: Some(Range::new(at, at)),
        range_length: None,
        text: "x".into(),
    };
    hub.change(&file(), vec![change], || unreachable!());

    assert_eq!(message(next(&mut notes)), "v1 range x");
}

#[test]
fn completes() {
    let (mut hub, mut notes) = hub();
    hub.open(&file(), Lang::Rust, "fn main() {}");
    next(&mut notes);

    let request = hub.complete(&file(), Position::new(0, 0), None).unwrap();
    let response = futures::executor::block_on(request).unwrap();

    let Some(CompletionResponse::Array(items)) = response else {
        panic!("unexpected {response:?}");
    };
    assert_eq!(items[0].label, "hello");
    assert_eq!(hub.triggers(&file()), ["."]);
}

#[test]
fn reports_missing_server_once() {
    let spec = Spec::new("ghost", "corvo-no-such-server")
        .langs([Lang::Rust])
        .hint("install ghost");
    let (mut hub, mut notes) = start(Registry::new(vec![spec]));

    hub.open(&file(), Lang::Rust, "");
    hub.open(&file(), Lang::Rust, "");

    assert!(matches!(next(&mut notes), Note::Missing { hint, .. } if hint == "install ghost"));
    std::thread::sleep(Duration::from_millis(100));
    assert!(notes.try_recv().is_err());
}

#[test]
fn reports_exit() {
    let spec = Spec::new("quitter", "true").langs([Lang::Rust]);
    let (mut hub, mut notes) = start(Registry::new(vec![spec]));

    hub.open(&file(), Lang::Rust, "");

    assert!(matches!(next(&mut notes), Note::Exited { server, .. } if server == "quitter"));
}

#[test]
fn reports_failed_initialize() {
    let script = "import sys,json\nh=sys.stdin.buffer.readline();sys.stdin.buffer.readline()\nb=json.dumps({'jsonrpc':'2.0','id':0,'error':{'code':-32603,'message':'no tsserver'}}).encode()\nsys.stdout.buffer.write(b'Content-Length: %d\\r\\n\\r\\n'%len(b)+b);sys.stdout.buffer.flush()\nsys.stdin.buffer.read()";
    let spec = Spec::new("broken", "python3")
        .args(["-I", "-c", script])
        .langs([Lang::Rust]);
    let (mut hub, mut notes) = start(Registry::new(vec![spec]));

    hub.open(&file(), Lang::Rust, "");

    assert!(matches!(next(&mut notes), Note::Failed { message, .. } if message == "no tsserver"));
}

#[test]
fn reaping_an_idle_server_is_quiet() {
    let (mut hub, mut notes) = hub();
    hub.open(&file(), Lang::Rust, "fn main() {}");
    next(&mut notes);

    hub.close(&file());
    hub.reap();

    std::thread::sleep(Duration::from_millis(300));
    assert!(notes.try_recv().is_err());
}

#[test]
fn reap_keeps_servers_with_open_documents() {
    let (mut hub, mut notes) = hub();
    hub.open(&file(), Lang::Rust, "fn main() {}");
    next(&mut notes);

    hub.reap();

    assert_eq!(hub.triggers(&file()), ["."]);
}
