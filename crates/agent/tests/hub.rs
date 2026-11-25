use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{Duration, Instant};

use corvo_agent::{Event, Hub, MODE};
use corvo_domain::{AgentSpec, End, Status, StepKind};
use futures::channel::mpsc::UnboundedReceiver;

fn start(spec: AgentSpec) -> (Hub, UnboundedReceiver<Event>) {
    static CACHE: Once = Once::new();
    CACHE.call_once(|| unsafe {
        std::env::set_var(
            "XDG_CACHE_HOME",
            std::env::temp_dir().join("corvo-test-cache"),
        );
    });
    Hub::new(vec![spec])
}

fn hub() -> (Hub, UnboundedReceiver<Event>) {
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_agent.py");
    let fake = fake.to_string_lossy().into_owned();
    start(AgentSpec::new(
        "fake",
        "Fake",
        "python3",
        &["-I", &fake],
        "pip install fake",
    ))
}

fn project() -> PathBuf {
    std::env::temp_dir()
}

fn next(events: &mut UnboundedReceiver<Event>) -> Event {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(event) = events.try_recv() {
            return event;
        }
        assert!(Instant::now() < deadline, "no event from the agent");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn opens_a_session_with_the_picks_the_agent_offers() {
    let (hub, mut events) = hub();
    hub.open(1, "fake", &project(), None, None);
    let Event::Ready {
        chat,
        session,
        options,
    } = next(&mut events)
    else {
        panic!("not ready");
    };
    assert_eq!((chat, session.as_str()), (1, "s1"));
    assert_eq!(options.model.unwrap().current, "opus");
    assert_eq!(options.effort.unwrap().name(), "high");
    let mode = options.mode.unwrap();
    assert_eq!((mode.id.as_str(), mode.current.as_str()), (MODE, "default"));
    assert!(mode.values[1].is_risky());
}

#[test]
fn a_message_before_the_session_waits_for_it() {
    let (hub, mut events) = hub();
    hub.open(1, "fake", &project(), None, None);
    hub.prompt(1, "hi");
    assert!(matches!(next(&mut events), Event::Ready { .. }));
    let Event::Text { text, .. } = next(&mut events) else {
        panic!("no text");
    };
    assert_eq!(text, "echo: hi");
    assert!(matches!(
        next(&mut events),
        Event::Ended {
            chat: 1,
            end: End::Done
        }
    ));
}

#[test]
fn steps_come_with_what_they_touch() {
    let (hub, mut events) = hub();
    hub.open(1, "fake", &project(), None, None);
    hub.prompt(1, "run it");
    next(&mut events);
    next(&mut events);
    let Event::Step { id, patch, .. } = next(&mut events) else {
        panic!("no step");
    };
    let step = patch.step(&id);
    assert_eq!(step.kind(), StepKind::Run);
    assert_eq!(step.target(), Some("cargo test"));
    assert_eq!(step.output(), "test result: ok");
    assert_eq!(step.status(), Status::Done);
}

#[test]
fn a_question_waits_on_the_dev() {
    let (hub, mut events) = hub();
    hub.open(1, "fake", &project(), None, None);
    hub.prompt(1, "ask first");
    next(&mut events);
    next(&mut events);
    let Event::Step { id, patch, .. } = next(&mut events) else {
        panic!("no step");
    };
    let mut step = patch.step(&id);
    assert_eq!(step.diffs()[0].new, "b\n");
    assert_eq!(step.target(), Some("/p/a.rs"));
    let Event::Ask { id, ask, .. } = next(&mut events) else {
        panic!("no question");
    };
    assert_eq!(id, "t1");
    assert!(ask.options[0].allow && !ask.options[1].allow);

    hub.answer(ask.id, Some("yes"));
    let Event::Step { patch, .. } = next(&mut events) else {
        panic!("no news of the step");
    };
    patch.apply(&mut step);
    assert_eq!(step.status(), Status::Done);
    assert!(matches!(
        next(&mut events),
        Event::Ended { end: End::Done, .. }
    ));
}

#[test]
fn cancelling_answers_the_open_question() {
    let (hub, mut events) = hub();
    hub.open(1, "fake", &project(), None, None);
    hub.prompt(1, "ask first");
    next(&mut events);
    next(&mut events);
    next(&mut events);
    assert!(matches!(next(&mut events), Event::Ask { .. }));
    hub.cancel(1);
    let Event::Step { patch, .. } = next(&mut events) else {
        panic!("no news of the step");
    };
    assert_eq!(patch.status, Some(Status::Failed));
}

#[test]
fn picks_reach_the_agent() {
    let (hub, mut events) = hub();
    hub.open(1, "fake", &project(), None, None);
    next(&mut events);
    hub.set(1, "model", "sonnet");
    let Event::Options { options, .. } = next(&mut events) else {
        panic!("no options");
    };
    assert_eq!(options.model.unwrap().current, "sonnet");
    hub.set(1, MODE, "bypassPermissions");
    let Event::Mode { current, .. } = next(&mut events) else {
        panic!("no mode");
    };
    assert_eq!(current, "bypassPermissions");
}

#[test]
fn a_new_agent_is_told_the_conversation_once() {
    let (hub, mut events) = hub();
    hub.open(2, "fake", &project(), None, Some("Dev: oi\n\n".into()));
    hub.prompt(2, "one");
    next(&mut events);
    let Event::Text { text, .. } = next(&mut events) else {
        panic!("no text");
    };
    assert!(text.contains("You are joining") && text.contains("Dev: oi") && text.ends_with("one"));
    next(&mut events);
    hub.prompt(2, "two");
    let Event::Text { text, .. } = next(&mut events) else {
        panic!("no text");
    };
    assert_eq!(text, "echo: two");
}

#[test]
fn a_session_loads_back_without_replaying_it() {
    let (hub, mut events) = hub();
    hub.open(
        3,
        "fake",
        &project(),
        Some("s9"),
        Some("Dev: oi\n\n".into()),
    );
    let Event::Ready { session, .. } = next(&mut events) else {
        panic!("not ready");
    };
    assert_eq!(session, "s9");
    hub.prompt(3, "x");
    let Event::Text { text, .. } = next(&mut events) else {
        panic!("no text");
    };
    assert_eq!(text, "echo: x", "a loaded session needs no telling");
}

#[test]
fn a_lost_session_starts_anew_and_is_told() {
    let (hub, mut events) = hub();
    hub.open(
        4,
        "fake",
        &project(),
        Some("lost"),
        Some("Dev: oi\n\n".into()),
    );
    hub.prompt(4, "x");
    let Event::Ready { session, .. } = next(&mut events) else {
        panic!("not ready");
    };
    assert_eq!(session, "s1");
    let Event::Text { text, .. } = next(&mut events) else {
        panic!("no text");
    };
    assert!(text.contains("You are joining"));
}

#[test]
fn a_missing_agent_says_how_to_install_it() {
    let (hub, mut events) = start(AgentSpec::new(
        "ghost",
        "Ghost",
        "corvo-no-such-agent",
        &[],
        "npm i -g ghost",
    ));
    assert!(!hub.available("ghost"));
    hub.open(1, "ghost", &project(), None, None);
    let Event::Missing { agent, hint } = next(&mut events) else {
        panic!("not missing");
    };
    assert_eq!((agent.as_str(), hint.as_str()), ("Ghost", "npm i -g ghost"));
    assert!(matches!(
        next(&mut events),
        Event::Ended {
            end: End::Failed(_),
            ..
        }
    ));
}
