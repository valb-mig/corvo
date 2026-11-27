use std::fs;
use std::path::PathBuf;

use corvo_domain::port::ChatStore;
use corvo_domain::{Answer, Ask, Chat, Diff, End, Status, Step, StepKind, Uuid};
use corvo_fs::JsonChats;

const SPACE: Uuid = Uuid::from_u128(1);

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
    assert!(store.load(SPACE).unwrap().is_empty());
}

#[test]
fn saves_and_loads_back_per_workspace() {
    let store = JsonChats::new(scratch("roundtrip"));
    let chat = chat();
    store.save(SPACE, &chat).unwrap();

    assert_eq!(store.load(SPACE).unwrap(), vec![chat]);
    assert!(store.load(Uuid::from_u128(2)).unwrap().is_empty());
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
    store.save(SPACE, &chat).unwrap();

    let back = store.load(SPACE).unwrap().remove(0);
    assert!(back.last_reply().unwrap().asking().is_none());
}

#[test]
fn removing_takes_the_file_and_ignores_a_missing_one() {
    let dir = scratch("remove");
    let store = JsonChats::new(&dir);
    store.save(SPACE, &chat()).unwrap();
    store.remove(SPACE, 3).unwrap();
    store.remove(SPACE, 3).unwrap();
    assert!(store.load(SPACE).unwrap().is_empty());
}

#[test]
fn forgetting_a_workspace_takes_its_chats() {
    let store = JsonChats::new(scratch("forget"));
    store.save(SPACE, &chat()).unwrap();
    store.forget(SPACE).unwrap();
    store.forget(SPACE).unwrap();
    assert!(store.load(SPACE).unwrap().is_empty());
}

#[test]
fn a_broken_file_is_left_out() {
    let dir = scratch("broken");
    let store = JsonChats::new(&dir);
    store.save(SPACE, &chat()).unwrap();
    fs::write(dir.join(SPACE.to_string()).join("9.json"), "{ not json").unwrap();
    assert_eq!(store.load(SPACE).unwrap().len(), 1);
}

#[test]
fn images_are_kept_beside_the_chat_and_go_with_it() {
    let dir = scratch("images");
    let store = JsonChats::new(&dir);
    let one = store.keep_image(SPACE, 3, b"png bytes", "PNG").unwrap();
    let two = store.keep_image(SPACE, 3, b"jpg bytes", "jpg").unwrap();
    let folder = dir.join(SPACE.to_string()).join("3");
    assert_eq!(one, folder.join("1.png"));
    assert_eq!(two, folder.join("2.jpg"));
    assert_eq!(fs::read(&one).unwrap(), b"png bytes");

    let mut chat = chat();
    chat.ask_with("e isso?", vec![one.clone(), two], 200);
    store.save(SPACE, &chat).unwrap();
    assert_eq!(store.load(SPACE).unwrap(), vec![chat]);

    store.remove(SPACE, 3).unwrap();
    assert!(!folder.exists());
}

#[test]
fn a_message_kept_before_images_loads_with_none() {
    let dir = scratch("old");
    let folder = dir.join(SPACE.to_string());
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("4.json"),
        r#"{"id":4,"title":"t","agent":"claude","project":"/p","created":1,"updated":1,
            "entries":[{"type":"you","text":"oi"}]}"#,
    )
    .unwrap();
    let chat = JsonChats::new(&dir).load(SPACE).unwrap().remove(0);
    let message = chat.last_ask().unwrap();
    assert_eq!((message.text.as_str(), message.images.len()), ("oi", 0));
}

#[test]
fn count_tells_how_many_chats_a_workspace_keeps() {
    let dir = scratch("count");
    let store = JsonChats::new(&dir);

    assert_eq!(store.count(SPACE).unwrap(), 0);
    store.save(SPACE, &chat()).unwrap();
    store
        .save(SPACE, &Chat::new(4, "claude", "/p", 200))
        .unwrap();
    fs::create_dir_all(dir.join(SPACE.to_string()).join("3")).unwrap();

    assert_eq!(store.count(SPACE).unwrap(), 2);
    fs::remove_dir_all(dir).unwrap();
}
