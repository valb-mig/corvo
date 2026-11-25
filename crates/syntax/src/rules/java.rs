use corvo_domain::SymbolKind as Kind;
use tree_sitter::Node;

use crate::walk::{Found, Local, Scope, bindings, kid};

pub fn java<'t>(node: Node<'t>, scope: Scope, _: &str) -> Option<Found<'t>> {
    let name = || node.child_by_field_name("name");
    let found = match node.kind() {
        "class_declaration" | "record_declaration" => {
            Found::new(Kind::Class, name()?).inner(Scope::Type)
        }
        "interface_declaration" | "annotation_type_declaration" => {
            Found::new(Kind::Interface, name()?).inner(Scope::Type)
        }
        "enum_declaration" => Found::new(Kind::Enum, name()?).inner(Scope::Enum),
        "enum_constant" if scope == Scope::Enum => Found::new(Kind::Variant, name()?),
        "method_declaration" | "constructor_declaration" => {
            Found::new(Kind::Method, name()?).inner(Scope::Body)
        }
        "variable_declarator" if scope.typed() => Found::new(Kind::Field, name()?),
        _ => return None,
    };
    Some(found)
}

pub fn java_local<'t>(node: Node<'t>, _: &str) -> Option<Local<'t>> {
    let name = || node.child_by_field_name("name");
    let local = match node.kind() {
        "formal_parameter" => Local::new(Kind::Parameter, node, vec![name()?]),
        "spread_parameter" => {
            let name = kid(node, &["variable_declarator"])?.child_by_field_name("name")?;
            Local::new(Kind::Parameter, node, vec![name])
        }
        "inferred_parameters" => Local::new(
            Kind::Parameter,
            node,
            bindings(node, &["identifier"], &[], &[]),
        ),
        "lambda_expression" => {
            let param = node
                .child_by_field_name("parameters")
                .filter(|param| param.kind() == "identifier")?;
            Local::new(Kind::Parameter, param, vec![param])
        }
        "variable_declarator" | "enhanced_for_statement" => {
            let name = name()?;
            Local::new(Kind::Variable, name, vec![name])
        }
        _ => return None,
    };
    Some(local)
}
