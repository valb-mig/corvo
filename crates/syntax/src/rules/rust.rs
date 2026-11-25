use corvo_domain::SymbolKind as Kind;
use tree_sitter::Node;

use crate::walk::{Found, Local, Scope, bindings};

pub fn rust<'t>(node: Node<'t>, scope: Scope, text: &str) -> Option<Found<'t>> {
    let name = || node.child_by_field_name("name");
    let item = scope != Scope::Body;
    let found = match node.kind() {
        "mod_item" => Found::new(Kind::Module, name()?).inner(Scope::Top),
        "struct_item" | "union_item" => Found::new(Kind::Struct, name()?).inner(Scope::Type),
        "enum_item" => Found::new(Kind::Enum, name()?).inner(Scope::Enum),
        "trait_item" => Found::new(Kind::Interface, name()?).inner(Scope::Type),
        "impl_item" => {
            let ty = node.child_by_field_name("type")?;
            let label = match node.child_by_field_name("trait") {
                Some(tr) => format!("{} for {}", &text[tr.byte_range()], &text[ty.byte_range()]),
                None => text[ty.byte_range()].to_owned(),
            };
            Found::new(Kind::Impl, ty).label(label).inner(Scope::Type)
        }
        "function_item" => Found::new(scope.callable(), name()?).inner(Scope::Body),
        "function_signature_item" => Found::new(scope.callable(), name()?),
        "macro_definition" if item => Found::new(Kind::Function, name()?),
        "const_item" | "static_item" if item => Found::new(Kind::Constant, name()?),
        "type_item" if item => Found::new(Kind::Type, name()?),
        "field_declaration" if scope.typed() => Found::new(Kind::Field, name()?),
        "enum_variant" if scope == Scope::Enum => Found::new(Kind::Variant, name()?),
        _ => return None,
    };
    Some(found)
}

const NAMES: &[&str] = &["identifier", "shorthand_field_identifier"];
const PATHS: &[&str] = &["scoped_identifier", "scoped_type_identifier"];

pub fn rust_local<'t>(node: Node<'t>, _: &str) -> Option<Local<'t>> {
    let field = |name| node.child_by_field_name(name);
    let names = |pattern| bindings(pattern, NAMES, &["type"], PATHS);
    let local = match node.kind() {
        "parameter" => Local::new(Kind::Parameter, node, names(field("pattern")?)),
        "closure_parameters" => Local::new(Kind::Parameter, node, names(node)),
        "let_declaration" | "for_expression" | "let_condition" => {
            let pattern = field("pattern")?;
            Local::new(Kind::Variable, pattern, names(pattern))
        }
        "const_item" | "static_item" => {
            let name = field("name")?;
            Local::new(Kind::Constant, name, vec![name])
        }
        _ => return None,
    };
    Some(local)
}
