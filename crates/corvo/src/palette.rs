use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::{Doc, Script, Workbench, ago};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::{App, Entity, SharedString, Window};

use crate::agents::{Agents, now};
use crate::pane::chats::ChatWindow;
use crate::pane::code::Code;
use crate::pane::overview::Overview;
use crate::pane::terminals::Terminals;

// How many files of a project the code's palette lists at most, of those that match.
const SHOWN: usize = 200;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

// One thing a window's palette offers: what it shows, what picking it does, and what picking it
// with Ctrl held does, when that is something else.
pub struct Offer {
    pub glyph: Glyph,
    pub label: SharedString,
    pub detail: Option<SharedString>,
    pub run: Run,
    pub alt: Option<Run>,
}

impl Offer {
    pub(crate) fn new(
        glyph: Glyph,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            glyph,
            label: label.into(),
            detail: None,
            run: Rc::new(run),
            alt: None,
        }
    }

    pub(crate) fn alt(mut self, alt: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.alt = Some(Rc::new(alt));
        self
    }

    pub(crate) fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

// Whether every word of the query is in the text, in any case.
pub fn finds(query: &str, text: &str) -> bool {
    has(&words(query), &text.to_lowercase())
}

// The words of a query, lowercased once for all the texts they are looked for in.
fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

// Whether every word is in the text, already lowercased.
fn has(words: &[String], lower: &str) -> bool {
    words.iter().all(|word| lower.contains(word.as_str()))
}

// A file of a project for the code's palette, with its path inside the project as the palette
// shows it and as it searches it; made once, when the files of the project are walked.
pub struct Listed {
    path: PathBuf,
    shown: String,
    lower: String,
}

impl Listed {
    pub fn new(root: &Path, path: PathBuf) -> Self {
        let shown = inside(Some(root), &path);
        let lower = shown.to_lowercase();
        Self { path, shown, lower }
    }
}

// The files of a project a query finds, by where they are in the list, leaving out the open
// ones. A project can have thousands, so what the last query found is kept while the query, the
// files and the open tabs stay the same: drawing the palette again does not search again.
#[derive(Default)]
pub struct Search {
    last: RefCell<Option<Found>>,
}

struct Found {
    query: String,
    files: Rc<Vec<Listed>>,
    open: Vec<PathBuf>,
    rows: Rc<[usize]>,
}

impl Search {
    pub fn found(&self, files: &Rc<Vec<Listed>>, query: &str, open: &[&Path]) -> Rc<[usize]> {
        let mut last = self.last.borrow_mut();
        if let Some(found) = last.as_ref()
            && found.query == query
            && Rc::ptr_eq(&found.files, files)
            && found
                .open
                .iter()
                .map(PathBuf::as_path)
                .eq(open.iter().copied())
        {
            return found.rows.clone();
        }
        let words = words(query);
        let rows: Rc<[usize]> = files
            .iter()
            .enumerate()
            .filter(|(_, file)| !open.contains(&file.path.as_path()) && has(&words, &file.lower))
            .map(|(row, _)| row)
            .take(SHOWN)
            .collect();
        *last = Some(Found {
            query: query.to_owned(),
            files: files.clone(),
            open: open.iter().map(|path| path.to_path_buf()).collect(),
            rows: rows.clone(),
        });
        rows
    }
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
        let detail = chat
            .snippet(query)
            .map(|(text, _)| text)
            .unwrap_or_else(|| {
                let when = ago(chat.updated(), now);
                let name = agents.agent_name(chat.agent());
                if chat.is_archived() {
                    format!("{name} · {when} · archived")
                } else {
                    format!("{name} · {when}")
                }
            });
        let pick = window.clone();
        let offer = Offer::new(
            Glyph::MessageSquare,
            chat.title().to_owned(),
            move |_, cx| {
                pick.update(cx, |window, cx| window.pick(id, cx));
            },
        );
        offers.push(offer.detail(detail));
    }
    offers
}

// The code's: the open files, then the files of the project in front, found by their path in it.
pub fn code(
    code: &Entity<Code>,
    bench: &Entity<Workbench>,
    files: &Rc<Vec<Listed>>,
    search: &Search,
    query: &str,
    cx: &App,
) -> Vec<Offer> {
    let bench = bench.read(cx);
    let root = bench.project();
    let open: Vec<&Path> = bench.tabs().docs().iter().map(Doc::path).collect();
    let offer = |path: &Path, shown: &str, open: bool| {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = Path::new(shown)
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
        offer.detail(detail)
    };
    let tabs = open.iter().filter_map(|path| {
        let shown = inside(root, path);
        finds(query, &shown).then(|| offer(path, &shown, true))
    });
    let rows = search.found(files, query, &open);
    let rest = rows.iter().map(|row| {
        let file = &files[*row];
        offer(&file.path, &file.shown, false)
    });
    tabs.chain(rest).collect()
}

