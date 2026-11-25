use std::io;

use crate::Chat;

// Where the chats of each workspace are kept, one by one.
pub trait ChatStore {
    fn load(&self, workspace: u64) -> io::Result<Vec<Chat>>;
    fn save(&self, workspace: u64, chat: &Chat) -> io::Result<()>;
    fn remove(&self, workspace: u64, id: u64) -> io::Result<()>;
}
