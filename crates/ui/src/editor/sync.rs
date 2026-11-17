use gpui_kit::component::input::{InputEdit, Rope, RopeExt as _};
use lsp_types::{Range, TextDocumentContentChangeEvent};

pub fn changes(prev: &Rope, edits: &[(InputEdit, Rope)]) -> Vec<TextDocumentContentChangeEvent> {
    let mut before = prev.clone();
    edits
        .iter()
        .map(|(edit, after)| {
            let start = before.offset_to_position(edit.start_byte);
            let end = before.offset_to_position(edit.old_end_byte);
            let text = after.slice(edit.start_byte..edit.new_end_byte).to_string();
            before = after.clone();
            TextDocumentContentChangeEvent {
                range: Some(Range::new(start, end)),
                range_length: None,
                text,
            }
        })
        .collect()
}
