use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use corvo_domain::Lang;
use corvo_domain::server::Registry;
use corvo_lsp::{Hub, Note};
use lsp_types::{CompletionResponse, Position};

#[test]
#[ignore = "needs tsc 7+ on PATH"]
fn completes_and_diagnoses_with_tsc() {
    let ws = scratch();
    let file = ws.join("main.ts");
    let text = "const greeting: string = 'olá';\ngreeting.toUpp\n";
    fs::write(&file, text).unwrap();
    fs::write(ws.join("package.json"), "{}").unwrap();

    let (mut hub, mut notes) = Hub::new(Registry::default(), &ws);
    hub.open(&file, Lang::TypeScript, text);

    let deadline = Instant::now() + Duration::from_secs(20);
    let items = loop {
        if let Ok(Note::Diagnostics(_, items)) = notes.try_recv()
            && !items.is_empty()
        {
            break items;
        }
        assert!(Instant::now() < deadline, "no diagnostics");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(items[0].range.start, Position::new(1, 9));

    let request = hub.complete(&file, Position::new(1, 14), None).unwrap();
    let labels: Vec<String> = match futures::executor::block_on(request).unwrap() {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => Vec::new(),
    }
    .into_iter()
    .map(|item| item.label)
    .collect();
    assert!(labels.iter().any(|label| label == "toUpperCase"));

    fs::remove_dir_all(ws).unwrap();
}

fn scratch() -> PathBuf {
    let ws = std::env::temp_dir().join(format!("corvo-tsc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&ws);
    fs::create_dir_all(&ws).unwrap();
    ws
}
