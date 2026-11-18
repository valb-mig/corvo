use std::io;

use crate::Workspace;

pub trait Store {
    fn load(&self) -> io::Result<Vec<Workspace>>;
    fn save(&self, items: &[Workspace]) -> io::Result<()>;
}
