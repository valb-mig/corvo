use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use corvo_agent::{Event, Hub, Patch};

use crate::attach::Attached;
use corvo_domain::port::OptionStore;
use corvo_domain::{Chat, Chats, End, Message, Options, Reply};
use gpui_kit::{Context, EventEmitter};

// Something the dev should hear about, outside any chat.
pub enum Note {
    Missing { agent: String, hint: String },
    Notice(String),
}

// The chats of the workspace and the agents they talk to: what the agent bar writes to and the
// chat window shows. A chat gets its session the first time it needs the agent: to send, or to
// show what the agent lets the dev pick.
pub struct Agents {
    hub: Hub,
    chats: Chats,
    options: HashMap<u64, Options>,
    // What each agent offered its last new session, shown while a chat waits for its own; the
    // picks made meanwhile reach the session once it is there. Kept between runs, so the picker
    // has it before the agent starts.
    known: HashMap<String, Options>,
    store: Box<dyn OptionStore>,
    pending: HashMap<u64, Vec<(String, String)>>,
    opening: HashSet<u64>,
    thinking: HashSet<u64>,
    // The agent a new chat starts with: the last one picked.
    agent: String,
}

impl EventEmitter<Note> for Agents {}

impl Agents {
    pub fn new(hub: Hub, chats: Chats, store: Box<dyn OptionStore>) -> Self {
        let agent = chats
            .active()
            .map(|chat| chat.agent().to_owned())
            .or_else(|| {
                hub.specs()
                    .iter()
                    .find(|spec| hub.available(spec.id()))
                    .map(|spec| spec.id().to_owned())
            })
            .or_else(|| hub.specs().first().map(|spec| spec.id().to_owned()))
            .unwrap_or_default();
        Self {
            hub,
            chats,
            options: HashMap::new(),
            // A file that no longer reads only costs the wait for the agent.
            known: store.load().unwrap_or_default(),
            store,
            pending: HashMap::new(),
            opening: HashSet::new(),
            thinking: HashSet::new(),
            agent,
        }
    }

    pub fn hub(&self) -> &Hub {
        &self.hub
    }

    pub fn chats(&self) -> &Chats {
        &self.chats
    }

    pub fn active(&self) -> Option<&Chat> {
        self.chats.active()
    }

    // The agent the bar shows: the picked chat's, or the one a new chat starts with.
    pub fn agent(&self) -> &str {
        self.chats.active().map_or(&self.agent, Chat::agent)
    }

    pub fn agent_name(&self, id: &str) -> String {
        self.hub
            .spec(id)
            .map_or_else(|| id.to_owned(), |spec| spec.name().to_owned())
    }

    pub fn options(&self) -> Option<&Options> {
        self.chats
            .active_id()
            .and_then(|id| self.options.get(&id))
            .or_else(|| self.known.get(self.agent()))
    }

    // Starts the agent a chat would talk to, before the dev asks for it.
    pub fn warm(&self, project: &Path) {
        self.hub.warm(self.agent(), project);
    }

    pub fn is_opening(&self) -> bool {
        self.chats
            .active_id()
            .is_some_and(|id| self.opening.contains(&id))
    }

    pub fn is_thinking(&self, chat: u64) -> bool {
        self.thinking.contains(&chat)
    }

    // A chat that some other chat than the picked one waits on the dev.
    pub fn someone_waits(&self) -> bool {
        let active = self.chats.active_id();
        self.chats
            .list()
            .any(|chat| Some(chat.id()) != active && chat.is_waiting())
    }

    // Makes sure there is a chat picked, and that it has its session with its agent.
    pub fn open(&mut self, project: &Path, cx: &mut Context<Self>) {
        if self.chats.active().is_none_or(Chat::is_archived) {
            self.chats.create(&self.agent, project, now());
        }
        let Some(chat) = self.chats.active() else {
            return;
        };
        let id = chat.id();
        if self.hub.is_open(id) || !self.hub.available(chat.agent()) {
            cx.notify();
            return;
        }
        let context = (chat.turns() > 0).then(|| chat.transcript());
        self.hub
            .open(id, chat.agent(), chat.project(), chat.session(), context);
        self.opening.insert(id);
        cx.notify();
    }

