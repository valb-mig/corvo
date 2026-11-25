use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use agent_client_protocol_schema::v1::{
    InitializeResponse, LoadSessionResponse, NewSessionResponse, PromptResponse,
    RequestPermissionRequest, SessionNotification, SessionUpdate, SetSessionConfigOptionResponse,
};
use corvo_domain::{AgentSpec, Ask, End, Options};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde_json::{Value, json};

use crate::conn::{Conn, Incoming, lock};
use crate::map::{self, MODE, Patch};

// What the agents tell the app, each about a chat.
#[derive(Debug)]
pub enum Event {
    // The chat has a session with its agent now, with the picks the agent offers.
    Ready {
        chat: u64,
        session: String,
        options: Options,
    },
    Options {
        chat: u64,
        options: Options,
    },
    Mode {
        chat: u64,
        current: String,
    },
    Text {
        chat: u64,
        text: String,
    },
    Thinking {
        chat: u64,
    },
    Step {
        chat: u64,
        id: String,
        patch: Patch,
    },
    // The agent waits on the dev to go on with a step.
    Ask {
        chat: u64,
        id: String,
        patch: Patch,
        ask: Ask,
    },
    Ended {
        chat: u64,
        end: End,
    },
    Missing {
        agent: String,
        hint: String,
    },
    // The agent's process went away, with the chats it was in.
    Exited {
        agent: String,
        chats: Vec<u64>,
    },
    Notice(String),
}

// The agents at work and their sessions, one per chat. An agent starts the first time a chat
// needs it, and serves every chat with it after that.
pub struct Hub {
    specs: Vec<AgentSpec>,
    shared: Arc<Shared>,
}

struct Shared {
    events: UnboundedSender<Event>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    agents: HashMap<String, Running>,
    sessions: HashMap<u64, Session>,
    asks: HashMap<u64, Waiting>,
    next_ask: u64,
}

type Then = Box<dyn FnOnce(&Arc<Shared>, &Arc<Conn>, bool) + Send>;

struct Running {
    conn: Arc<Conn>,
    // Whether it can load a session back, once it said hello; until then, what waits on it.
    load: Option<bool>,
    queue: Vec<Then>,
}

struct Session {
    agent: String,
    id: Option<String>,
    // The agent replays a session it loads: that is not news.
    loading: bool,
    // The conversation before this agent came in, told with its first message.
    context: Option<String>,
    queued: Vec<String>,
}

struct Waiting {
    agent: String,
    chat: u64,
    rpc: Value,
}

impl Hub {
    pub fn new(specs: Vec<AgentSpec>) -> (Self, UnboundedReceiver<Event>) {
        let (events, rx) = unbounded();
        let shared = Arc::new(Shared {
            events,
            state: Mutex::new(State::default()),
        });
        (Self { specs, shared }, rx)
    }

    pub fn specs(&self) -> &[AgentSpec] {
        &self.specs
    }

    pub fn spec(&self, id: &str) -> Option<&AgentSpec> {
        self.specs.iter().find(|spec| spec.id() == id)
    }

    // Whether the agent's program is there to start.
    pub fn available(&self, id: &str) -> bool {
        self.spec(id)
            .is_some_and(|spec| find(spec.program()).is_some())
    }

    // Starts the agent ahead of the first chat that needs it, so that chat only waits on its
    // session.
    pub fn warm(&self, agent: &str, cwd: &Path) {
        if let Some(spec) = self.spec(agent).cloned()
            && self.available(agent)
        {
            self.shared.when_ready(&spec, cwd, |_, _, _| {});
        }
    }

    pub fn is_open(&self, chat: u64) -> bool {
        lock(&self.shared.state).sessions.contains_key(&chat)
    }

