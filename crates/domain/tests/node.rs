use corvo_domain::Node;

#[test]
fn sorts_dirs_first_then_by_name() {
    let mut nodes = [
        Node::file("/p/b.rs"),
        Node::dir("/p/zeta"),
        Node::file("/p/A.rs"),
        Node::dir("/p/alpha"),
    ];
    nodes.sort();

    let names: Vec<_> = nodes.iter().map(Node::name).collect();
    assert_eq!(names, ["alpha", "zeta", "A.rs", "b.rs"]);
}