// The home's: the files of every project linked, by their path from the folder the projects are
// in, so each starts with its project's name. Picking one shows it on the home; with Ctrl, it
// opens in the code.
pub fn home(
    home: &Entity<Overview>,
    code: &Entity<Code>,
    files: &Rc<Vec<Listed>>,
    search: &Search,
    query: &str,
) -> Vec<Offer> {
    search
        .found(files, query, &[])
        .iter()
        .map(|row| {
            let file = &files[*row];
            let name = file
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dir = Path::new(&file.shown)
                .parent()
                .map(|dir| dir.display().to_string())
                .unwrap_or_default();
            let (home, code) = (home.clone(), code.clone());
            let (shown, opened) = (file.path.clone(), file.path.clone());
            Offer::new(Glyph::FileText, name, move |window, cx| {
                home.update(cx, |home, cx| home.reveal(&shown, window, cx));
            })
            .alt(move |window, cx| {
                code.update(cx, |code, cx| code.open(&opened, window, cx));
            })
            .detail(dir)
        })
        .collect()
}

// The terminal window's: a new terminal, the terminals by name, then the scripts of the project
// in front, each run in a terminal of its own.
pub fn terminals(
    terminals: &Entity<Terminals>,
    scripts: &[Script],
    root: Option<&Path>,
    query: &str,
    cx: &App,
) -> Vec<Offer> {
    let mut offers = Vec::new();
    if finds(query, "New terminal") {
        let fresh = terminals.clone();
        offers.push(Offer::new(
            Glyph::Plus,
            "New terminal",
            move |window, cx| {
                fresh.update(cx, |terminals, cx| terminals.new_terminal(window, cx));
            },
        ));
    }
    for (id, name, detail) in terminals.read(cx).list() {
        if !finds(query, &format!("{name} {detail}")) {
            continue;
        }
        let pick = terminals.clone();
        let offer = Offer::new(Glyph::Terminal, name, move |window, cx| {
            pick.update(cx, |terminals, cx| terminals.show(id, window, cx));
        });
        offers.push(offer.detail(detail));
    }
    let Some(root) = root else {
        return offers;
    };
    for script in scripts {
        let label = format!("Run {}", script.name());
        if !finds(query, &format!("{label} {}", script.command())) {
            continue;
        }
        let (run, script, root) = (terminals.clone(), script.clone(), root.to_path_buf());
        let detail = format!("{} · {}", script.source(), script.command().trim_end());
        let offer = Offer::new(Glyph::Play, label, move |window, cx| {
            run.update(cx, |terminals, cx| {
                terminals.run(&script, &root, window, cx)
            });
        });
        offers.push(offer.detail(detail));
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
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use super::{Listed, Search, finds};

    fn project(paths: &[&str]) -> Rc<Vec<Listed>> {
        let root = Path::new("/p");
        Rc::new(
            paths
                .iter()
                .map(|path| Listed::new(root, root.join(path)))
                .collect(),
        )
    }

    #[test]
    fn finds_every_word_in_any_case() {
        assert!(finds("", "anything"));
        assert!(finds("pane code", "crates/corvo/src/pane/Code.rs"));
        assert!(!finds("pane chat", "crates/corvo/src/pane/code.rs"));
    }

    #[test]
    fn a_search_finds_files_by_their_path_in_the_project_leaving_out_the_open_ones() {
        let files = project(&["src/Main.rs", "src/pane/code.rs", "docs/code.md"]);
        let open = PathBuf::from("/p/docs/code.md");

        let rows = Search::default().found(&files, "CODE", &[open.as_path()]);

        assert_eq!(&*rows, [1]);
        // The project's own folder is not part of what is searched.
        assert!(Search::default().found(&files, "p/src", &[]).is_empty());
    }

    #[test]
    fn a_search_runs_again_only_when_the_query_files_or_tabs_change() {
        let files = project(&["src/main.rs", "src/lib.rs"]);
        let search = Search::default();

        let first = search.found(&files, "src", &[]);
        assert!(Rc::ptr_eq(&first, &search.found(&files, "src", &[])));

        let narrower = search.found(&files, "src lib", &[]);
        assert_eq!(&*narrower, [1]);
        let lib = PathBuf::from("/p/src/lib.rs");
        assert!(search.found(&files, "src lib", &[lib.as_path()]).is_empty());
        let walked_again = project(&["src/main.rs", "src/lib.rs"]);
        assert!(!Rc::ptr_eq(
            &first,
            &search.found(&walked_again, "src", &[])
        ));
    }
}
