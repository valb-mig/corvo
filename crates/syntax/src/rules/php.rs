use corvo_domain::SymbolKind as Kind;
use tree_sitter::Node;

use crate::walk::{Found, Local, Scope, bindings, kid};

pub fn php<'t>(node: Node<'t>, scope: Scope, _: &str) -> Option<Found<'t>> {
    let name = || node.child_by_field_name("name");
    let found = match node.kind() {
        "namespace_definition" => Found::new(Kind::Module, name()?).inner(Scope::Top),
        "class_declaration" => Found::new(Kind::Class, name()?).inner(Scope::Type),
        "interface_declaration" | "trait_declaration" => {
            Found::new(Kind::Interface, name()?).inner(Scope::Type)
        }
        "enum_declaration" => Found::new(Kind::Enum, name()?).inner(Scope::Enum),
        "enum_case" if scope == Scope::Enum => Found::new(Kind::Variant, name()?),
        "function_definition" => Found::new(Kind::Function, name()?).inner(Scope::Body),
        "method_declaration" => Found::new(Kind::Method, name()?).inner(Scope::Body),
        "property_element" if scope.typed() => Found::new(Kind::Field, name()?),
        "const_element" if scope != Scope::Body => {
            Found::new(Kind::Constant, kid(node, &["name"])?)
        }
        _ => return None,
    };
    Some(found)
}

const MEMBERS: &[&str] = &[
    "member_access_expression",
    "nullsafe_member_access_expression",
    "scoped_property_access_expression",
    "subscript_expression",
];

pub fn php_local<'t>(node: Node<'t>, text: &str) -> Option<Local<'t>> {
    let names = |pattern| {
        bindings(pattern, &["variable_name"], &[], MEMBERS)
            .into_iter()
            .filter(|name| &text[name.byte_range()] != "$this")
            .collect()
    };
    let local = match node.kind() {
        "simple_parameter" | "variadic_parameter" | "property_promotion_parameter" => Local::new(
            Kind::Parameter,
            node,
            vec![node.child_by_field_name("name")?],
        ),
        "assignment_expression" | "reference_assignment_expression" => {
            let left = node.child_by_field_name("left")?;
            Local::new(Kind::Variable, left, names(left))
        }
        "foreach_statement" => {
            // foreach ($items as $key => $value): the pattern follows the list.
            let pattern = node
                .named_child(1)
                .filter(|pattern| pattern.kind() != "compound_statement")?;
            Local::new(Kind::Variable, pattern, names(pattern))
        }
        _ => return None,
    };
    Some(local)
}