    // Gives the chat a session with its agent: the one it had, when the agent can load it back,
    // or a new one told the conversation so far.
    pub fn open(
        &self,
        chat: u64,
        agent: &str,
        project: &Path,
        resume: Option<&str>,
        context: Option<String>,
    ) {
        let Some(spec) = self.spec(agent).cloned() else {
            self.shared
                .emit(Event::Notice(format!("unknown agent {agent}")));
            return;
        };
        {
            let mut state = lock(&self.shared.state);
            if state
                .sessions
                .get(&chat)
                .is_some_and(|session| session.agent == agent)
            {
                return;
            }
            state.sessions.insert(
                chat,
                Session {
                    agent: agent.to_owned(),
                    id: None,
                    loading: false,
                    context,
                    queued: Vec::new(),
                },
            );
        }
        let cwd = project.to_path_buf();
        let resume = resume.map(str::to_owned);
        self.shared
            .when_ready(&spec, project, move |shared, conn, load| {
                shared.start(chat, conn, load, cwd, resume);
            });
    }

    // Sends what the dev wrote; before the session is there, it waits for it.
    pub fn prompt(&self, chat: u64, text: &str) {
        let mut state = lock(&self.shared.state);
        let Some(session) = state.sessions.get_mut(&chat) else {
            drop(state);
            self.shared.emit(Event::Ended {
                chat,
                end: End::Failed("the chat has no session".to_owned()),
            });
            return;
        };
        if session.id.is_none() || session.loading {
            session.queued.push(text.to_owned());
            return;
        }
        let text = told(session.context.take(), text);
        let id = session.id.clone().unwrap_or_default();
        let agent = session.agent.clone();
        let conn = state.agents.get(&agent).map(|running| running.conn.clone());
        drop(state);
        if let Some(conn) = conn {
            self.shared.send(chat, &conn, &id, text);
        }
    }

    // Stops the reply coming in; a question left open is answered as cancelled.
    pub fn cancel(&self, chat: u64) {
        let mut state = lock(&self.shared.state);
        let asks: Vec<u64> = state
            .asks
            .iter()
            .filter(|(_, waiting)| waiting.chat == chat)
            .map(|(id, _)| *id)
            .collect();
        let mut answers = Vec::new();
        for id in asks {
            if let Some(waiting) = state.asks.remove(&id)
                && let Some(running) = state.agents.get(&waiting.agent)
            {
                answers.push((running.conn.clone(), waiting.rpc));
            }
        }
        let target = state.sessions.get(&chat).and_then(|session| {
            let conn = state.agents.get(&session.agent)?.conn.clone();
            Some((conn, session.id.clone()?))
        });
        drop(state);
        for (conn, rpc) in answers {
            conn.respond(rpc, Ok(cancelled()));
        }
        if let Some((conn, id)) = target {
            conn.notify("session/cancel", json!({ "sessionId": id }));
        }
    }

    // Picks a value of one of the agent's options for the chat's session.
    pub fn set(&self, chat: u64, option: &str, value: &str) {
        let state = lock(&self.shared.state);
        let Some((conn, id)) = state.sessions.get(&chat).and_then(|session| {
            let conn = state.agents.get(&session.agent)?.conn.clone();
            Some((conn, session.id.clone()?))
        }) else {
            return;
        };
        drop(state);
        let shared = Arc::downgrade(&self.shared);
        if option == MODE {
            let current = value.to_owned();
            conn.call(
                "session/set_mode",
                json!({ "sessionId": id, "modeId": value }),
                Box::new(move |result| {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    match result {
                        Ok(_) => shared.emit(Event::Mode { chat, current }),
                        Err(err) => shared.emit(Event::Notice(err)),
                    }
                }),
            );
            return;
        }
        conn.call(
            "session/set_config_option",
            json!({ "sessionId": id, "configId": option, "value": value }),
            Box::new(move |result| {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let parsed = result.and_then(|value| {
                    serde_json::from_value::<SetSessionConfigOptionResponse>(value)
                        .map_err(|err| err.to_string())
                });
                match parsed {
                    Ok(response) => shared.emit(Event::Options {
                        chat,
                        options: map::options(&response.config_options, None),
                    }),
                    Err(err) => shared.emit(Event::Notice(err)),
                }
            }),
        );
    }

    // The dev's answer to a question; none cancels it.
    pub fn answer(&self, ask: u64, option: Option<&str>) {
        let mut state = lock(&self.shared.state);
        let Some(waiting) = state.asks.remove(&ask) else {
            return;
        };
        let conn = state
            .agents
            .get(&waiting.agent)
            .map(|running| running.conn.clone());
        drop(state);
        let outcome = match option {
            Some(option) => json!({ "outcome": { "outcome": "selected", "optionId": option } }),
            None => cancelled(),
        };
        if let Some(conn) = conn {
            conn.respond(waiting.rpc, Ok(outcome));
        }
    }

