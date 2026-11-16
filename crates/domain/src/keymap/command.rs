#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Save,
    Undo,
    Redo,
}