    // What the dev wrote goes to the picked chat, or to a new one, with the images attached, kept
    // with the chat first. Whether it went.
    pub fn send(
        &mut self,
        text: &str,
        images: &[Attached],
        project: &Path,
        cx: &mut Context<Self>,
    ) -> bool {
        let text = text.trim();
        if text.is_empty() && images.is_empty() {
            return false;
        }
        if self.chats.active().is_some_and(Chat::is_working) {
            cx.emit(Note::Notice(
                "The agent is still working: stop it or wait for it".to_owned(),
            ));
            return false;
        }
        self.open(project, cx);
        let Some(id) = self.chats.active_id() else {
            return false;
        };
        let mut kept = Vec::new();
        for image in images {
            match self.chats.keep_image(id, &image.bytes, image.ext) {
                Ok(path) => kept.push(path),
                Err(err) => {
                    cx.emit(Note::Notice(format!("{}: {err}", image.name)));
                    return false;
                }
            }
        }
        let message = Message::new(text, kept);
        let Some(chat) = self.chats.active_mut() else {
            return false;
        };
        chat.ask_with(text, message.images.clone(), now());
        let available = self.hub.available(chat.agent());
        let agent = chat.agent().to_owned();
        if !available {
            let name = self.agent_name(&agent);
            if let Some(chat) = self.chats.get_mut(id) {
                chat.end(End::Failed(format!("{name} is not installed")), now());
            }
            if let Some(spec) = self.hub.spec(&agent) {
                cx.emit(Note::Missing {
                    agent: spec.name().to_owned(),
                    hint: spec.install().to_owned(),
                });
            }
        } else {
            self.hub.send(id, &message);
        }
        self.save(id, cx);
        cx.notify();
        true
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(chat) = self.chats.active()
            && chat.is_working()
        {
            self.hub.cancel(chat.id());
        }
        cx.notify();
    }

    // Another agent for the picked chat: it goes on with the conversation so far.
    pub fn pick_agent(&mut self, agent: &str, project: &Path, cx: &mut Context<Self>) {
        if self.chats.active().is_some_and(Chat::is_working) {
            cx.emit(Note::Notice(
                "The agent is still working: stop it before switching".to_owned(),
            ));
            return;
        }
        self.agent = agent.to_owned();
        if let Some(chat) = self.chats.active_mut()
            && chat.agent() != agent
        {
            let id = chat.id();
            chat.switch(agent);
            self.hub.close(id);
            self.options.remove(&id);
            self.pending.remove(&id);
            self.opening.remove(&id);
            self.save(id, cx);
        }
        self.open(project, cx);
    }

    pub fn set(&mut self, option: &str, value: &str, cx: &mut Context<Self>) {
        let Some(id) = self.chats.active_id() else {
            return;
        };
        if self.hub.is_open(id) && !self.opening.contains(&id) {
            self.hub.set(id, option, value);
        } else {
            let picks = self.pending.entry(id).or_default();
            picks.retain(|(known, _)| known != option);
            picks.push((option.to_owned(), value.to_owned()));
            if !self.options.contains_key(&id)
                && let Some(known) = self.known.get(self.agent()).cloned()
            {
                self.options.insert(id, known);
            }
        }
        // Shown right away; the agent's answer settles it.
        if let Some(options) = self.options.get_mut(&id) {
            choose(options, option, value);
        }
        cx.notify();
    }

    pub fn new_chat(&mut self, project: &Path, cx: &mut Context<Self>) {
        self.chats.create(&self.agent, project, now());
        cx.notify();
    }

    pub fn pick(&mut self, id: u64, cx: &mut Context<Self>) {
        self.chats.pick(id);
        if let Some(chat) = self.chats.get(id) {
            self.agent = chat.agent().to_owned();
        }
        cx.notify();
    }

    pub fn rename(&mut self, id: u64, title: &str, cx: &mut Context<Self>) {
        let done = self.chats.rename(id, title);
        self.report(done, cx);
    }

    pub fn archive(&mut self, id: u64, archived: bool, cx: &mut Context<Self>) {
        let done = self.chats.archive(id, archived);
        self.report(done, cx);
    }

    pub fn delete(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.chats.get(id).is_some_and(Chat::is_working) {
            self.hub.cancel(id);
        }
        self.hub.close(id);
        self.options.remove(&id);
        self.pending.remove(&id);
        self.opening.remove(&id);
        let done = self.chats.delete(id);
        self.report(done, cx);
    }

