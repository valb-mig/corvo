use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

use corvo_domain::port::ChatStore;
use corvo_domain::{
    Answer, Ask, Chat, Chats, Diff, End, Entry, Message, Pick, Status, Step, StepKind, Uuid, ago,
    image_mime,
};

fn chat() -> Chat {
    Chat::new(1, "claude", "/p", 100)
}

fn edit(id: &str, path: &str, old: &str, new: &str) -> Step {
    let mut step = Step::new(id, StepKind::Edit, format!("Edit {path}"));
    step.set_diffs(vec![Diff {
        path: PathBuf::from(path),
        old: Some(old.to_owned()),
        new: new.to_owned(),
    }]);
    step.set_status(Status::Done);
    step
}

#[test]
fn first_message_names_the_chat() {
    let mut chat = chat();
    assert_eq!(chat.title(), "New chat");
    chat.ask(
        "troca o ícone da status bar por um de código e roda os testes",
        110,
    );
    assert_eq!(chat.title(), "troca o ícone da status bar por um de có…");
    chat.end(End::Done, 120);
    chat.ask("outra coisa", 130);
    assert_eq!(chat.turns(), 2);
    assert!(chat.title().starts_with("troca"));
}

#[test]
fn a_message_of_images_alone_is_named_after_them() {
    let mut chat = chat();
    chat.ask_with("  ", vec![PathBuf::from("/k/1.png")], 110);
    assert_eq!(chat.title(), "Image");
    assert_eq!(
        chat.last_ask(),
        Some(&Message::new("", vec![PathBuf::from("/k/1.png")]))
    );

    let mut chat = self::chat();
    chat.ask_with(
        "",
        vec![PathBuf::from("/a.png"), PathBuf::from("/b.png")],
        110,
    );
    assert_eq!(chat.title(), "Images");

    let mut chat = self::chat();
    chat.ask_with(" olha isso ", vec![PathBuf::from("/a.png")], 110);
    assert_eq!(chat.title(), "olha isso");
    assert_eq!(chat.last_ask().unwrap().text, "olha isso");
}

#[test]
fn images_are_told_by_where_they_are_in_plain_text() {
    let message = Message::new("o que é?", vec![PathBuf::from("/k/1.png")]);
    assert_eq!(
        message.plain(),
        "o que é?\n\nImages attached, read them from disk:\n- /k/1.png"
    );
    assert_eq!(Message::new("só texto", Vec::new()).plain(), "só texto");

    let mut chat = chat();
    chat.ask_with("", vec![PathBuf::from("/k/1.png")], 110);
    chat.end(End::Done, 120);
    assert_eq!(
        chat.transcript(),
        "Dev: Images attached, read them from disk:\n- /k/1.png\n\n"
    );
}

#[test]
fn agents_take_the_common_image_types() {
    assert_eq!(image_mime(&PathBuf::from("/a/shot.PNG")), Some("image/png"));
    assert_eq!(image_mime(&PathBuf::from("b.jpg")), Some("image/jpeg"));
    assert_eq!(image_mime(&PathBuf::from("c.webp")), Some("image/webp"));
    assert_eq!(image_mime(&PathBuf::from("d.svg")), None);
    assert_eq!(image_mime(&PathBuf::from("e")), None);
}

#[test]
fn words_in_pieces_make_one_text() {
    let mut chat = chat();
    chat.ask("oi", 110);
    let reply = chat.reply_mut().unwrap();
    reply.say("Pron");
    reply.say("to.");
    reply.step(Step::new("t1", StepKind::Read, "Read a.rs"));
    reply.say("Fim");
    assert_eq!(reply.parts().len(), 3);
}

#[test]
fn news_of_a_step_replaces_it() {
    let mut chat = chat();
    chat.ask("oi", 110);
    let reply = chat.reply_mut().unwrap();
    reply.step(Step::new("t1", StepKind::Run, "cargo test"));
    let mut done = Step::new("t1", StepKind::Run, "cargo test");
    done.set_status(Status::Done);
    reply.step(done);
    assert_eq!(reply.steps().count(), 1);
    assert_eq!(reply.steps().next().unwrap().status(), Status::Done);
}