    // The chat no longer talks to its session: it was deleted, or another agent took it.
    pub fn close(&self, chat: u64) {
        let mut state = lock(&self.shared.state);
        state.sessions.remove(&chat);
        state.asks.retain(|_, waiting| waiting.chat != chat);
    }
}

impl Shared {
    fn emit(&self, event: Event) {
        let _ = self.events.unbounded_send(event);
    }

    // Runs once the agent said hello, starting it first if it is not running.
    fn when_ready(
        self: &Arc<Self>,
        spec: &AgentSpec,
        cwd: &Path,
        then: impl FnOnce(&Arc<Shared>, &Arc<Conn>, bool) + Send + 'static,
    ) {
        let mut state = lock(&self.state);
        if let Some(running) = state.agents.get_mut(spec.id()) {
            match running.load {
                Some(load) => {
                    let conn = running.conn.clone();
                    drop(state);
                    then(self, &conn, load);
                }
                None => running.queue.push(Box::new(then)),
            }
            return;
        }
        let conn = match self.spawn(spec, cwd) {
            Ok(conn) => conn,
            Err(err) => {
                let chats: Vec<u64> = state
                    .sessions
                    .iter()
                    .filter(|(_, session)| session.agent == spec.id())
                    .map(|(chat, _)| *chat)
                    .collect();
                state
                    .sessions
                    .retain(|_, session| session.agent != spec.id());
                drop(state);
                if err.kind() == std::io::ErrorKind::NotFound {
                    self.emit(Event::Missing {
                        agent: spec.name().to_owned(),
                        hint: spec.install().to_owned(),
                    });
                }
                for chat in chats {
                    self.emit(Event::Ended {
                        chat,
                        end: End::Failed(format!("{} did not start: {err}", spec.name())),
                    });
                }
                return;
            }
        };
        state.agents.insert(
            spec.id().to_owned(),
            Running {
                conn: conn.clone(),
                load: None,
                queue: vec![Box::new(then)],
            },
        );
        drop(state);

        let shared = Arc::downgrade(self);
        let agent = spec.id().to_owned();
        let name = spec.name().to_owned();
        conn.call(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": { "readTextFile": false, "writeTextFile": false },
                    "terminal": false,
                },
                "clientInfo": { "name": "corvo", "version": env!("CARGO_PKG_VERSION") },
            }),
            Box::new(move |result| {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let parsed = result.and_then(|value| {
                    serde_json::from_value::<InitializeResponse>(value)
                        .map_err(|err| err.to_string())
                });
                match parsed {
                    Ok(hello) => shared.ready(&agent, hello.agent_capabilities.load_session),
                    Err(err) => shared.gone(&agent, Some(format!("{name}: {err}"))),
                }
            }),
        );
    }

    fn spawn(self: &Arc<Self>, spec: &AgentSpec, cwd: &Path) -> std::io::Result<Arc<Conn>> {
        let program = find(spec.program())
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let shared = Arc::downgrade(self);
        let agent = spec.id().to_owned();
        Conn::spawn(&program, spec.args(), cwd, log(spec.id()), move |msg| {
            if let Some(shared) = shared.upgrade() {
                shared.incoming(&agent, msg);
            }
        })
    }

    fn ready(self: &Arc<Self>, agent: &str, load: bool) {
        let mut state = lock(&self.state);
        let Some(running) = state.agents.get_mut(agent) else {
            return;
        };
        running.load = Some(load);
        let queue = std::mem::take(&mut running.queue);
        let conn = running.conn.clone();
        drop(state);
        for then in queue {
            then(self, &conn, load);
        }
    }

    // A new session for the chat, or the one it had loaded back.
    fn start(
        self: &Arc<Self>,
        chat: u64,
        conn: &Arc<Conn>,
        load: bool,
        project: PathBuf,
        resume: Option<String>,
    ) {
        let shared = Arc::downgrade(self);
        if let (true, Some(id)) = (load, resume) {
            {
                let mut state = lock(&self.state);
                let Some(session) = state.sessions.get_mut(&chat) else {
                    return;
                };
                session.id = Some(id.clone());
                session.loading = true;
            }
            let retry = conn.clone();
            let cwd = project.clone();
            conn.call(
                "session/load",
                json!({ "sessionId": id, "cwd": project, "mcpServers": [] }),
                Box::new(move |result| {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    let parsed = result.and_then(|value| {
                        serde_json::from_value::<LoadSessionResponse>(value)
                            .map_err(|err| err.to_string())
                    });
                    match parsed {
                        Ok(loaded) => {
                            let options = map::options(
                                &loaded.config_options.unwrap_or_default(),
                                loaded.modes.as_ref(),
                            );
                            shared.started(chat, id, options, true);
                        }
                        // The agent lost it: a new one, told the conversation.
                        Err(_) => shared.start(chat, &retry, false, cwd, None),
                    }
                }),
            );
            return;
        }
        conn.call(
            "session/new",
            json!({ "cwd": project, "mcpServers": [] }),
            Box::new(move |result| {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let parsed = result.and_then(|value| {
                    serde_json::from_value::<NewSessionResponse>(value)
                        .map_err(|err| err.to_string())
                });
                match parsed {
                    Ok(new) => {
                        let options = map::options(
                            &new.config_options.unwrap_or_default(),
                            new.modes.as_ref(),
                        );
                        shared.started(chat, new.session_id.0.to_string(), options, false);
                    }
                    Err(err) => shared.failed(chat, err),
                }
            }),
        );
    }

    fn started(self: &Arc<Self>, chat: u64, id: String, options: Options, loaded: bool) {
        let mut state = lock(&self.state);
        let Some(session) = state.sessions.get_mut(&chat) else {
            return;
        };
        session.id = Some(id.clone());
        session.loading = false;
        if loaded {
            session.context = None;
        }
        let queued = std::mem::take(&mut session.queued);
        let mut context = session.context.take();
        let agent = session.agent.clone();
        let conn = state.agents.get(&agent).map(|running| running.conn.clone());
        drop(state);
        self.emit(Event::Ready {
            chat,
            session: id.clone(),
            options,
        });
        let Some(conn) = conn else { return };
        for text in queued {
            self.send(chat, &conn, &id, told(context.take(), &text));
        }
        if let Some(context) = context {
            let mut state = lock(&self.state);
            if let Some(session) = state.sessions.get_mut(&chat) {
                session.context = Some(context);
            }
        }
    }

    fn failed(&self, chat: u64, why: String) {
        lock(&self.state).sessions.remove(&chat);
        self.emit(Event::Ended {
            chat,
            end: End::Failed(why),
        });
    }

    fn send(self: &Arc<Self>, chat: u64, conn: &Arc<Conn>, session: &str, text: String) {
        let shared = Arc::downgrade(self);
        conn.call(
            "session/prompt",
            json!({ "sessionId": session, "prompt": [{ "type": "text", "text": text }] }),
            Box::new(move |result| {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let end = match result.and_then(|value| {
                    serde_json::from_value::<PromptResponse>(value).map_err(|err| err.to_string())
                }) {
                    Ok(response) => map::end(&response.stop_reason),
                    Err(err) => End::Failed(err),
                };
                shared.emit(Event::Ended { chat, end });
            }),
        );
    }

    fn incoming(self: &Arc<Self>, agent: &str, msg: Incoming) {
        match msg {
            Incoming::Notification { method, params } if method == "session/update" => {
                let Ok(note) = serde_json::from_value::<SessionNotification>(params) else {
                    return;
                };
                let Some(chat) = self.chat_of(agent, &note.session_id.0) else {
                    return;
                };
                self.update(chat, note.update);
            }
            Incoming::Notification { .. } => {}
            Incoming::Request { id, method, params } if method == "session/request_permission" => {
                self.permission(agent, id, params);
            }
            Incoming::Request { id, method, .. } => {
                let conn = lock(&self.state)
                    .agents
                    .get(agent)
                    .map(|running| running.conn.clone());
                if let Some(conn) = conn {
                    conn.respond(id, Err((-32601, format!("{method} is not supported"))));
                }
            }
            Incoming::Closed => self.gone(agent, None),
        }
    }

    // The chat a session of the agent belongs to, unless it is replaying a load.
    fn chat_of(&self, agent: &str, session: &str) -> Option<u64> {
        let state = lock(&self.state);
        state.sessions.iter().find_map(|(chat, known)| {
            (known.agent == agent && known.id.as_deref() == Some(session) && !known.loading)
                .then_some(*chat)
        })
    }

    fn update(&self, chat: u64, update: SessionUpdate) {
        let event = match update {
            SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                agent_client_protocol_schema::v1::ContentBlock::Text(text) => Event::Text {
                    chat,
                    text: text.text,
                },
                _ => return,
            },
            SessionUpdate::AgentThoughtChunk(_) => Event::Thinking { chat },
            SessionUpdate::ToolCall(call) => Event::Step {
                chat,
                id: call.tool_call_id.0.to_string(),
                patch: map::call(&call),
            },
            SessionUpdate::ToolCallUpdate(update) => Event::Step {
                chat,
                id: update.tool_call_id.0.to_string(),
                patch: map::update(&update.fields),
            },
            SessionUpdate::CurrentModeUpdate(mode) => Event::Mode {
                chat,
                current: mode.current_mode_id.0.to_string(),
            },
            SessionUpdate::ConfigOptionUpdate(update) => Event::Options {
                chat,
                options: map::options(&update.config_options, None),
            },
            _ => return,
        };
        self.emit(event);
    }

    fn permission(&self, agent: &str, rpc: Value, params: Value) {
        let parsed = serde_json::from_value::<RequestPermissionRequest>(params);
        let chat = parsed
            .as_ref()
            .ok()
            .and_then(|request| self.chat_of(agent, &request.session_id.0));
        let (Ok(request), Some(chat)) = (parsed, chat) else {
            let conn = lock(&self.state)
                .agents
                .get(agent)
                .map(|running| running.conn.clone());
            if let Some(conn) = conn {
                conn.respond(rpc, Ok(cancelled()));
            }
            return;
        };
        let mut state = lock(&self.state);
        state.next_ask += 1;
        let ask = state.next_ask;
        state.asks.insert(
            ask,
            Waiting {
                agent: agent.to_owned(),
                chat,
                rpc,
            },
        );
        drop(state);
        self.emit(Event::Ask {
            chat,
            id: request.tool_call.tool_call_id.0.to_string(),
            patch: map::update(&request.tool_call.fields),
            ask: Ask {
                id: ask,
                options: map::answers(&request.options),
            },
        });
    }

    // The agent went away: its chats lose their sessions and start new ones when written to.
    fn gone(&self, agent: &str, why: Option<String>) {
        let mut state = lock(&self.state);
        state.agents.remove(agent);
        let chats: Vec<u64> = state
            .sessions
            .iter()
            .filter(|(_, session)| session.agent == agent)
            .map(|(chat, _)| *chat)
            .collect();
        state.sessions.retain(|_, session| session.agent != agent);
        state.asks.retain(|_, waiting| waiting.agent != agent);
        drop(state);
        if let Some(why) = why {
            self.emit(Event::Notice(why));
        }
        self.emit(Event::Exited {
            agent: agent.to_owned(),
            chats,
        });
    }
}

// The conversation before the agent joined goes ahead of its first message.
fn told(context: Option<String>, text: &str) -> String {
    match context {
        Some(context) if !context.trim().is_empty() => format!(
            "You are joining a conversation that another agent started. This is what was said \
             so far:\n\n{context}\nNow the dev says:\n\n{text}"
        ),
        _ => text.to_owned(),
    }
}

fn cancelled() -> Value {
    json!({ "outcome": { "outcome": "cancelled" } })
}

fn log(agent: &str) -> Stdio {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("corvo")
        .join("agents");
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::File::create(dir.join(format!("{agent}.log"))))
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null())
}

fn find(program: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let runnable = |path: &Path| {
        path.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        return runnable(Path::new(program)).then(|| PathBuf::from(program));
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|path| runnable(path))
}
