#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Global,
    Editor,
    Tree,
    // The list of workspaces the window opens on.
    Workspaces,
}
