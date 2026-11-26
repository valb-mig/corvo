use std::io;
use std::path::PathBuf;

use uuid::Uuid;

use crate::Chat;

// Where the chats of each workspace are kept, one by one.
pub trait ChatStore {
    fn load(&self, workspace: Uuid) -> io::Result<Vec<Chat>>;
    fn save(&self, workspace: Uuid, chat: &Chat) -> io::Result<()>;
    // Goes with the images kept for it.
    fn remove(&self, workspace: Uuid, id: u64) -> io::Result<()>;
    // An image the dev sent in the chat, kept beside it under a name of its own; where it is.
    fn keep_image(&self, workspace: Uuid, id: u64, bytes: &[u8], ext: &str) -> io::Result<PathBuf>;
}
