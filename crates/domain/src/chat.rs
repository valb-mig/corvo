use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// A conversation with agents, kept with the workspace it belongs to. It goes on in the folder it
// started in, with one agent at a time: a switch of agent is marked in it and the chat goes on.
#[derive(Clone, Debug, PartialEq)]
pub struct Chat {
    id: u64,
    title: String,
    agent: String,
    project: PathBuf,
    archived: bool,
    created: u64,
    updated: u64,
    entries: Vec<Entry>,
    // The agent's own session, to take the chat back up with it.
    session: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    You(String),
    Reply(Reply),
    // Another agent goes on from here.
    Switch(String),
}

// What an agent answered to one message: its words and the steps it took, in the order they came.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    agent: String,
    parts: Vec<Part>,
    started: u64,
    end: Option<End>,
    // How long it took, once it ended.
    secs: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(String),
    Step(Step),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    Done,
    Stopped,
    Failed(String),
}

// One thing the agent did: read, edit, search, run…, with what it touched and what came of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    id: String,
    kind: StepKind,
    title: String,
    target: Option<String>,
    status: Status,
    diffs: Vec<Diff>,
    output: String,
    // The agent asks the dev before going on.
    ask: Option<Ask>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Run,
    Think,
    Fetch,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    pub path: PathBuf,
    pub old: Option<String>,
    pub new: String,
}

// A question the agent waits on, with the answers it offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub id: u64,
    pub options: Vec<Answer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub id: String,
    pub name: String,
    pub allow: bool,
}

const TITLE: usize = 40;

impl Chat {
    pub fn new(id: u64, agent: impl Into<String>, project: impl Into<PathBuf>, now: u64) -> Self {
        Self {
            id,
            title: "New chat".to_owned(),
            agent: agent.into(),
            project: project.into(),
            archived: false,
            created: now,
            updated: now,
            entries: Vec::new(),
            session: None,
        }
    }

    // A chat as it was kept.
    #[allow(clippy::too_many_arguments)]
    pub fn kept(
        id: u64,
        title: String,
        agent: String,
        project: PathBuf,
        archived: bool,
        created: u64,
        updated: u64,
        entries: Vec<Entry>,
        session: Option<String>,
    ) -> Self {
        Self {
            id,
            title,
            agent,
            project,
            archived,
            created,
            updated,
            entries,
            session,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn agent(&self) -> &str {
        &self.agent
    }

    pub fn project(&self) -> &Path {
        &self.project
    }

    pub fn is_archived(&self) -> bool {
        self.archived
    }

    pub fn created(&self) -> u64 {
        self.created
    }

    pub fn updated(&self) -> u64 {
        self.updated
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub fn set_session(&mut self, session: Option<String>) {
        self.session = session;
    }

    // A blank title leaves the chat as it was.
    pub fn rename(&mut self, title: &str) {
        let title = title.trim();
        if !title.is_empty() {
            self.title = title.to_owned();
        }
    }

    pub fn archive(&mut self, archived: bool) {
        self.archived = archived;
    }

    pub fn turns(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, Entry::You(_)))
            .count()
    }

    // The dev writes: the reply of the chat's agent starts. The first message names the chat.
    pub fn ask(&mut self, text: &str, now: u64) {
        if self.turns() == 0 {
            self.title = title(text);
        }
        self.entries.push(Entry::You(text.trim().to_owned()));
        self.entries
            .push(Entry::Reply(Reply::new(self.agent.clone(), now)));
        self.updated = now;
    }

    // Another agent goes on with the chat; the old session stays with the old agent.
    pub fn switch(&mut self, agent: &str) {
        if self.agent == agent {
            return;
        }
        self.agent = agent.to_owned();
        self.session = None;
        if self.turns() == 0 {
            return;
        }
        // Two switches in a row only keep the last.
        if matches!(self.entries.last(), Some(Entry::Switch(_))) {
            self.entries.pop();
        }
        self.entries.push(Entry::Switch(agent.to_owned()));
    }

    pub fn last_reply(&self) -> Option<&Reply> {
        self.entries.iter().rev().find_map(|entry| match entry {
            Entry::Reply(reply) => Some(reply),
            _ => None,
        })
    }

    // The message the last reply answers.
    pub fn last_ask(&self) -> Option<&str> {
        self.entries.iter().rev().find_map(|entry| match entry {
            Entry::You(text) => Some(text.as_str()),
            _ => None,
        })
    }

    // The reply still coming in, if any.
    pub fn reply_mut(&mut self) -> Option<&mut Reply> {
        match self.entries.last_mut() {
            Some(Entry::Reply(reply)) if reply.end.is_none() => Some(reply),
            _ => None,
        }
    }

    pub fn is_working(&self) -> bool {
        matches!(self.entries.last(), Some(Entry::Reply(reply)) if reply.end.is_none())
    }

    pub fn is_waiting(&self) -> bool {
        self.last_reply()
            .is_some_and(|reply| reply.end.is_none() && reply.asking().is_some())
    }

    // The reply is over: what it asked is no longer waited on.
    pub fn end(&mut self, end: End, now: u64) {
        if let Some(reply) = self.reply_mut() {
            reply.secs = now.saturating_sub(reply.started);
            reply.end = Some(end);
            for part in &mut reply.parts {
                if let Part::Step(step) = part {
                    step.ask = None;
                }
            }
        }
        self.updated = now;
    }

    // A chat left working when the app closed has nothing coming in anymore.
    pub fn settle(&mut self) {
        if let Some(reply) = self.reply_mut() {
            reply.end = Some(End::Stopped);
            for part in &mut reply.parts {
                if let Part::Step(step) = part {
                    step.ask = None;
                    if matches!(step.status, Status::Pending | Status::Running) {
                        step.status = Status::Failed;
                    }
                }
            }
        }
    }

    // Found by its title, what the dev wrote or what the agents answered.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty() || self.title.to_lowercase().contains(&query) || self.hit(&query).is_some()
    }

