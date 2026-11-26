use std::io;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::Chat;
use crate::port::ChatStore;

// The chats of a workspace, and the one picked, which the agent bar writes to. A chat is kept
// once it has a message; one left empty goes with the app.
pub struct Chats {
    workspace: Uuid,
    store: Box<dyn ChatStore>,
    items: Vec<Chat>,
    active: Option<u64>,
}

impl Chats {
    pub fn load(workspace: Uuid, store: Box<dyn ChatStore>) -> io::Result<Self> {
        let mut items = store.load(workspace)?;
        for chat in &mut items {
            chat.settle();
        }
        let mut chats = Self {
            workspace,
            store,
            items,
            active: None,
        };
        let first = chats.list().find(|chat| !chat.is_archived()).map(Chat::id);
        chats.active = first;
        Ok(chats)
    }

    // No chats, as when the kept ones could not be read.
    pub fn empty(workspace: Uuid, store: Box<dyn ChatStore>) -> Self {
        Self {
            workspace,
            store,
            items: Vec::new(),
            active: None,
        }
    }

    // The newest first.
    pub fn list(&self) -> impl Iterator<Item = &Chat> {
        let mut items: Vec<&Chat> = self.items.iter().collect();
        items.sort_by_key(|chat| std::cmp::Reverse((chat.updated(), chat.id())));
        items.into_iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&Chat> {
        self.items.iter().find(|chat| chat.id() == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Chat> {
        self.items.iter_mut().find(|chat| chat.id() == id)
    }

    pub fn active(&self) -> Option<&Chat> {
        self.active.and_then(|id| self.get(id))
    }

    pub fn active_mut(&mut self) -> Option<&mut Chat> {
        let id = self.active?;
        self.get_mut(id)
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active
    }

    pub fn pick(&mut self, id: u64) {
        if self.get(id).is_some() {
            self.active = Some(id);
        }
    }

    // A new chat, picked; the one left empty before it goes.
    pub fn create(&mut self, agent: &str, project: &Path, now: u64) -> u64 {
        self.items
            .retain(|chat| chat.turns() > 0 || Some(chat.id()) != self.active);
        let id = self.items.iter().map(Chat::id).max().unwrap_or(0) + 1;
        self.items.push(Chat::new(id, agent, project, now));
        self.active = Some(id);
        id
    }

    pub fn rename(&mut self, id: u64, title: &str) -> io::Result<()> {
        if let Some(chat) = self.get_mut(id) {
            chat.rename(title);
        }
        self.save(id)
    }

    // An archived chat leaves the list for its own section; the picked one moves on to the next.
    pub fn archive(&mut self, id: u64, archived: bool) -> io::Result<()> {
        if let Some(chat) = self.get_mut(id) {
            chat.archive(archived);
        }
        if archived && self.active == Some(id) {
            self.move_on(id);
        }
        self.save(id)
    }

    pub fn delete(&mut self, id: u64) -> io::Result<()> {
        self.items.retain(|chat| chat.id() != id);
        if self.active == Some(id) {
            self.move_on(id);
        }
        self.store.remove(self.workspace, id)
    }

    // An image to send in the chat, kept with it so the chat holds it whatever happens to where
    // it came from.
    pub fn keep_image(&self, id: u64, bytes: &[u8], ext: &str) -> io::Result<PathBuf> {
        self.store.keep_image(self.workspace, id, bytes, ext)
    }

    // Writes a chat down, once it has something in it.
    pub fn save(&self, id: u64) -> io::Result<()> {
        match self.get(id) {
            Some(chat) if chat.turns() > 0 => self.store.save(self.workspace, chat),
            _ => Ok(()),
        }
    }

    fn move_on(&mut self, from: u64) {
        let next = self
            .list()
            .find(|chat| chat.id() != from && !chat.is_archived())
            .map(Chat::id);
        self.active = next;
    }
}
