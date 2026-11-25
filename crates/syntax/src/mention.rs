use std::ops::Range;

use tree_sitter::Node;

// The innermost nodes whose text is the name, inside the range, leaving out comments and the
// text of strings (an interpolated name still counts).
pub fn find(root: Node, text: &str, name: &str, within: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    visit(root, text, name, &within, &mut out);
    out
}

fn visit(node: Node, text: &str, name: &str, within: &Range<usize>, out: &mut Vec<Range<usize>>) {
    let range = node.byte_range();
    if range.end <= within.start || range.start >= within.end || quoted(node) {
        return;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    if text[range.clone()] == *name {
        match children
            .iter()
            .find(|child| text[child.byte_range()] == *name)
        {
            Some(child) => visit(*child, text, name, within, out),
            None if range.start >= within.start && range.end <= within.end => out.push(range),
            None => {}
        }
        return;
    }
    for child in children {
        visit(child, text, name, within, out);
    }
}

fn quoted(node: Node) -> bool {
    let kind = node.kind();
    let text = kind.contains("string") || kind.contains("heredoc");
    kind.contains("comment") || (text && node.named_child_count() == 0)
}
