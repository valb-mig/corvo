use corvo_domain::SymbolKind as Kind;
use tree_sitter::Node;

use crate::walk::{Found, Local, Scope, bindings};

pub fn python<'t>(node: Node<'t>, scope: Scope, text: &str) -> Option<Found<'t>> {
    let name = || node.child_by_field_name("name");
    let found = match node.kind() {
        "class_definition" => Found::new(Kind::Class, name()?).inner(Scope::Type),
        "function_definition" => Found::new(scope.callable(), name()?).inner(Scope::Body),
        "assignment" if scope != Scope::Body => {
            let left = node
                .child_by_field_name("left")
                .filter(|left| left.kind() == "identifier")?;
            let kind = if scope.typed() {
                Kind::Field
            } else if shouting(&text[left.byte_range()]) {
                Kind::Constant
            } else {
                Kind::Variable
            };
            Found::new(kind, left)
        }
        _ => return None,
    };
    Some(found)
}

fn shouting(name: &str) -> bool {
    name.chars().any(char::is_alphabetic) && !name.chars().any(char::is_lowercase)
}

const MEMBERS: &[&str] = &["attribute", "subscript"];

pub fn python_local<'t>(node: Node<'t>, text: &str) -> Option<Local<'t>> {
    let names = |pattern| bindings(pattern, &["identifier"], &["type", "value"], MEMBERS);
    let local = match node.kind() {
        "parameters" | "lambda_parameters" => {
            let names = names(node)
                .into_iter()
                .filter(|name| !matches!(&text[name.byte_range()], "self" | "cls"))
                .collect();
            Local::new(Kind::Parameter, node, names)
        }
        "assignment" | "for_statement" => {
            let left = node.child_by_field_name("left")?;
            Local::new(Kind::Variable, left, names(left))
        }
        _ => return None,
    };
    Some(local)
}
