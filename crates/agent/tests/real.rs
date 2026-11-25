// Talks to the real Claude Code adapter, so it needs Node, network and a Claude Code login, and
// spends a few tokens. Run it by hand: `cargo test -p corvo-agent --test real -- --ignored`.

use std::time::{Duration, Instant};

use corvo_agent::{Event, Hub};
use corvo_domain::{End, agents};

#[test]
#[ignore]
fn claude_code_answers_through_acp() {
    let (hub, mut events) = Hub::new(agents());
    let project = std::env::temp_dir();
    hub.open(1, "claude", &project, None, None);
    hub.prompt(1, "Reply with just the word: pong. Do not use any tool.");

    let deadline = Instant::now() + Duration::from_secs(180);
    let (mut ready, mut text) = (false, String::new());
    loop {
        assert!(Instant::now() < deadline, "no end from Claude Code");
        let Ok(event) = events.try_recv() else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        match event {
            Event::Ready { options, .. } => {
                ready = true;
                assert!(options.model.is_some(), "Claude Code offers models");
                assert!(options.mode.is_some(), "Claude Code offers modes");
            }
            Event::Text { text: piece, .. } => text.push_str(&piece),
            Event::Ended { end, .. } => {
                assert_eq!(end, End::Done);
                break;
            }
            Event::Missing { .. } | Event::Exited { .. } => panic!("Claude Code did not run"),
            _ => {}
        }
    }
    assert!(ready);
    assert!(text.to_lowercase().contains("pong"), "said: {text}");
}
