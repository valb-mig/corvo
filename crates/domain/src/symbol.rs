#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Module,
    Class,
    Struct,
    Enum,
    Interface,
    Impl,
    Function,
    Method,
    Field,
    Variant,
    Constant,
    Variable,
    Parameter,
    Type,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    name: String,
    kind: Kind,
    line: u32,
    column: u32,
    first: u32,
    last: u32,
    children: Vec<Symbol>,
}

impl Symbol {
    pub fn new(name: impl Into<String>, kind: Kind, line: u32, column: u32) -> Self {
        Self {
            name: name.into(),
            kind,
            line,
            column,
            first: line,
            last: line,
            children: Vec::new(),
        }
    }

    // The lines the whole declaration takes, from its first to its last.
    pub fn span(mut self, first: u32, last: u32) -> Self {
        self.first = first;
        self.last = last;
        self
    }

    pub fn children(mut self, children: Vec<Symbol>) -> Self {
        self.children = children;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn column(&self) -> u32 {
        self.column
    }

    pub fn first_line(&self) -> u32 {
        self.first
    }

    pub fn last_line(&self) -> u32 {
        self.last
    }

    pub fn nested(&self) -> &[Symbol] {
        &self.children
    }
}
