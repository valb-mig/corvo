use std::ops::Range;

use crate::{Lang, Symbol};

// How deep an outline reads: the items of the file, or also the parameters and local variables
// inside each function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Items,
    Locals,
}

pub trait Syntax {
    fn outlines(&self, lang: Lang) -> bool;
    fn outline(&self, lang: Lang, text: &str, depth: Depth) -> Vec<Symbol>;
    // Byte ranges where the name comes as code inside `within`, not in strings or comments.
    fn mentions(
        &self,
        lang: Lang,
        text: &str,
        name: &str,
        within: Range<usize>,
    ) -> Vec<Range<usize>>;
}
