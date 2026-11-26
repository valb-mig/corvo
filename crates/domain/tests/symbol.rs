use corvo_domain::{Symbol, SymbolKind as Kind, gather_impls, path_to};

fn sym(name: &str, kind: Kind, first: u32, last: u32) -> Symbol {
    Symbol::new(name, kind, first, 0).span(first, last)
}

fn names(symbols: &[Symbol]) -> Vec<&str> {
    symbols.iter().map(Symbol::name).collect()
}

// struct Spec { id, name }   impl Spec { new, id }   fn specs()   impl Display for Spec { fmt }
fn outline() -> Vec<Symbol> {
    vec![
        sym("Spec", Kind::Struct, 0, 3).children(vec![
            sym("id", Kind::Field, 1, 1),
            sym("name", Kind::Field, 2, 2),
        ]),
        sym("Spec", Kind::Impl, 5, 12).children(vec![
            sym("new", Kind::Method, 6, 8),
            sym("id", Kind::Method, 10, 11),
        ]),
        sym("specs", Kind::Function, 14, 16),
        sym("Display for Spec", Kind::Impl, 18, 22).children(vec![sym(
            "fmt",
            Kind::Method,
            19,
            21,
        )]),
    ]
}

#[test]
fn the_methods_of_a_types_own_impl_go_into_the_type_after_its_fields() {
    let symbols = gather_impls(outline());

    assert_eq!(names(&symbols), ["Spec", "specs"]);
    assert_eq!(
        names(symbols[0].nested()),
        ["id", "name", "new", "id", "Display"]
    );
}

#[test]
fn an_impl_of_a_trait_goes_into_the_type_named_for_the_trait_with_its_methods() {
    let symbols = gather_impls(outline());
    let display = &symbols[0].nested()[4];

    assert_eq!(display.kind(), Kind::Impl);
    assert_eq!(display.name(), "Display");
    assert_eq!(names(display.nested()), ["fmt"]);
}

#[test]
fn generics_and_paths_do_not_keep_an_impl_from_its_type() {
    let symbols = gather_impls(vec![
        sym("Pile", Kind::Struct, 0, 2),
        sym("Pile<T>", Kind::Impl, 4, 8).children(vec![sym("len", Kind::Method, 5, 7)]),
        sym("From<u8> for crate::pile::Pile<T>", Kind::Impl, 10, 14),
    ]);

    assert_eq!(names(&symbols), ["Pile"]);
    assert_eq!(names(symbols[0].nested()), ["len", "From<u8>"]);
}

#[test]
fn an_impl_whose_type_is_elsewhere_stays_where_it_is() {
    let symbols = gather_impls(vec![
        sym("Spec", Kind::Struct, 0, 2),
        sym("Other", Kind::Impl, 4, 8).children(vec![sym("go", Kind::Method, 5, 7)]),
    ]);

    assert_eq!(names(&symbols), ["Spec", "Other"]);
    assert_eq!(names(symbols[1].nested()), ["go"]);
}

#[test]
fn impls_inside_a_module_go_into_the_types_of_that_module() {
    let symbols = gather_impls(vec![sym("tests", Kind::Module, 0, 20).children(outline())]);

    assert_eq!(names(symbols[0].nested()), ["Spec", "specs"]);
}

#[test]
fn the_path_to_a_line_goes_down_to_the_innermost_symbol_holding_it() {
    let symbols = gather_impls(outline());

    // Inside new, which came from the impl into the type.
    assert_eq!(path_to(&symbols, 7), [0, 2]);
    // On a field.
    assert_eq!(path_to(&symbols, 2), [0, 1]);
    // Inside fmt, under the impl of Display.
    assert_eq!(path_to(&symbols, 20), [0, 4, 0]);
    // On the type's own line, nothing deeper.
    assert_eq!(path_to(&symbols, 0), [0]);
    assert_eq!(path_to(&symbols, 15), [1]);
}

#[test]
fn a_line_between_symbols_has_no_path() {
    let symbols = gather_impls(outline());

    assert!(path_to(&symbols, 4).is_empty());
    assert!(path_to(&symbols, 30).is_empty());
}
