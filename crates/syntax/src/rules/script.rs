use corvo_domain::SymbolKind as Kind;
use tree_sitter::Node;

use crate::walk::{Found, Local, Scope, bindings};

const FUNCTIONS: &[&str] = &[
    "arrow_function",
    "function_expression",
    "function",
    "generator_function",
];

pub fn script<'t>(node: Node<'t>, scope: Scope, _: &str) -> Option<Found<'t>> {
    let name = || node.child_by_field_name("name");
    let found = match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            Found::new(scope.callable(), name()?).inner(Scope::Body)
        }
        "class_declaration" | "abstract_class_declaration" => {
            Found::new(Kind::Class, name()?).inner(Scope::Type)
        }
        "interface_declaration" => Found::new(Kind::Interface, name()?).inner(Scope::Type),
        "enum_declaration" => Found::new(Kind::Enum, name()?).inner(Scope::Enum),
        "type_alias_declaration" if scope != Scope::Body => Found::new(Kind::Type, name()?),
        "internal_module" | "module" => Found::new(Kind::Module, name()?).inner(Scope::Top),
        "method_definition" => Found::new(Kind::Method, name()?).inner(Scope::Body),
        "method_signature" | "abstract_method_signature" => Found::new(Kind::Method, name()?),
        "public_field_definition" => member(node, name()?),
        "field_definition" => member(node, node.child_by_field_name("property")?),
        "property_signature" => Found::new(Kind::Field, name()?),
        "property_identifier" if scope == Scope::Enum => Found::new(Kind::Variant, node),
        "enum_assignment" if scope == Scope::Enum => Found::new(Kind::Variant, name()?),
        "variable_declarator" => variable(node, scope)?,
        _ => return None,
    };
    Some(found)
}

fn member<'t>(node: Node<'t>, name: Node<'t>) -> Found<'t> {
    if callable(node) {
        Found::new(Kind::Method, name).inner(Scope::Body)
    } else {
        Found::new(Kind::Field, name)
    }
}

fn variable<'t>(node: Node<'t>, scope: Scope) -> Option<Found<'t>> {
    let name = node
        .child_by_field_name("name")
        .filter(|name| name.kind() == "identifier")?;
    if callable(node) {
        return Some(Found::new(Kind::Function, name).inner(Scope::Body));
    }
    if scope == Scope::Body {
        return None;
    }
    let constant = node.parent().is_some_and(|parent| {
        parent.kind() == "lexical_declaration"
            && parent.child(0).is_some_and(|word| word.kind() == "const")
    });
    let kind = if constant {
        Kind::Constant
    } else {
        Kind::Variable
    };
    Some(Found::new(kind, name))
}

fn callable(node: Node) -> bool {
    node.child_by_field_name("value")
        .is_some_and(|value| FUNCTIONS.contains(&value.kind()))
}

const NAMES: &[&str] = &["identifier", "shorthand_property_identifier_pattern"];
const SKIP: &[&str] = &["type", "right"];

pub fn script_local<'t>(node: Node<'t>, _: &str) -> Option<Local<'t>> {
    let field = |name| node.child_by_field_name(name);
    let names = |pattern| bindings(pattern, NAMES, SKIP, &[]);
    let local = match node.kind() {
        "formal_parameters" => {
            let mut cursor = node.walk();
            let names = node
                .named_children(&mut cursor)
                .flat_map(|param| names(param.child_by_field_name("pattern").unwrap_or(param)))
                .collect();
            Local::new(Kind::Parameter, node, names)
        }
        "arrow_function" => {
            let param = field("parameter")?;
            Local::new(Kind::Parameter, param, vec![param])
        }
        "variable_declarator" => {
            let name = field("name")?;
            Local::new(Kind::Variable, name, names(name))
        }
        "for_in_statement" => {
            let left = field("left")?;
            Local::new(Kind::Variable, left, names(left))
        }
        _ => return None,
    };
    Some(local)
}
