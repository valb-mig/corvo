use std::io;
use std::path::{Path, PathBuf};

use corvo_domain::port::ChatStore;
use corvo_domain::{Chat, Diff, End, Entry, Part, Reply, Status, Step, StepKind, Uuid};
use serde::{Deserialize, Serialize};

// One file per chat, in a folder per workspace. A question the agent was waiting on is not kept:
// the agent that asked it is gone with the app.
#[derive(Deserialize, Serialize)]
struct File {
    id: u64,
    title: String,
    agent: String,
    project: PathBuf,
    #[serde(default)]
    archived: bool,
    created: u64,
    updated: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    entries: Vec<Kept>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Kept {
    You {
        text: String,
    },
    Reply {
        agent: String,
        parts: Vec<KeptPart>,
        started: u64,
        #[serde(default)]
        secs: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<KeptEnd>,
    },
    Switch {
        agent: String,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum KeptPart {
    Text {
        text: String,
    },
    Step {
        id: String,
        kind: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        status: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        diffs: Vec<KeptDiff>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        output: String,
    },
}

#[derive(Deserialize, Serialize)]
struct KeptDiff {
    path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    old: Option<String>,
    new: String,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum KeptEnd {
    Done,
    Stopped,
    Failed { why: String },
}

impl From<&Chat> for File {
    fn from(chat: &Chat) -> Self {
        Self {
            id: chat.id(),
            title: chat.title().to_owned(),
            agent: chat.agent().to_owned(),
            project: chat.project().to_path_buf(),
            archived: chat.is_archived(),
            created: chat.created(),
            updated: chat.updated(),
            session: chat.session().map(str::to_owned),
            entries: chat.entries().iter().map(Kept::from).collect(),
        }
    }
}

impl From<File> for Chat {
    fn from(file: File) -> Self {
        Chat::kept(
            file.id,
            file.title,
            file.agent,
            file.project,
            file.archived,
            file.created,
            file.updated,
            file.entries.into_iter().map(Entry::from).collect(),
            file.session,
        )
    }
}

impl From<&Entry> for Kept {
    fn from(entry: &Entry) -> Self {
        match entry {
            Entry::You(text) => Kept::You { text: text.clone() },
            Entry::Switch(agent) => Kept::Switch {
                agent: agent.clone(),
            },
            Entry::Reply(reply) => Kept::Reply {
                agent: reply.agent().to_owned(),
                parts: reply.parts().iter().map(KeptPart::from).collect(),
                started: reply.started(),
                secs: reply.secs(),
                end: reply.end().map(|end| match end {
                    End::Done => KeptEnd::Done,
                    End::Stopped => KeptEnd::Stopped,
                    End::Failed(why) => KeptEnd::Failed { why: why.clone() },
                }),
            },
        }
    }
}

impl From<Kept> for Entry {
    fn from(kept: Kept) -> Self {
        match kept {
            Kept::You { text } => Entry::You(text),
            Kept::Switch { agent } => Entry::Switch(agent),
            Kept::Reply {
                agent,
                parts,
                started,
                secs,
                end,
            } => Entry::Reply(Reply::kept(
                agent,
                parts.into_iter().map(Part::from).collect(),
                started,
                end.map(|end| match end {
                    KeptEnd::Done => End::Done,
                    KeptEnd::Stopped => End::Stopped,
                    KeptEnd::Failed { why } => End::Failed(why),
                }),
                secs,
            )),
        }
    }
}

impl From<&Part> for KeptPart {
    fn from(part: &Part) -> Self {
        match part {
            Part::Text(text) => KeptPart::Text { text: text.clone() },
            Part::Step(step) => KeptPart::Step {
                id: step.id().to_owned(),
                kind: kind_name(step.kind()).to_owned(),
                title: step.title().to_owned(),
                target: step.target().map(str::to_owned),
                status: status_name(step.status()).to_owned(),
                diffs: step
                    .diffs()
                    .iter()
                    .map(|diff| KeptDiff {
                        path: diff.path.clone(),
                        old: diff.old.clone(),
                        new: diff.new.clone(),
                    })
                    .collect(),
                output: step.output().to_owned(),
            },
        }
    }
}

impl From<KeptPart> for Part {
    fn from(part: KeptPart) -> Self {
        match part {
            KeptPart::Text { text } => Part::Text(text),
            KeptPart::Step {
                id,
                kind,
                title,
                target,
                status,
                diffs,
                output,
            } => {
                let mut step = Step::new(id, kind_of(&kind), title);
                step.set_target(target);
                step.set_status(status_of(&status));
                step.set_diffs(
                    diffs
                        .into_iter()
                        .map(|diff| Diff {
                            path: diff.path,
                            old: diff.old,
                            new: diff.new,
                        })
                        .collect(),
                );
                step.set_output(output);
                Part::Step(step)
            }
        }
    }
}

fn kind_name(kind: StepKind) -> &'static str {
    match kind {
        StepKind::Read => "read",
        StepKind::Edit => "edit",
        StepKind::Delete => "delete",
        StepKind::Move => "move",
        StepKind::Search => "search",
        StepKind::Run => "run",
        StepKind::Think => "think",
        StepKind::Fetch => "fetch",
        StepKind::Other => "other",
    }
}

fn kind_of(name: &str) -> StepKind {
    match name {
        "read" => StepKind::Read,
        "edit" => StepKind::Edit,
        "delete" => StepKind::Delete,
        "move" => StepKind::Move,
        "search" => StepKind::Search,
        "run" => StepKind::Run,
        "think" => StepKind::Think,
        "fetch" => StepKind::Fetch,
        _ => StepKind::Other,
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Pending => "pending",
        Status::Running => "running",
        Status::Done => "done",
        Status::Failed => "failed",
    }
}

fn status_of(name: &str) -> Status {
    match name {
        "pending" => Status::Pending,
        "running" => Status::Running,
        "done" => Status::Done,
        _ => Status::Failed,
    }
}

pub struct JsonChats {
    dir: PathBuf,
}

impl JsonChats {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn config() -> io::Result<Self> {
        Ok(Self::new(crate::store::config_dir()?.join("chats")))
    }

    // The chats of a removed workspace go with it.
    pub fn forget(&self, workspace: Uuid) -> io::Result<()> {
        match std::fs::remove_dir_all(self.folder(workspace)) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            done => done,
        }
    }

    fn folder(&self, workspace: Uuid) -> PathBuf {
        self.dir.join(workspace.to_string())
    }

    fn path(&self, workspace: Uuid, id: u64) -> PathBuf {
        self.folder(workspace).join(format!("{id}.json"))
    }
}

impl ChatStore for JsonChats {
    // A file that no longer reads is left where it is, out of the list.
    fn load(&self, workspace: Uuid) -> io::Result<Vec<Chat>> {
        let dir = self.folder(workspace);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let mut chats = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            if let Ok(chat) = read(&path) {
                chats.push(chat);
            }
        }
        Ok(chats)
    }

    fn save(&self, workspace: Uuid, chat: &Chat) -> io::Result<()> {
        let path = self.path(workspace, chat.id());
        let text = serde_json::to_string_pretty(&File::from(chat)).map_err(io::Error::other)?;
        std::fs::create_dir_all(self.folder(workspace))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    fn remove(&self, workspace: Uuid, id: u64) -> io::Result<()> {
        match std::fs::remove_file(self.path(workspace, id)) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            done => done,
        }
    }
}

fn read(path: &Path) -> io::Result<Chat> {
    let text = std::fs::read_to_string(path)?;
    let file: File = serde_json::from_str(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {err}", path.display()),
        )
    })?;
    Ok(file.into())
}