    // The dev answers what the agent asked; no answer cancels it.
    pub fn answer(
        &mut self,
        chat: u64,
        step: &str,
        ask: u64,
        option: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.hub.answer(ask, option);
        if let Some(reply) = self.chats.get_mut(chat).and_then(Chat::reply_mut)
            && let Some(step) = reply.step_mut(step)
        {
            step.set_ask(None);
        }
        cx.notify();
    }

    pub fn apply(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Ready {
                chat,
                session,
                mut options,
            } => {
                self.opening.remove(&chat);
                if let Some(kept) = self.chats.get_mut(chat) {
                    kept.set_session(Some(session));
                    let agent = kept.agent().to_owned();
                    if self.known.get(&agent) != Some(&options) {
                        self.known.insert(agent, options.clone());
                        if let Err(err) = self.store.save(&self.known) {
                            cx.emit(Note::Notice(format!("agents: {err}")));
                        }
                    }
                }
                for (option, value) in self.pending.remove(&chat).unwrap_or_default() {
                    self.hub.set(chat, &option, &value);
                    choose(&mut options, &option, &value);
                }
                self.options.insert(chat, options);
                self.save(chat, cx);
            }
            Event::Options { chat, options } => {
                self.options.insert(chat, options);
            }
            Event::Mode { chat, current } => {
                if let Some(mode) = self
                    .options
                    .get_mut(&chat)
                    .and_then(|options| options.mode.as_mut())
                {
                    mode.current = current;
                }
            }
            Event::Text { chat, text } => {
                self.thinking.remove(&chat);
                if let Some(reply) = self.reply(chat) {
                    reply.say(&text);
                }
            }
            Event::Thinking { chat } => {
                self.thinking.insert(chat);
            }
            Event::Step { chat, id, patch } => {
                self.thinking.remove(&chat);
                if let Some(reply) = self.reply(chat) {
                    upsert(reply, &id, &patch);
                }
            }
            Event::Ask {
                chat,
                id,
                patch,
                ask,
            } => {
                if let Some(reply) = self.reply(chat) {
                    upsert(reply, &id, &patch);
                    if let Some(step) = reply.step_mut(&id) {
                        step.set_ask(Some(ask));
                    }
                }
            }
            Event::Ended { chat, end } => {
                self.opening.remove(&chat);
                self.thinking.remove(&chat);
                let working = self.chats.get(chat).is_some_and(Chat::is_working);
                if working {
                    if let Some(kept) = self.chats.get_mut(chat) {
                        kept.end(end, now());
                    }
                    self.save(chat, cx);
                } else if let End::Failed(why) = end {
                    cx.emit(Note::Notice(why));
                }
            }
            Event::Missing { agent, hint } => cx.emit(Note::Missing { agent, hint }),
            Event::Exited { chats, .. } => {
                for chat in chats {
                    self.opening.remove(&chat);
                    self.thinking.remove(&chat);
                    self.options.remove(&chat);
                    self.pending.remove(&chat);
                    if self.chats.get(chat).is_some_and(Chat::is_working) {
                        if let Some(kept) = self.chats.get_mut(chat) {
                            kept.end(End::Failed("the agent stopped".to_owned()), now());
                        }
                        self.save(chat, cx);
                    }
                }
            }
            Event::Notice(message) => cx.emit(Note::Notice(message)),
        }
        cx.notify();
    }

    fn reply(&mut self, chat: u64) -> Option<&mut Reply> {
        self.chats.get_mut(chat).and_then(Chat::reply_mut)
    }

    fn save(&mut self, id: u64, cx: &mut Context<Self>) {
        let done = self.chats.save(id);
        self.report(done, cx);
    }

    fn report(&mut self, done: std::io::Result<()>, cx: &mut Context<Self>) {
        if let Err(err) = done {
            cx.emit(Note::Notice(format!("chats: {err}")));
        }
        cx.notify();
    }
}

// A value picked for one of the options shown.
fn choose(options: &mut Options, option: &str, value: &str) {
    for choice in [&mut options.model, &mut options.effort, &mut options.mode]
        .into_iter()
        .flatten()
    {
        if choice.id == option {
            choice.current = value.to_owned();
        }
    }
}

fn upsert(reply: &mut Reply, id: &str, patch: &Patch) {
    match reply.step_mut(id) {
        Some(step) => patch.apply(step),
        None => reply.step(patch.step(id)),
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}