    // Where the query is in the conversation, when it is not in the title: a bit of text around
    // it, the match starting at the char offset given.
    pub fn snippet(&self, query: &str) -> Option<(String, usize)> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || self.title.to_lowercase().contains(&query) {
            return None;
        }
        let text = self.hit(&query)?;
        let lower = text.to_lowercase();
        let at = lower.find(&query)?;
        let start = lower[..at]
            .char_indices()
            .rev()
            .nth(15)
            .map_or(0, |(ix, _)| ix);
        let lead = if start > 0 { "…" } else { "" };
        let piece: String = text.get(start..)?.chars().take(80).collect();
        let offset = lead.chars().count() + lower[start..at].chars().count();
        Some((format!("{lead}{piece}"), offset))
    }

    fn hit(&self, query: &str) -> Option<&str> {
        self.entries.iter().find_map(|entry| match entry {
            Entry::You(text) if text.to_lowercase().contains(query) => Some(text.as_str()),
            Entry::Reply(reply) => reply.parts.iter().find_map(|part| match part {
                Part::Text(text) if text.to_lowercase().contains(query) => Some(text.as_str()),
                _ => None,
            }),
            _ => None,
        })
    }

    // The conversation so far as plain text, for an agent that comes into the chat after it began.
    pub fn transcript(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            match entry {
                Entry::You(text) => out.push_str(&format!("Dev: {text}\n\n")),
                Entry::Reply(reply) => {
                    let mut said = String::new();
                    for part in &reply.parts {
                        match part {
                            Part::Text(text) => said.push_str(text),
                            Part::Step(step) => said.push_str(&format!("\n[{}]\n", step.title)),
                        }
                    }
                    let said = said.trim();
                    if !said.is_empty() {
                        out.push_str(&format!("{}: {said}\n\n", reply.agent));
                    }
                }
                Entry::Switch(agent) => out.push_str(&format!("(switched to {agent})\n\n")),
            }
        }
        out
    }
}

impl Reply {
    pub fn new(agent: String, started: u64) -> Self {
        Self {
            agent,
            parts: Vec::new(),
            started,
            end: None,
            secs: 0,
        }
    }

    pub fn kept(
        agent: String,
        parts: Vec<Part>,
        started: u64,
        end: Option<End>,
        secs: u64,
    ) -> Self {
        Self {
            agent,
            parts,
            started,
            end,
            secs,
        }
    }

    pub fn agent(&self) -> &str {
        &self.agent
    }

    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    pub fn started(&self) -> u64 {
        self.started
    }

    pub fn end(&self) -> Option<&End> {
        self.end.as_ref()
    }

    pub fn secs(&self) -> u64 {
        self.secs
    }

