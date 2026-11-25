use corvo_domain::{Symbol, SymbolKind as Kind};
use tree_sitter::Node;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Top,
    Type,
    Enum,
    Body,
}

impl Scope {
    pub fn typed(self) -> bool {
        matches!(self, Scope::Type | Scope::Enum)
    }

    pub fn callable(self) -> Kind {
        if self.typed() {
            Kind::Method
        } else {
            Kind::Function
        }
    }
}

pub struct Found<'t> {
    kind: Kind,
    name: Node<'t>,
    label: Option<String>,
    inner: Option<Scope>,
}

impl<'t> Found<'t> {
    pub fn new(kind: Kind, name: Node<'t>) -> Self {
        Self {
            kind,
            name,
            label: None,
            inner: None,
        }
    }

    pub fn inner(mut self, scope: Scope) -> Self {
        self.inner = Some(scope);
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

// Parameters and local variables of a function body: the names bound by one node, and the part
// of that node that holds them, so the walk goes on through the rest (values, loop bodies) but
// not through the names again.
pub struct Local<'t> {
    kind: Kind,
    names: Vec<Node<'t>>,
    pattern: Node<'t>,
}

impl<'t> Local<'t> {
    pub fn new(kind: Kind, pattern: Node<'t>, names: Vec<Node<'t>>) -> Self {
        Self {
            kind,
            names,
            pattern,
        }
    }
}

pub type Rule = for<'t> fn(Node<'t>, Scope, &str) -> Option<Found<'t>>;
pub type Locals = for<'t> fn(Node<'t>, &str) -> Option<Local<'t>>;

pub struct Walk<'a> {
    rule: Rule,
    locals: Option<Locals>,
    text: &'a str,
}

impl<'a> Walk<'a> {
    pub fn new(rule: Rule, text: &'a str) -> Self {
        Self {
            rule,
            locals: None,
            text,
        }
    }

    pub fn locals(mut self, locals: Locals) -> Self {
        self.locals = Some(locals);
        self
    }

    pub fn within(&self, node: Node, scope: Scope) -> Vec<Symbol> {
        let mut out = Vec::new();
        self.children(node, scope, &mut out);
        out
    }

    fn children(&self, node: Node, scope: Scope, out: &mut Vec<Symbol>) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, scope, out);
        }
    }

    fn visit(&self, node: Node, scope: Scope, out: &mut Vec<Symbol>) {
        if let Some(found) = (self.rule)(node, scope, self.text) {
            let children = found
                .inner
                .map(|inner| self.within(node, inner))
                .unwrap_or_default();
            let (first, last) = (node.start_position().row, node.end_position().row);
            let symbol = self.symbol(found.name, found.kind, found.label);
            out.push(symbol.span(first as u32, last as u32).children(children));
            return;
        }
        let local = match (scope, self.locals) {
            (Scope::Body, Some(locals)) => locals(node, self.text),
            _ => None,
        };
        let Some(local) = local else {
            self.children(node, scope, out);
            return;
        };
        for name in local.names {
            let symbol = self.symbol(name, local.kind, None);
            // A name bound again (shadowed, reassigned) shows once, where it first comes.
            let seen = out.iter().any(|seen| {
                seen.name() == symbol.name()
                    && matches!(seen.kind(), Kind::Variable | Kind::Parameter)
            });
            if !seen {
                out.push(symbol);
            }
        }
        if local.pattern == node {
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child != local.pattern {
                self.visit(child, scope, out);
            }
        }
    }

    fn symbol(&self, name: Node, kind: Kind, label: Option<String>) -> Symbol {
        let text = label.unwrap_or_else(|| self.text[name.byte_range()].to_owned());
        let at = name.start_position();
        let start = name.start_byte();
        let column = self.text[start - at.column..start].chars().count();
        Symbol::new(text, kind, at.row as u32, column as u32)
    }
}

// The nodes of the given kinds under a pattern, leaving out what hangs from the skipped fields
// (types, default values) and what sits under the stop kinds (paths, member access).
pub fn bindings<'t>(node: Node<'t>, kinds: &[&str], skip: &[&str], stop: &[&str]) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    collect(node, kinds, skip, stop, &mut out);
    out
}

fn collect<'t>(
    node: Node<'t>,
    kinds: &[&str],
    skip: &[&str],
    stop: &[&str],
    out: &mut Vec<Node<'t>>,
) {
    if kinds.contains(&node.kind()) {
        out.push(node);
        return;
    }
    if stop.contains(&node.kind()) {
        return;
    }
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let skipped = cursor
            .field_name()
            .is_some_and(|field| skip.contains(&field));
        let child = cursor.node();
        if child.is_named() && !skipped {
            collect(child, kinds, skip, stop, out);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

pub fn kid<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()))
}

pub fn has_token(node: Node, token: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| !child.is_named() && child.kind() == token)
}