#[test]
fn ending_keeps_how_long_it_took_and_what_changed() {
    let mut chat = chat();
    chat.ask("muda", 110);
    let reply = chat.reply_mut().unwrap();
    reply.step(edit("a", "src/a.rs", "x\ny\n", "x\nz\nw\n"));
    reply.step(edit("b", "src/a.rs", "1\n", "2\n"));
    reply.step(edit("c", "src/b.rs", "", "new\n"));
    assert!(chat.is_working());
    chat.end(End::Done, 158);
    let reply = chat.last_reply().unwrap();
    assert!(!chat.is_working());
    assert_eq!(reply.secs(), 48);
    assert_eq!(reply.summary(), "2 files changed");
    assert_eq!(reply.steps().next().unwrap().lines(), (2, 1));
}

#[test]
fn a_question_from_the_agent_waits_on_the_dev() {
    let mut chat = chat();
    chat.ask("commita", 110);
    let mut step = Step::new("t1", StepKind::Run, "git commit");
    step.set_ask(Some(Ask {
        id: 7,
        options: vec![Answer {
            id: "allow".into(),
            name: "Allow".into(),
            allow: true,
        }],
    }));
    chat.reply_mut().unwrap().step(step);
    assert!(chat.is_waiting());
    chat.reply_mut()
        .unwrap()
        .step_mut("t1")
        .unwrap()
        .set_ask(None);
    assert!(!chat.is_waiting());
}

#[test]
fn switching_agent_marks_the_chat_once() {
    let mut chat = chat();
    chat.switch("codex");
    assert!(
        chat.entries().is_empty(),
        "nothing to mark before a message"
    );
    chat.ask("oi", 110);
    chat.end(End::Done, 111);
    chat.set_session(Some("s1".into()));
    chat.switch("gemini");
    chat.switch("opencode");
    assert_eq!(chat.agent(), "opencode");
    assert_eq!(chat.session(), None);
    assert_eq!(
        chat.entries().last(),
        Some(&Entry::Switch("opencode".into()))
    );
    assert_eq!(
        chat.entries()
            .iter()
            .filter(|entry| matches!(entry, Entry::Switch(_)))
            .count(),
        1
    );
}

#[test]
fn search_finds_title_messages_and_answers() {
    let mut chat = chat();
    chat.ask("ajusta a status bar", 110);
    chat.reply_mut()
        .unwrap()
        .say("Pronto: o card de estrutura fica no pé da sidebar.");
    chat.end(End::Done, 120);
    assert!(chat.matches("STATUS"));
    assert!(chat.matches("estrutura"));
    assert!(!chat.matches("grafo"));
    assert_eq!(chat.snippet("status"), None, "already in the title");
    let (text, at) = chat.snippet("estrutura").unwrap();
    assert!(text.starts_with('…'));
    assert_eq!(
        &text.chars().skip(at).take(9).collect::<String>(),
        "estrutura"
    );
}

#[test]
fn a_chat_left_working_settles_as_stopped() {
    let mut chat = chat();
    chat.ask("roda", 110);
    let mut step = Step::new("t1", StepKind::Run, "cargo test");
    step.set_status(Status::Running);
    chat.reply_mut().unwrap().step(step);
    chat.settle();
    assert!(!chat.is_working());
    let reply = chat.last_reply().unwrap();
    assert_eq!(reply.end(), Some(&End::Stopped));
    assert_eq!(reply.steps().next().unwrap().status(), Status::Failed);
}

#[test]
fn transcript_tells_who_said_what() {
    let mut chat = chat();
    chat.ask("oi", 110);
    chat.reply_mut().unwrap().say("olá");
    chat.end(End::Done, 111);
    chat.switch("codex");
    let text = chat.transcript();
    assert!(text.contains("Dev: oi"));
    assert!(text.contains("claude: olá"));
    assert!(text.contains("(switched to codex)"));
}

