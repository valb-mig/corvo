mod branches;
mod changes;
mod compare;
mod conflicts;
mod history;
mod parts;
mod stash;
mod worktrees;

use std::path::{Path, PathBuf};

use corvo_domain::{Op, Repo};
use corvo_ui::notice;
use corvo_ui::rail::{Rail, Stop};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _, h_flex,
};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, WeakEntity, Window, div,
    prelude::FluentBuilder as _,
};

use crate::agents::Agents;
use crate::palette::Offer;
use crate::repos::Repos;

use branches::Branches;
use changes::Changes;
use conflicts::Conflicts;
use history::History;
use stash::Stashes;
use worktrees::Worktrees;

pub enum Event {
    // Asked to open a file in the code.
    Open(PathBuf),
    // Asked to send a chat a message, starting one in this folder when none is open there.
    Ask { project: PathBuf, prompt: String },
    // Asked to open a chat by its id.
    Chat(u64),
    // Asked to open a terminal in a folder.
    Terminal(PathBuf),
}

// The screens of the git window, one per icon down its left side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Changes,
    History,
    Branches,
    Stash,
    Worktrees,
    // Only while a merge, rebase, cherry-pick or revert waits on the dev.
    Conflicts,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Changes,
        Tab::History,
        Tab::Branches,
        Tab::Stash,
        Tab::Worktrees,
        Tab::Conflicts,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Changes => "Changes",
            Tab::History => "History",
            Tab::Branches => "Branches",
            Tab::Stash => "Stash",
            Tab::Worktrees => "Worktrees",
            Tab::Conflicts => "Conflicts",
        }
    }

    fn glyph(self) -> Glyph {
        match self {
            Tab::Changes => Glyph::FileDiff,
            Tab::History => Glyph::RotateCcwClock,
            Tab::Branches => Glyph::GitBranch,
            Tab::Stash => Glyph::Archive,
            Tab::Worktrees => Glyph::FolderGit2,
            Tab::Conflicts => Glyph::GitMergeConflict,
        }
    }

    // What the window's palette finds on this screen.
    pub fn ask(self) -> &'static str {
        match self {
            Tab::Changes => "Find a changed file",
            Tab::History => "Find a commit: message, hash, author or path",
            Tab::Branches => "Find a branch or tag",
            Tab::Stash => "Find a stash",
            Tab::Worktrees => "Find a worktree",
            Tab::Conflicts => "Find a conflicted file",
        }
    }
}

// What the screens of the window need of it: the repositories, the one shown, and a way to ask
// git to do something.
#[derive(Clone)]
pub struct Ctx {
    pub repos: Entity<Repos>,
    pub window: WeakEntity<GitWindow>,
}

impl Ctx {
    pub fn act(&self, op: Op, window: &mut Window, cx: &mut App) {
        self.window
            .update(cx, |git, cx| git.act(op, window, cx))
            .ok();
    }

    pub fn emit(&self, event: Event, cx: &mut App) {
        self.window.update(cx, |_, cx| cx.emit(event)).ok();
    }

    // The repository shown, as git last read it.
    pub fn repo<'a>(&self, root: &Path, cx: &'a App) -> Option<&'a Repo> {
        self.repos
            .read(cx)
            .repos()
            .iter()
            .find(|repo| repo.root() == root)
    }
}

// The git window: icons down its left for its screens (the changes, the history, the branches,
// the stash, the worktrees, and the conflicts while something waits on them), the repository
// shown picked beside its name, and fetch, pull and push on the right of its bar. It takes turns
// with the code and the chats over the home.
pub struct GitWindow {
    ctx: Ctx,
    tab: Tab,
    // The repository shown; the first one until another is picked.
    root: Option<PathBuf>,
    changes: Entity<Changes>,
    history: Entity<History>,
    branches: Entity<Branches>,
    stashes: Entity<Stashes>,
    worktrees: Entity<Worktrees>,
    conflicts: Entity<Conflicts>,
    // A fetch, pull or push under way.
    busy: Option<&'static str>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for GitWindow {}

impl GitWindow {
    pub fn new(repos: Entity<Repos>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ctx = Ctx {
            repos: repos.clone(),
            window: cx.entity().downgrade(),
        };
        let changes = cx.new(|cx| Changes::new(ctx.clone(), window, cx));
        let history = cx.new(|cx| History::new(ctx.clone(), cx));
        let branches = cx.new(|cx| Branches::new(ctx.clone(), cx));
        let stashes = cx.new(|cx| Stashes::new(ctx.clone(), window, cx));
        let worktrees = cx.new(|cx| Worktrees::new(ctx.clone(), cx));
        let conflicts = cx.new(|cx| Conflicts::new(ctx.clone(), cx));
        let subs = vec![cx.observe(&repos, |this, _, cx| this.repos_moved(cx))];
        Self {
            ctx,
            tab: Tab::Changes,
            root: None,
            changes,
            history,
            branches,
            stashes,
            worktrees,
            conflicts,
            busy: None,
            _subs: subs,
        }
    }

