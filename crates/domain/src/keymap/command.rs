#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Save,
    Undo,
    Redo,
    Complete,
    Definition,
    // Brings the code over the home.
    ShowCode,
    // Puts the code away, back to the home.
    Dismiss,
}
