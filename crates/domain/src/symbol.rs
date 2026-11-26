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

// The outline as the file tree nests it: what a type's own impl blocks hold goes into the type,
// after its fields, and an impl of a trait for it goes in as a child named for the trait. An
// impl whose type is not declared alongside it stays where it is.
pub fn gather_impls(symbols: Vec<Symbol>) -> Vec<Symbol> {
    let symbols: Vec<Symbol> = symbols
        .into_iter()
        .map(|mut symbol| {
            symbol.children = gather_impls(std::mem::take(&mut symbol.children));
            symbol
        })
        .collect();
    let mut own: Vec<Vec<Symbol>> = vec![Vec::new(); symbols.len()];
    let mut traits: Vec<Vec<Symbol>> = vec![Vec::new(); symbols.len()];
    let mut kept = vec![true; symbols.len()];
    for (ix, symbol) in symbols.iter().enumerate() {
        if symbol.kind != Kind::Impl {
            continue;
        }
        let (tr, ty) = match symbol.name.split_once(" for ") {
            Some((tr, ty)) => (Some(tr), ty),
            None => (None, symbol.name.as_str()),
        };
        let ty = bare(ty);
        let Some(home) = symbols.iter().position(|s| is_type(s.kind) && s.name == ty) else {
            continue;
        };
        kept[ix] = false;
        match tr {
            Some(tr) => {
                let mut symbol = symbol.clone();
                symbol.name = tr.to_owned();
                traits[home].push(symbol);
            }
            None => own[home].extend(symbol.children.iter().cloned()),
        }
    }
    symbols
        .into_iter()
        .zip(own.into_iter().zip(traits))
        .zip(kept)
        .filter_map(|((mut symbol, (own, traits)), kept)| {
            kept.then(|| {
                symbol.children.extend(own);
                symbol.children.extend(traits);
                symbol
            })
        })
        .collect()
}

// The indexes that lead through `symbols` to the innermost one whose lines hold `line`; a type
// holds the lines of what was gathered into it too.
pub fn path_to(symbols: &[Symbol], line: u32) -> Vec<usize> {
    let mut path = Vec::new();
    let mut symbols = symbols;
    while let Some(ix) = symbols.iter().rposition(|symbol| holds(symbol, line)) {
        path.push(ix);
        symbols = symbols[ix].nested();
    }
    path
}

fn holds(symbol: &Symbol, line: u32) -> bool {
    (symbol.first..=symbol.last).contains(&line)
        || symbol.children.iter().any(|child| holds(child, line))
}

fn is_type(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Struct | Kind::Enum | Kind::Class | Kind::Interface
    )
}

// A type's name without its path or generics: `crate::a::Foo<T>` is `Foo`.
fn bare(ty: &str) -> &str {
    let ty = ty.split('<').next().unwrap_or(ty).trim();
    ty.rsplit("::").next().unwrap_or(ty)
}