    pub fn set_agents(&mut self, agents: Entity<Agents>, cx: &mut Context<Self>) {
        self.worktrees
            .update(cx, |worktrees, cx| worktrees.set_agents(agents, cx));
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    fn repo<'a>(&self, cx: &'a App) -> Option<&'a Repo> {
        self.ctx.repo(self.root.as_deref()?, cx)
    }

    // Whether the conflicts screen has something to show.
    fn conflicted(&self, cx: &App) -> bool {
        self.repo(cx)
            .is_some_and(|repo| repo.doing().is_some() || repo.conflicts().next().is_some())
    }

    // Read again when the window comes over the home, and every few seconds while it is there:
    // the files may have changed meanwhile.
    pub fn reread(&mut self, cx: &mut Context<Self>) {
        self.settle(cx);
        self.load(self.tab, cx);
    }

    // The repositories moved: the one shown may be gone, and what is half done may have ended.
    fn repos_moved(&mut self, cx: &mut Context<Self>) {
        let before = self.root.clone();
        self.settle(cx);
        if self.root != before {
            self.load(self.tab, cx);
        } else if self.tab == Tab::Changes || self.tab == Tab::Conflicts {
            self.load(self.tab, cx);
        }
        cx.notify();
    }

    // Keeps the repository shown one that is there, and the screen one there is.
    fn settle(&mut self, cx: &mut Context<Self>) {
        let repos = self.ctx.repos.read(cx).repos();
        let still = self
            .root
            .as_deref()
            .is_some_and(|root| repos.iter().any(|repo| repo.root() == root));
        if !still {
            self.root = repos.first().map(|repo| repo.root().to_path_buf());
        }
        if self.tab == Tab::Conflicts && !self.conflicted(cx) {
            self.tab = Tab::Changes;
        }
    }

    fn load(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let root = self.root.clone();
        match tab {
            Tab::Changes => self.changes.update(cx, |view, cx| view.show(root, cx)),
            Tab::History => self.history.update(cx, |view, cx| view.show(root, cx)),
            Tab::Branches => self.branches.update(cx, |view, cx| view.show(root, cx)),
            Tab::Stash => self.stashes.update(cx, |view, cx| view.show(root, cx)),
            Tab::Worktrees => self.worktrees.update(cx, |view, cx| view.show(root, cx)),
            Tab::Conflicts => self.conflicts.update(cx, |view, cx| view.show(root, cx)),
        }
    }