#[test]
fn ago_is_short() {
    assert_eq!(ago(100, 130), "now");
    assert_eq!(ago(0, 12 * 60), "12m");
    assert_eq!(ago(0, 3 * 3600), "3h");
    assert_eq!(ago(0, 2 * 86_400), "2d");
    assert_eq!(ago(0, 21 * 86_400), "3w");
}

#[test]
fn modes_that_skip_asking_are_risky() {
    let pick = |id: &str, name: &str| Pick {
        id: id.into(),
        name: name.into(),
        description: None,
    };
    assert!(pick("bypassPermissions", "Bypass Permissions").is_risky());
    assert!(pick("yolo", "YOLO").is_risky());
    assert!(!pick("acceptEdits", "Accept Edits").is_risky());
}

#[derive(Clone, Default)]
struct Memory(Rc<RefCell<Vec<(Uuid, Chat)>>>);

impl ChatStore for Memory {
    fn load(&self, workspace: Uuid) -> io::Result<Vec<Chat>> {
        Ok(self
            .0
            .borrow()
            .iter()
            .filter(|(ws, _)| *ws == workspace)
            .map(|(_, chat)| chat.clone())
            .collect())
    }

    fn save(&self, workspace: Uuid, chat: &Chat) -> io::Result<()> {
        let mut items = self.0.borrow_mut();
        items.retain(|(ws, kept)| !(*ws == workspace && kept.id() == chat.id()));
        items.push((workspace, chat.clone()));
        Ok(())
    }

    fn remove(&self, workspace: Uuid, id: u64) -> io::Result<()> {
        self.0
            .borrow_mut()
            .retain(|(ws, kept)| !(*ws == workspace && kept.id() == id));
        Ok(())
    }

    fn keep_image(&self, _: Uuid, id: u64, bytes: &[u8], ext: &str) -> io::Result<PathBuf> {
        Ok(PathBuf::from(format!("/kept/{id}/{}.{ext}", bytes.len())))
    }
}

#[test]
fn chats_keep_only_what_has_messages() {
    let store = Memory::default();
    let mut chats = Chats::load(Uuid::from_u128(7), Box::new(store.clone())).unwrap();
    let id = chats.create("claude", &PathBuf::from("/p"), 100);
    chats.save(id).unwrap();
    assert!(store.0.borrow().is_empty());
    chats.active_mut().unwrap().ask("oi", 110);
    chats.save(id).unwrap();
    assert_eq!(store.0.borrow().len(), 1);

    let again = Chats::load(Uuid::from_u128(7), Box::new(store.clone())).unwrap();
    assert_eq!(again.active_id(), Some(id));
    assert!(!again.active().unwrap().is_working(), "settled on load");
    assert!(
        Chats::load(Uuid::from_u128(8), Box::new(store))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_new_chat_drops_the_empty_one_picked() {
    let mut chats = Chats::load(Uuid::from_u128(1), Box::new(Memory::default())).unwrap();
    chats.create("claude", &PathBuf::from("/p"), 100);
    let id = chats.create("claude", &PathBuf::from("/p"), 101);
    assert_eq!(chats.len(), 1);
    assert_eq!(chats.active_id(), Some(id));
}

#[test]
fn archiving_or_deleting_the_picked_chat_moves_on() {
    let store = Memory::default();
    let mut chats = Chats::load(Uuid::from_u128(1), Box::new(store.clone())).unwrap();
    let old = chats.create("claude", &PathBuf::from("/p"), 100);
    chats.active_mut().unwrap().ask("um", 100);
    let new = chats.create("codex", &PathBuf::from("/p"), 200);
    chats.active_mut().unwrap().ask("dois", 200);

    chats.archive(new, true).unwrap();
    assert_eq!(chats.active_id(), Some(old));
    assert!(chats.get(new).unwrap().is_archived());

    chats.rename(old, "  Primeiro  ").unwrap();
    assert_eq!(chats.get(old).unwrap().title(), "Primeiro");
    chats.delete(old).unwrap();
    assert_eq!(chats.active_id(), None, "only an archived chat is left");
    assert_eq!(store.0.borrow().len(), 1);
}
