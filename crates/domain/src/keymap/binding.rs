use super::{Command, Scope};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    chord: String,
    command: Command,
    scope: Scope,
}

impl Binding {
    pub fn new(chord: impl Into<String>, command: Command, scope: Scope) -> Self {
        Self {
            chord: chord.into(),
            command,
            scope,
        }
    }

    pub fn chord(&self) -> &str {
        &self.chord
    }

    pub fn command(&self) -> Command {
        self.command
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }
}