    pub fn show_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if tab == Tab::Conflicts && !self.conflicted(cx) {
            return;
        }
        self.tab = tab;
        self.load(tab, cx);
        cx.notify();
    }

    pub fn pick_repo(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.root.as_ref() == Some(&root) {
            return;
        }
        self.root = Some(root);
        self.settle(cx);
        self.load(self.tab, cx);
        cx.notify();
    }

    // The repository above or below the one shown, round the list.
    pub fn step_repo(&mut self, down: bool, cx: &mut Context<Self>) {
        let repos = self.ctx.repos.read(cx).repos();
        if repos.len() < 2 {
            return;
        }
        let at = self
            .root
            .as_deref()
            .and_then(|root| repos.iter().position(|repo| repo.root() == root))
            .unwrap_or(0);
        let next = if down {
            (at + 1) % repos.len()
        } else {
            (at + repos.len() - 1) % repos.len()
        };
        let root = repos[next].root().to_path_buf();
        self.pick_repo(root, cx);
    }

    // Only the changes have a panel, the list of files floating on the right.
    pub fn has_panel(&self, cx: &App) -> bool {
        self.tab == Tab::Changes && self.changes.read(cx).has_panel()
    }

    // Opens the list of changed files, or closes it; tells whether it is open now.
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tab != Tab::Changes {
            self.show_tab(Tab::Changes, cx);
            return true;
        }
        self.changes
            .update(cx, |changes, cx| changes.toggle_panel(cx))
    }

    // Shows a changed file's patch.
    pub fn pick(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(root) = self.ctx.repos.read(cx).repo_of(&path) {
            let root = root.root().to_path_buf();
            if self.root.as_ref() != Some(&root) {
                self.root = Some(root);
            }
        }
        self.tab = Tab::Changes;
        let root = self.root.clone();
        self.changes.update(cx, |changes, cx| {
            changes.show(root, cx);
            changes.pick(path, cx);
        });
        cx.notify();
    }

    // Asks git to do something to the repository shown; what can lose work asks the dev first.
    // A fetch, pull or push says so on the bar while it runs.
    pub fn act(&mut self, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        match ask(&op) {
            Some((title, what, ok)) => {
                let view = cx.entity().downgrade();
                window.open_alert_dialog(cx, move |alert, _, _| {
                    let (view, op, root) = (view.clone(), op.clone(), root.clone());
                    alert
                        .title(title.clone())
                        .description(what.clone())
                        .confirm()
                        .ok_text(ok.clone())
                        .ok_variant(ButtonVariant::Danger)
                        .on_ok(move |_, window, cx| {
                            let (op, root) = (op.clone(), root.clone());
                            view.update(cx, |git, cx| git.run(root, op, window, cx))
                                .ok();
                            true
                        })
                });
            }
            None => self.run(root, op, window, cx),
        }
    }

    fn run(&mut self, root: PathBuf, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        if op.is_remote() {
            if self.busy.is_some() {
                return;
            }
            self.busy = Some(match op {
                Op::Fetch => "Fetching",
                Op::Pull => "Pulling",
                _ => "Pushing",
            });
            cx.notify();
        }
        let git = self.ctx.repos.read(cx).git();
        let (ran, remote) = (op.clone(), op.is_remote());
        let task = cx
            .background_executor()
            .spawn(async move { git.run(&root, &ran) });
        cx.spawn_in(window, async move |this, cx| {
            let done = task.await;
            this.update_in(cx, |this, window, cx| {
                if remote {
                    this.busy = None;
                }
                this.ctx.repos.update(cx, |repos, cx| repos.refresh(cx));
                match done {
                    Ok(()) => {
                        if let Some(said) = said(&op) {
                            notice::info(said, window, cx);
                        }
                    }
                    // A merge or rebase that stops on conflicts goes to them.
                    Err(why) if why.to_string().contains("CONFLICT") => {
                        notice::warn("Stopped on conflicts.", window, cx);
                        this.tab = Tab::Conflicts;
                    }
                    Err(why) => notice::warn(why.to_string(), window, cx),
                }
                this.after(&op, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // What a screen keeps once git did what it was asked.
    fn after(&mut self, op: &Op, cx: &mut Context<Self>) {
        if matches!(op, Op::Commit { .. }) {
            self.changes.update(cx, |changes, cx| changes.committed(cx));
        }
        self.load(self.tab, cx);
    }

    // The repository picker, after the window's name.
    pub fn lead(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = cx.theme();
        let repo = self.repo(cx)?;
        let view = cx.entity().downgrade();
        let roots: Vec<(PathBuf, String, String, usize, bool)> = self
            .ctx
            .repos
            .read(cx)
            .repos()
            .iter()
            .map(|r| {
                (
                    r.root().to_path_buf(),
                    r.name(),
                    r.branch().to_owned(),
                    r.changes().len(),
                    Some(r.root()) == self.root.as_deref(),
                )
            })
            .collect();
        let several = roots.len() > 1;
        let label = h_flex()
            .gap_1p5()
            .child(Icon::new(Glyph::Folder).xsmall())
            .child(repo.name())
            .child(
                h_flex()
                    .gap_1()
                    .text_color(theme.muted_foreground)
                    .font_family("monospace")
                    .text_xs()
                    .child(Icon::new(Glyph::GitBranch).xsmall())
                    .child(repo.branch().to_owned()),
            );
        let button = Button::new("git-repo")
            .outline()
            .small()
            .child(label)
            .when(several, |b| b.dropdown_caret(true))
            .tooltip(if several {
                "Repository (Alt+Up / Alt+Down)"
            } else {
                "Repository"
            });
        if !several {
            return Some(button.into_any_element());
        }
        Some(
            button
                .dropdown_menu(move |menu: PopupMenu, _, _| {
                    let mut menu = menu.label("Repositories");
                    for (root, name, branch, changed, on) in &roots {
                        let (view, root) = (view.clone(), root.clone());
                        let detail = match changed {
                            0 => format!("{branch} · clean"),
                            n => format!("{branch} · {n} changed"),
                        };
                        menu = menu.item(
                            PopupMenuItem::new(format!("{name}   {detail}"))
                                .icon(Glyph::Folder)
                                .checked(*on)
                                .on_click(move |_, _, cx| {
                                    let root = root.clone();
                                    view.update(cx, |git, cx| git.pick_repo(root, cx)).ok();
                                }),
                        );
                    }
                    menu
                })
                .into_any_element(),
        )
    }

    // Fetch, pull and push, with how far the branch is from its upstream.
    pub fn tools(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = cx.theme();
        let repo = self.repo(cx)?;
        let sync = repo.sync().clone();
        let detached = repo.is_detached();
        let busy = self.busy;
        let muted = theme.muted_foreground;
        let view = cx.entity().downgrade();
        let act = move |op: Op| {
            let view = view.clone();
            move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
                view.update(cx, |git, cx| git.act(op.clone(), window, cx))
                    .ok();
            }
        };
        let count = |n: usize| {
            div()
                .font_family("monospace")
                .text_xs()
                .when(n == 0, |c| c.text_color(muted))
                .child(n.to_string())
        };
        let push_label = if sync.upstream.is_none() && !detached {
            "Publish"
        } else {
            "Push"
        };
        let upstream = sync.upstream.is_none().then(|| "origin".to_owned());
        let push_tip: SharedString = match &sync.upstream {
            Some(up) => format!("Push to {up}").into(),
            None => "Publish the branch to origin".into(),
        };
        let pull_tip: SharedString = match &sync.upstream {
            Some(up) => format!("Pull from {up}").into(),
            None => "The branch tracks nothing to pull from".into(),
        };
        let group = h_flex()
            .gap_1()
            .when_some(busy, |g, busy| {
                g.child(div().text_xs().text_color(muted).child(format!("{busy}…")))
            })
            .child(
                Button::new("git-fetch")
                    .ghost()
                    .small()
                    .icon(Glyph::RefreshCw)
                    .tooltip("Fetch every remote")
                    .disabled(busy.is_some())
                    .on_click(act(Op::Fetch)),
            )
            .child(
                Button::new("git-pull")
                    .ghost()
                    .small()
                    .icon(Glyph::ArrowDown)
                    .child(count(sync.behind))
                    .tooltip(pull_tip)
                    .disabled(busy.is_some() || sync.upstream.is_none())
                    .on_click(act(Op::Pull)),
            )
            .child(
                Button::new("git-push")
                    .outline()
                    .small()
                    .icon(Glyph::ArrowUp)
                    .when(sync.upstream.is_some(), |b| b.child(count(sync.ahead)))
                    .label(push_label)
                    .tooltip(push_tip)
                    .disabled(busy.is_some() || detached)
                    .on_click(act(Op::Push { upstream })),
            );
        Some(group.into_any_element())
    }

    // What the window's palette offers on the screen shown.
    pub fn offers(&self, query: &str, cx: &App) -> Vec<Offer> {
        match self.tab {
            Tab::Changes => self.changes.read(cx).offers(query, cx),
            Tab::History => self.history.read(cx).offers(query, cx),
            Tab::Branches => self.branches.read(cx).offers(query, cx),
            Tab::Stash => self.stashes.read(cx).offers(query, cx),
            Tab::Worktrees => self.worktrees.read(cx).offers(query, cx),
            Tab::Conflicts => self.conflicts.read(cx).offers(query, cx),
        }
    }

    fn rail(&self, cx: &mut Context<Self>) -> Rail {
        let changed = self.repo(cx).map_or(0, |repo| repo.changes().len());
        let conflicts = self.repo(cx).map_or(0, |repo| repo.conflicts().count());
        let working = self.worktrees.read(cx).agent_working(cx);
        let claude: gpui_kit::Hsla = gpui_kit::rgb(0xd97757).into();
        let conflicted = self.conflicted(cx);
        let mut stops = Vec::new();
        let mut shown = 0;
        for (ix, tab) in Tab::ALL.into_iter().enumerate() {
            if tab == Tab::Conflicts && !conflicted {
                continue;
            }
            if tab == self.tab {
                shown = stops.len();
            }
            let view = cx.entity().downgrade();
            let stop = Stop::new(("git-tab", ix), tab.glyph(), tab.label(), move |_, cx| {
                view.update(cx, |git, cx| git.show_tab(tab, cx)).ok();
            })
            .key(format!("Alt+{}", ix + 1));
            let stop = match tab {
                Tab::Changes => stop.count(changed),
                Tab::Worktrees => stop.dot(working.then_some(claude)),
                Tab::Conflicts => stop.alarm(true).count(conflicts.max(1)),
                _ => stop,
            };
            stops.push(stop);
        }
        Rail::new(stops, shown)
    }
}

impl Render for GitWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: AnyElement = if self.root.is_none() {
            parts::note("No git repository in the linked projects.", cx)
        } else {
            match self.tab {
                Tab::Changes => self.changes.clone().into_any_element(),
                Tab::History => self.history.clone().into_any_element(),
                Tab::Branches => self.branches.clone().into_any_element(),
                Tab::Stash => self.stashes.clone().into_any_element(),
                Tab::Worktrees => self.worktrees.clone().into_any_element(),
                Tab::Conflicts => self.conflicts.clone().into_any_element(),
            }
        };
        h_flex()
            .size_full()
            .child(self.rail(cx))
            .child(div().relative().flex_1().min_w_0().h_full().child(body))
    }
}

// What the dev is asked before git does something that can lose work: a title, what is lost,
// and the button that does it.
fn ask(op: &Op) -> Option<(String, String, String)> {
    if !op.loses_work() {
        return None;
    }
    Some(match op {
        Op::Discard { tracked, untracked } => {
            let n = tracked.len() + untracked.len();
            let what = if n == 1 {
                let path = tracked
                    .first()
                    .or(untracked.first())
                    .cloned()
                    .unwrap_or_default();
                format!("The changes to {path} that are not staged are lost.")
            } else {
                format!("The changes to {n} files that are not staged are lost.")
            };
            let deleted = if untracked.is_empty() {
                String::new()
            } else {
                format!(" {} new files are deleted.", untracked.len())
            };
            ("Discard changes?".into(), what + &deleted, "Discard".into())
        }
        Op::Apply { .. } => (
            "Discard these lines?".into(),
            "The lines go back to what the index has; what was typed in them is lost.".into(),
            "Discard".into(),
        ),
        Op::ResetTo { rev, .. } => (
            format!("Reset hard to {}?", short(rev)),
            "The branch moves there, and every change not committed is lost.".into(),
            "Reset".into(),
        ),
        Op::StashDrop(ix) => (
            format!("Drop stash@{{{ix}}}?"),
            "The work kept in it is lost.".into(),
            "Drop".into(),
        ),
        Op::DeleteBranch { name, .. } => (
            format!("Delete {name}?"),
            "It has commits no other branch holds; they are lost.".into(),
            "Delete".into(),
        ),
        Op::RemoveWorktree { path, .. } => (
            "Remove the worktree?".into(),
            format!(
                "{} has changes not committed; they are deleted with it.",
                path.display()
            ),
            "Remove".into(),
        ),
        Op::Abort => (
            "Give up?".into(),
            "Everything goes back to how it was before it started, and what was resolved is lost."
                .into(),
            "Abort".into(),
        ),
        _ => ("Are you sure?".into(), String::new(), "Go on".into()),
    })
}

// What is said once git did it, for what shows nowhere else.
fn said(op: &Op) -> Option<String> {
    Some(match op {
        Op::Fetch => "Fetched.".into(),
        Op::Pull => "Pulled.".into(),
        Op::Push { .. } => "Pushed.".into(),
        Op::Commit { amend: true, .. } => "Amended.".into(),
        Op::Commit { .. } => "Committed.".into(),
        Op::Tag { name, .. } => format!("Tagged {name}."),
        Op::Branch { name, .. } => format!("Made {name}."),
        Op::Stash { .. } => "Stashed.".into(),
        Op::CherryPick(rev) => format!("Picked {}.", short(rev)),
        Op::Revert(rev) => format!("Reverted {}.", short(rev)),
        Op::Merge(rev) => format!("Merged {rev}."),
        Op::Rebase(onto) => format!("Rebased onto {onto}."),
        Op::AddWorktree { path, .. } => format!("Made the worktree {}.", path.display()),
        _ => return None,
    })
}

pub fn short(rev: &str) -> &str {
    if rev.len() == 40 && rev.chars().all(|c| c.is_ascii_hexdigit()) {
        &rev[..7]
    } else {
        rev
    }
}
