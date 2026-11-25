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

// The names that lead through `symbols` to the one at `at`, which hold while lines move. A name
// that symbols before it under the same parent share takes `~n`, n being how many of them.
pub fn trail(symbols: &[Symbol], at: &[usize]) -> Option<Vec<String>> {
    let mut symbols = symbols;
    let mut trail = Vec::with_capacity(at.len());
    for &ix in at {
        let symbol = symbols.get(ix)?;
        trail.push(step(symbols, ix));
        symbols = symbol.nested();
    }
    Some(trail)
}

// The indexes a trail leads to through `symbols`; none when a name is no longer there.
pub fn find(symbols: &[Symbol], trail: &[String]) -> Option<Vec<usize>> {
    let mut symbols = symbols;
    let mut at = Vec::with_capacity(trail.len());
    for name in trail {
        let ix = (0..symbols.len()).find(|&ix| step(symbols, ix) == *name)?;
        at.push(ix);
        symbols = symbols[ix].nested();
    }
    Some(at)
}

fn step(siblings: &[Symbol], ix: usize) -> String {
    let name = siblings[ix].name();
    match siblings[..ix]
        .iter()
        .filter(|sibling| sibling.name() == name)
        .count()
    {
        0 => name.to_owned(),
        n => format!("{name}~{n}"),
    }
}