    // Words come in pieces: one after another make one text.
    pub fn say(&mut self, text: &str) {
        match self.parts.last_mut() {
            Some(Part::Text(last)) => last.push_str(text),
            _ => self.parts.push(Part::Text(text.to_owned())),
        }
    }

    // A step comes in, or news of one already there.
    pub fn step(&mut self, step: Step) {
        match self.step_mut(&step.id) {
            Some(known) => *known = step,
            None => self.parts.push(Part::Step(step)),
        }
    }

    pub fn step_mut(&mut self, id: &str) -> Option<&mut Step> {
        self.parts.iter_mut().find_map(|part| match part {
            Part::Step(step) if step.id == id => Some(step),
            _ => None,
        })
    }

    pub fn steps(&self) -> impl Iterator<Item = &Step> {
        self.parts.iter().filter_map(|part| match part {
            Part::Step(step) => Some(step),
            _ => None,
        })
    }

    // The step that waits on the dev.
    pub fn asking(&self) -> Option<&Step> {
        self.steps().find(|step| step.ask.is_some())
    }

    // What the agent is at now: the step running, or the last one.
    pub fn current(&self) -> Option<&Step> {
        self.steps()
            .filter(|step| matches!(step.status, Status::Pending | Status::Running))
            .last()
            .or_else(|| self.steps().last())
    }

    // The files it changed, each once.
    pub fn changed(&self) -> usize {
        let mut files = BTreeSet::new();
        for step in self.steps() {
            if !matches!(
                step.kind,
                StepKind::Edit | StepKind::Delete | StepKind::Move
            ) {
                continue;
            }
            if step.status == Status::Failed {
                continue;
            }
            for diff in &step.diffs {
                files.insert(diff.path.display().to_string());
            }
            if step.diffs.is_empty()
                && let Some(target) = &step.target
            {
                files.insert(target.clone());
            }
        }
        files.len()
    }

    // A line for the end of the reply: "2 files changed", or how it ended.
    pub fn summary(&self) -> String {
        match &self.end {
            Some(End::Stopped) => "Stopped".to_owned(),
            Some(End::Failed(why)) => format!("Failed: {why}"),
            _ => match self.changed() {
                0 => "No files changed".to_owned(),
                1 => "1 file changed".to_owned(),
                n => format!("{n} files changed"),
            },
        }
    }
}

impl Step {
    pub fn new(id: impl Into<String>, kind: StepKind, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            title: title.into(),
            target: None,
            status: Status::Pending,
            diffs: Vec::new(),
            output: String::new(),
            ask: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn kind(&self) -> StepKind {
        self.kind
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn diffs(&self) -> &[Diff] {
        &self.diffs
    }

    pub fn output(&self) -> &str {
        &self.output
    }

    pub fn ask(&self) -> Option<&Ask> {
        self.ask.as_ref()
    }

    pub fn set_kind(&mut self, kind: StepKind) {
        self.kind = kind;
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn set_target(&mut self, target: Option<String>) {
        self.target = target;
    }

    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    pub fn set_diffs(&mut self, diffs: Vec<Diff>) {
        self.diffs = diffs;
    }

    pub fn set_output(&mut self, output: impl Into<String>) {
        self.output = output.into();
    }

    pub fn set_ask(&mut self, ask: Option<Ask>) {
        self.ask = ask;
    }

    // Lines added and taken out across its diffs.
    pub fn lines(&self) -> (usize, usize) {
        self.diffs.iter().fold((0, 0), |(add, del), diff| {
            let old = diff.old.as_deref().map_or(0, |old| old.lines().count());
            let new = diff.new.lines().count();
            let same = common(diff.old.as_deref().unwrap_or(""), &diff.new);
            (add + new - same, del + old - same)
        })
    }
}

// Lines the old and new text keep at their start and end, the rest being what changed.
fn common(old: &str, new: &str) -> usize {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    head + tail
}

fn title(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= TITLE {
        return text;
    }
    let cut: String = text.chars().take(TITLE).collect();
    format!("{}…", cut.trim_end())
}

// How long ago, short: "now", "12m", "3h", "2d", "3w".
pub fn ago(then: u64, now: u64) -> String {
    let secs = now.saturating_sub(then);
    match secs {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", secs / 60),
        3600..86_400 => format!("{}h", secs / 3600),
        86_400..604_800 => format!("{}d", secs / 86_400),
        _ => format!("{}w", secs / 604_800),
    }
}
