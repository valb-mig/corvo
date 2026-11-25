use std::fs;
use std::path::PathBuf;

use corvo_domain::port::ChatStore;
use corvo_domain::{Answer, Ask, Chat, Diff, End, Status, Step, StepKind};
use corvo_fs::JsonChats;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("corvo-chats-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn chat() -> Chat {
    let mut chat = Chat::new(3, "claude", "/p", 100);
    chat.ask("muda o ícone", 110);
    let reply = chat.reply_mut().unwrap();
    reply.say("Vou mudar.");
    let mut step = Step::new("t1", StepKind::Edit, "Edit shell.rs");
    step.set_target(Some("crates/corvo/src/shell.rs".into()));
    step.set_status(Status::Done);
    step.set_diffs(vec![Diff {
        path: "crates/corvo/src/shell.rs".into(),
        old: Some("a\n".into()),
        new: "b\n".into(),
    }]);
    reply.step(step);
    let mut run = Step::new("t2", StepKind::Run, "cargo test");
    run.set_status(Status::Done);
    run.set_output("test result: ok");
    reply.step(run);
    chat.end(End::Done, 150);
    chat.switch("codex");
    chat.set_session(Some("s-9".into()));
    chat
}

#[test]
fn missing_folder_loads_empty() {
    let store = JsonChats::new(scratch("missing"));
    assert!(store.load(1).unwrap().is_empty());
}

#[test]
fn saves_and_loads_back_per_workspace() {
    let store = JsonChats::new(scratch("roundtrip"));
    let chat = chat();
    store.save(1, &chat).unwrap();

    assert_eq!(store.load(1).unwrap(), vec![chat]);
    assert!(store.load(2).unwrap().is_empty());
}

#[test]
fn a_waiting_question_is_not_kept() {
    let store = JsonChats::new(scratch("ask"));
    let mut chat = Chat::new(1, "claude", "/p", 100);
    chat.ask("commita", 110);
    let mut step = Step::new("t1", StepKind::Run, "git commit");
    step.set_ask(Some(Ask {
        id: 1,
        options: vec![Answer {
            id: "ok".into(),
            name: "Allow".into(),
            allow: true,
        }],
    }));
    chat.reply_mut().unwrap().step(step);
    store.save(1, &chat).unwrap();

    let back = store.load(1).unwrap().remove(0);
    assert!(back.last_reply().unwrap().asking().is_none());
}

#[test]
fn removing_takes_the_file_and_ignores_a_missing_one() {
    let dir = scratch("remove");
    let store = JsonChats::new(&dir);
    store.save(1, &chat()).unwrap();
    store.remove(1, 3).unwrap();
    store.remove(1, 3).unwrap();
    assert!(store.load(1).unwrap().is_empty());
}

#[test]
fn a_broken_file_is_left_out() {
    let dir = scratch("broken");
    let store = JsonChats::new(&dir);
    store.save(1, &chat()).unwrap();
    fs::write(dir.join("1").join("9.json"), "{ not json").unwrap();
    assert_eq!(store.load(1).unwrap().len(), 1);
}
