use agent_client_protocol_schema::v1::{
    ContentBlock, PermissionOption, PermissionOptionKind, SessionConfigKind, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOptions, SessionModeState, StopReason,
    ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdateFields, ToolKind,
};
use corvo_domain::{Answer, Choice, Diff, End, Options, Pick, Status, Step, StepKind};
use serde_json::Value;

// The id the mode takes when it comes from the agent's older session modes, set with
// `session/set_mode` instead of as a config option.
pub const MODE: &str = "\u{0}mode";

// News of a step: what changed in it. A new step comes with all of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Patch {
    pub kind: Option<StepKind>,
    pub title: Option<String>,
    pub status: Option<Status>,
    pub target: Option<String>,
    pub diffs: Option<Vec<Diff>>,
    pub output: Option<String>,
}

impl Patch {
    pub fn apply(&self, step: &mut Step) {
        if let Some(kind) = self.kind {
            step.set_kind(kind);
        }
        if let Some(title) = &self.title {
            step.set_title(title.clone());
        }
        if let Some(status) = self.status {
            step.set_status(status);
        }
        if let Some(target) = &self.target {
            step.set_target(Some(target.clone()));
        }
        if let Some(diffs) = &self.diffs {
            step.set_diffs(diffs.clone());
        }
        if let Some(output) = &self.output {
            step.set_output(output.clone());
        }
    }

    pub fn step(&self, id: &str) -> Step {
        let mut step = Step::new(id, StepKind::Other, "");
        self.apply(&mut step);
        step
    }
}

pub fn call(call: &ToolCall) -> Patch {
    Patch {
        kind: Some(kind(&call.kind)),
        title: Some(call.title.clone()),
        status: Some(status(&call.status)),
        target: target(
            call.locations.first().map(|at| &at.path),
            call.raw_input.as_ref(),
        ),
        diffs: Some(diffs(&call.content)),
        output: Some(output(&call.content, call.raw_output.as_ref())),
    }
}

pub fn update(fields: &ToolCallUpdateFields) -> Patch {
    let diffs = fields.content.as_deref().map(diffs);
    let output = fields
        .content
        .as_deref()
        .map(|content| output(content, fields.raw_output.as_ref()))
        .filter(|text| !text.is_empty());
    Patch {
        kind: fields.kind.as_ref().map(kind),
        title: fields.title.clone(),
        status: fields.status.as_ref().map(status),
        target: target(
            fields
                .locations
                .as_ref()
                .and_then(|locations| locations.first())
                .map(|at| &at.path),
            fields.raw_input.as_ref(),
        ),
        diffs: diffs.filter(|diffs| !diffs.is_empty()),
        output,
    }
}

fn kind(kind: &ToolKind) -> StepKind {
    match kind {
        ToolKind::Read => StepKind::Read,
        ToolKind::Edit => StepKind::Edit,
        ToolKind::Delete => StepKind::Delete,
        ToolKind::Move => StepKind::Move,
        ToolKind::Search => StepKind::Search,
        ToolKind::Execute => StepKind::Run,
        ToolKind::Think => StepKind::Think,
        ToolKind::Fetch => StepKind::Fetch,
        _ => StepKind::Other,
    }
}

fn status(status: &ToolCallStatus) -> Status {
    match status {
        ToolCallStatus::Pending => Status::Pending,
        ToolCallStatus::InProgress => Status::Running,
        ToolCallStatus::Completed => Status::Done,
        _ => Status::Failed,
    }
}

// What the step works on: the file it points at, or the command it runs.
fn target(path: Option<&std::path::PathBuf>, input: Option<&Value>) -> Option<String> {
    if let Some(path) = path {
        return Some(path.display().to_string());
    }
    let input = input?;
    ["command", "file_path", "path", "pattern", "url"]
        .iter()
        .find_map(|key| input.get(key))
        .and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Array(parts) => Some(
                parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            _ => None,
        })
}

fn diffs(content: &[ToolCallContent]) -> Vec<Diff> {
    content
        .iter()
        .filter_map(|item| match item {
            ToolCallContent::Diff(diff) => Some(Diff {
                path: diff.path.clone(),
                old: diff.old_text.clone(),
                new: diff.new_text.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn output(content: &[ToolCallContent], raw: Option<&Value>) -> String {
    let text: Vec<&str> = content
        .iter()
        .filter_map(|item| match item {
            ToolCallContent::Content(content) => match &content.content {
                ContentBlock::Text(text) => Some(text.text.as_str()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if !text.is_empty() {
        return text.join("\n");
    }
    match raw {
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

// The picks the agent offers, by what they are for. Older agents give only their modes.
pub fn options(config: &[SessionConfigOption], modes: Option<&SessionModeState>) -> Options {
    let mut options = Options::default();
    for option in config {
        let SessionConfigKind::Select(select) = &option.kind else {
            continue;
        };
        let values = match &select.options {
            SessionConfigSelectOptions::Ungrouped(values) => values.iter().collect::<Vec<_>>(),
            SessionConfigSelectOptions::Grouped(groups) => {
                groups.iter().flat_map(|group| &group.options).collect()
            }
            _ => continue,
        };
        let choice = Choice {
            id: option.id.0.to_string(),
            current: select.current_value.0.to_string(),
            values: values
                .into_iter()
                .map(|value| Pick {
                    id: value.value.0.to_string(),
                    name: value.name.clone(),
                    description: value.description.clone(),
                })
                .collect(),
        };
        let slot = match &option.category {
            Some(SessionConfigOptionCategory::Model) => &mut options.model,
            Some(SessionConfigOptionCategory::ThoughtLevel) => &mut options.effort,
            Some(SessionConfigOptionCategory::Mode) => &mut options.mode,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(choice);
        }
    }
    if options.mode.is_none()
        && let Some(modes) = modes
        && !modes.available_modes.is_empty()
    {
        options.mode = Some(Choice {
            id: MODE.to_owned(),
            current: modes.current_mode_id.0.to_string(),
            values: modes
                .available_modes
                .iter()
                .map(|mode| Pick {
                    id: mode.id.0.to_string(),
                    name: mode.name.clone(),
                    description: mode.description.clone(),
                })
                .collect(),
        });
    }
    options
}

pub fn answers(options: &[PermissionOption]) -> Vec<Answer> {
    options
        .iter()
        .map(|option| Answer {
            id: option.option_id.0.to_string(),
            name: option.name.clone(),
            allow: matches!(
                option.kind,
                PermissionOptionKind::AllowOnce | PermissionOptionKind::AllowAlways
            ),
        })
        .collect()
}

pub fn end(reason: &StopReason) -> End {
    match reason {
        StopReason::EndTurn => End::Done,
        StopReason::Cancelled => End::Stopped,
        StopReason::MaxTokens => End::Failed("the reply hit the token limit".to_owned()),
        StopReason::MaxTurnRequests => End::Failed("the turn hit its step limit".to_owned()),
        StopReason::Refusal => End::Failed("the agent refused".to_owned()),
        _ => End::Done,
    }
}
