use crate::Lang;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    name: String,
    command: String,
    args: Vec<String>,
    langs: Vec<Lang>,
    markers: Vec<String>,
    stack: Option<String>,
    hint: String,
}

impl Spec {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            langs: Vec::new(),
            markers: Vec::new(),
            stack: None,
            hint: String::new(),
        }
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn langs(mut self, langs: impl IntoIterator<Item = Lang>) -> Self {
        self.langs = langs.into_iter().collect();
        self
    }

    pub fn markers<I, S>(mut self, markers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.markers = markers.into_iter().map(Into::into).collect();
        self
    }

    pub fn stack(mut self, stack: impl Into<String>) -> Self {
        self.stack = Some(stack.into());
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn arguments(&self) -> &[String] {
        &self.args
    }

    pub fn serves(&self, lang: Lang) -> bool {
        self.langs.contains(&lang)
    }

    pub fn root_markers(&self) -> &[String] {
        &self.markers
    }

    pub fn stack_name(&self) -> Option<&str> {
        self.stack.as_deref()
    }

    pub fn install_hint(&self) -> &str {
        &self.hint
    }
}
