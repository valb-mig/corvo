use std::path::Path;

use crate::Doc;

#[derive(Debug, Default)]
pub struct Tabs {
    docs: Vec<Doc>,
    active: Option<usize>,
}

impl Tabs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn docs(&self) -> &[Doc] {
        &self.docs
    }

    pub fn active(&self) -> Option<&Doc> {
        self.active.and_then(|ix| self.docs.get(ix))
    }

    pub fn active_ix(&self) -> Option<usize> {
        self.active
    }

    pub fn get(&self, path: &Path) -> Option<&Doc> {
        self.find(path).map(|ix| &self.docs[ix])
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.find(path).is_some()
    }

    pub fn open(&mut self, path: &Path) {
        let ix = self.find(path).unwrap_or_else(|| {
            self.docs.push(Doc::new(path));
            self.docs.len() - 1
        });
        self.active = Some(ix);
    }

    pub fn focus(&mut self, path: &Path) {
        if let Some(ix) = self.find(path) {
            self.active = Some(ix);
        }
    }

    pub fn close(&mut self, path: &Path) {
        let Some(ix) = self.find(path) else {
            return;
        };
        self.docs.remove(ix);
        self.active = match self.active {
            _ if self.docs.is_empty() => None,
            Some(active) if active > ix => Some(active - 1),
            Some(active) if active == ix => Some(ix.min(self.docs.len() - 1)),
            active => active,
        };
    }

    pub fn touch(&mut self, path: &Path) {
        if let Some(doc) = self.doc_mut(path) {
            doc.touch();
        }
    }

    pub fn clean(&mut self, path: &Path) {
        if let Some(doc) = self.doc_mut(path) {
            doc.clean();
        }
    }

    fn find(&self, path: &Path) -> Option<usize> {
        self.docs.iter().position(|doc| doc.path() == path)
    }

    fn doc_mut(&mut self, path: &Path) -> Option<&mut Doc> {
        self.docs.iter_mut().find(|doc| doc.path() == path)
    }
}
