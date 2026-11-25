use corvo_domain::SymbolKind as Kind;
use tree_sitter::Node;

use crate::walk::{Found, Local, Scope, bindings, has_token, kid};

pub fn kotlin<'t>(node: Node<'t>, scope: Scope, _: &str) -> Option<Found<'t>> {
    let ty = || kid(node, &["type_identifier"]);
    let id = || kid(node, &["simple_identifier"]);
    let found = match node.kind() {
        "class_declaration" if kid(node, &["enum_class_body"]).is_some() => {
            Found::new(Kind::Enum, ty()?).inner(Scope::Enum)
        }
        "class_declaration" if has_token(node, "interface") => {
            Found::new(Kind::Interface, ty()?).inner(Scope::Type)
        }
        "class_declaration" | "object_declaration" => {
            Found::new(Kind::Class, ty()?).inner(Scope::Type)
        }
        "companion_object" => match ty() {
            Some(name) => Found::new(Kind::Class, name),
            None => Found::new(Kind::Class, node).label("companion object"),
        }
        .inner(Scope::Type),
        "function_declaration" => Found::new(scope.callable(), id()?).inner(Scope::Body),
        "property_declaration" if scope != Scope::Body => {
            let name = kid(
                kid(node, &["variable_declaration"])?,
                &["simple_identifier"],
            )?;
            let kind = if scope.typed() {
                Kind::Field
            } else {
                Kind::Variable
            };
            Found::new(kind, name)
        }
        "class_parameter" if scope.typed() && kid(node, &["binding_pattern_kind"]).is_some() => {
            Found::new(Kind::Field, id()?)
        }
        "enum_entry" if scope == Scope::Enum => Found::new(Kind::Variant, id()?),
        "type_alias" if scope != Scope::Body => Found::new(Kind::Type, ty()?),
        _ => return None,
    };
    Some(found)
}

const DECLARATIONS: &[&str] = &["variable_declaration", "multi_variable_declaration"];
const TYPES: &[&str] = &["user_type", "nullable_type", "function_type"];

pub fn kotlin_local<'t>(node: Node<'t>, _: &str) -> Option<Local<'t>> {
    let names = |pattern| bindings(pattern, &["simple_identifier"], &[], TYPES);
    let local = match node.kind() {
        "parameter" => Local::new(
            Kind::Parameter,
            node,
            vec![kid(node, &["simple_identifier"])?],
        ),
        "lambda_parameters" => Local::new(Kind::Parameter, node, names(node)),
        "property_declaration" | "for_statement" => {
            let pattern = kid(node, DECLARATIONS)?;
            Local::new(Kind::Variable, pattern, names(pattern))
        }
        _ => return None,
    };
    Some(local)
}
