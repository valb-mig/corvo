use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::{Doc, Workbench, ago};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::{App, Entity, SharedString, Window};

use crate::agents::{Agents, now};
use crate::pane::chats::ChatWindow;
use crate::pane::code::Code;
use crate::pane::git::GitWindow;
use crate::repos::Repos;

// How many files of a project the code's palette lists at most, of those that match.
const SHOWN: usize = 200;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

// One thing a window's palette offers: what it shows, and what picking it does.
pub struct Offer {
    pub glyph: Glyph,
    pub label: SharedString,
    pub detail: Option<SharedString>,
    pub run: Run,
}

impl Offer {
    fn new(
        glyph: Glyph,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            glyph,
            label: label.into(),
            detail: None,
            run: Rc::new(run),
        }
    }

    fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

// Whether every word of the query is in the text, in any case.
pub fn finds(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

// The chat window's: a new chat, then the chats, newest first, found by title and messages.
pub fn chats(
    window: &Entity<ChatWindow>,
    agents: &Entity<Agents>,
    query: &str,
    cx: &App,
) -> Vec<Offer> {
    let mut offers = Vec::new();
    if finds(query, "New chat") {
        let fresh = window.clone();
        offers.push(Offer::new(Glyph::Plus, "New chat", move |_, cx| {
            fresh.update(cx, |window, cx| window.new_chat(cx));
        }));
    }
    let agents = agents.read(cx);
    let now = now();
    for chat in agents.chats().list().filter(|chat| chat.matches(query)) {
        let id = chat.id();
        let detail = chat.snippet(query).map(|(text, _)| text).unwrap_or_else(|| {
            let when = ago(chat.updated(), now);
            let name = agents.agent_name(chat.agent());
            if chat.is_archived() {
                format!("{name} · {when} · archived")
            } else {
                format!("{name} · {when}")
            }
        });
        let pick = window.clone();
        let offer = Offer::new(Glyph::MessageSquare, chat.title().to_owned(), move |_, cx| {
            pick.update(cx, |window, cx| window.pick(id, cx));
        });
        offers.push(offer.detail(detail));
    }
    offers
}

// The code's: the open files, then the files of the project in front, found by their path in it.
pub fn code(
    code: &Entity<Code>,
    bench: &Entity<Workbench>,
    files: &[PathBuf],
    query: &str,
    cx: &App,
) -> Vec<Offer> {
    let bench = bench.read(cx);
    let root = bench.project();
    let open: Vec<&Path> = bench.tabs().docs().iter().map(Doc::path).collect();
    let offer = |path: &Path, open: bool| {
        let shown = inside(root, path);
        if !finds(query, &shown) {
            return None;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = Path::new(&shown)
            .parent()
            .map(|dir| dir.display().to_string())
            .unwrap_or_default();
        let detail = match (open, dir.is_empty()) {
            (true, true) => "open".to_owned(),
            (true, false) => format!("{dir} · open"),
            (false, _) => dir,
        };
        let (code, path) = (code.clone(), path.to_path_buf());
        let offer = Offer::new(Glyph::FileText, name, move |window, cx| {
            code.update(cx, |code, cx| code.open(&path, window, cx));
        });
        Some(offer.detail(detail))
    };
    let tabs = open.iter().filter_map(|path| offer(path, true));
    let rest = files
        .iter()
        .filter(|path| !open.contains(&path.as_path()))
        .filter_map(|path| offer(path, false))
        .take(SHOWN);
    tabs.chain(rest).collect()
}

// The git window's: the changed files of each repository, found by their path in it.
pub fn git(git: &Entity<GitWindow>, repos: &Entity<Repos>, query: &str, cx: &App) -> Vec<Offer> {
    let mut offers = Vec::new();
    for repo in repos.read(cx).repos() {
        for changed in repo.changes() {
            let shown = inside(Some(repo.root()), &changed.path);
            if !finds(query, &shown) {
                continue;
            }
            let name = changed
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let detail = format!("{} · {shown}", changed.change.letter());
            let (git, path) = (git.clone(), changed.path.clone());
            let offer = Offer::new(Glyph::GitBranch, name, move |_, cx| {
                git.update(cx, |git, cx| git.pick(path.clone(), cx));
            });
            offers.push(offer.detail(detail));
        }
    }
    offers
}

// A path as it reads inside its project or repository.
fn inside(root: Option<&Path>, path: &Path) -> String {
    root.and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::finds;

    #[test]
    fn finds_every_word_in_any_case() {
        assert!(finds("", "anything"));
        assert!(finds("pane code", "crates/corvo/src/pane/Code.rs"));
        assert!(!finds("pane chat", "crates/corvo/src/pane/code.rs"));
    }
}
