// An agent the chat can talk to: how to start it, speaking ACP over its stdio, and how to install
// it when it is not there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSpec {
    id: String,
    name: String,
    program: String,
    args: Vec<String>,
    // The agent's own program, when another one starts it: without it the agent is not there.
    needs: Option<String>,
    install: String,
}

impl AgentSpec {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        program: impl Into<String>,
        args: &[&str],
        install: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            program: program.into(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            needs: None,
            install: install.into(),
        }
    }

    pub fn needs(mut self, program: impl Into<String>) -> Self {
        self.needs = Some(program.into());
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn needed(&self) -> Option<&str> {
        self.needs.as_deref()
    }

    pub fn install(&self) -> &str {
        &self.install
    }
}

// The agents known, the first being the one a new chat starts with.
pub fn agents() -> Vec<AgentSpec> {
    vec![
        AgentSpec::new(
            "claude",
            "Claude Code",
            "npx",
            &[
                "--prefer-offline",
                "-y",
                "@agentclientprotocol/claude-agent-acp",
            ],
            "Node with npx, and Claude Code logged in",
        )
        .needs("claude"),
        AgentSpec::new(
            "opencode",
            "opencode",
            "opencode",
            &["acp"],
            "curl -fsSL https://opencode.ai/install | bash",
        ),
        AgentSpec::new(
            "gemini",
            "Gemini CLI",
            "gemini",
            &["--experimental-acp"],
            "npm i -g @google/gemini-cli",
        ),
        AgentSpec::new(
            "codex",
            "Codex",
            "npx",
            &["--prefer-offline", "-y", "@zed-industries/codex-acp"],
            "Node with npx, and Codex logged in",
        )
        .needs("codex"),
    ]
}

// What the agent lets the dev pick for a session; each is there only if the agent offers it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub model: Option<Choice>,
    pub effort: Option<Choice>,
    pub mode: Option<Choice>,
}

// One pick: its values and the one in use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub current: String,
    pub values: Vec<Pick>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

impl Choice {
    pub fn current(&self) -> Option<&Pick> {
        self.values.iter().find(|pick| pick.id == self.current)
    }

    pub fn name(&self) -> &str {
        self.current().map_or(&self.current, |pick| &pick.name)
    }
}

impl Pick {
    // A mode that goes on without asking the dev first.
    pub fn is_risky(&self) -> bool {
        let words = format!("{} {}", self.id, self.name).to_lowercase();
        [
            "bypass",
            "yolo",
            "full access",
            "full-access",
            "dangerous",
            "skip permissions",
        ]
        .iter()
        .any(|word| words.contains(word))
    }
}
