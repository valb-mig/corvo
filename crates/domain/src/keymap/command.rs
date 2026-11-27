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
    // Brings the terminal window over the home, or puts it away.
    ShowTerminal,
    // Opens the palette of the window in front, or closes it.
    OpenPalette,
    // Puts the code away, back to the home.
    Dismiss,
    // Opens the panel on the right of the window in front, or closes it: the file tree of the
    // code, the list of chats, the changed files of the git window, the terminals.
    TogglePanel,
    // On the list of workspaces: picks the one above or below, the one on the right shown.
    PickPrevious,
    PickNext,
    // Opens the workspace picked on the list.
    OpenPicked,
    // Asks whether to remove the workspace picked on the list, or to send the file or folder
    // picked in the file tree to the trash.
    RemovePicked,
    // Turns the name of the workspace picked on the list, or of the file or folder picked in the
    // file tree, into a field.
    RenamePicked,
    // Puts the cursor in the field that adds a workspace.
    NewWorkspace,
    // Brings the code over the home with its search through the project's files.
    FindInFiles,
    // In the git window: shows one of its screens.
    GitChanges,
    GitHistory,
    GitBranches,
    GitStash,
    GitWorktrees,
    GitConflicts,
    // In the git window: shows the repository above or below in the workspace's list.
    GitPreviousRepo,
    GitNextRepo,
}
