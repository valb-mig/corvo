use std::io;

use uuid::Uuid;

use crate::Workspace;

// What is kept of the workspaces: every one of them, and the one picked last on their list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    pub items: Vec<Workspace>,
    pub last: Option<Uuid>,
}

pub trait Store {
    fn load(&self) -> io::Result<Saved>;
    fn save(&self, items: &[Workspace], last: Option<Uuid>) -> io::Result<()>;
}
