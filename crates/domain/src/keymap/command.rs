#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Save,
    Undo,
    Redo,
    Complete,
    Definition,
    // Brings the code over the home.
    ShowCode,
    // Brings the git window over the home, or puts it away.
    ShowGit,
    // Brings the chat window over the home, or puts it away.
    ShowChat,
    // Puts the code away, back to the home.
    Dismiss,
    // Hides the file tree beside the code, or brings it back.
    ToggleTree,
}
