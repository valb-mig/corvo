use std::io;

use uuid::Uuid;

use crate::Chat;

// Where the chats of each workspace are kept, one by one.
pub trait ChatStore {
    fn load(&self, workspace: Uuid) -> io::Result<Vec<Chat>>;
    fn save(&self, workspace: Uuid, chat: &Chat) -> io::Result<()>;
    fn remove(&self, workspace: Uuid, id: u64) -> io::Result<()>;
}
