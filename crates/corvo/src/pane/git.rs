use std::path::{Path, PathBuf};

use corvo_domain::{Change, Changed, Repo};
use corvo_ui::file_icon::FileIcon;
use corvo_ui::git::tint;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::diff::{Diff, DiffFile, DiffState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, EventEmitter, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, WeakEntity, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::repos::Repos;

pub enum Event {
    // Asked to open a changed file in the code.
    Open(PathBuf),
}

// The width of the list of changed files.
const LIST: f32 = 280.;

// The git window: on the left the changed files of each repository, under it; on the right the
// patch of the file picked, from the last commit to the file on disk. It takes turns with the
// code and the chats over the home.
pub struct GitWindow {
    repos: Entity<Repos>,
    picked: Option<PathBuf>,
    diff: Entity<DiffState>,
    // The patch shown did not read, and why.
    trouble: Option<String>,
    // The file and patch shown. Showing a patch again starts it over at the top, so one read
    // again the same is left as it is.
    shown: Option<(PathBuf, String)>,
    // The read of the patch under way; a newer one drops it.
    _reading: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for GitWindow {}

impl GitWindow {
    pub fn new(repos: Entity<Repos>, cx: &mut Context<Self>) -> Self {
        let diff = cx.new(|cx| DiffState::new(Vec::new(), cx));
        // What changed moved on: the pick stays while its file still differs.
        let subs = vec![cx.observe(&repos, |this, _, cx| this.reread(cx))];
        let mut window = Self {
            repos,
            picked: None,
            diff,
            trouble: None,
            shown: None,
            _reading: None,
            _subs: subs,
        };
        window.reread(cx);
        window
    }

    // Read again when the window comes over the home: the files may have changed meanwhile.
    pub fn reread(&mut self, cx: &mut Context<Self>) {
        let repos = self.repos.read(cx);
        let still = self
            .picked
            .as_deref()
            .is_some_and(|path| repos.change_of(path).is_some());
        if !still {
            self.picked = repos
                .repos()
                .iter()
                .flat_map(Repo::changes)
                .next()
                .map(|changed| changed.path.clone());
        }
        self.read(cx);
    }

    pub fn pick(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.picked = Some(path);
        self.read(cx);
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        let target = self.picked.clone().and_then(|path| {
            let root = self.repos.read(cx).repo_of(&path)?.root().to_path_buf();
            Some((root, path))
        });
        let Some((root, path)) = target else {
            self._reading = None;
            self.trouble = None;
            if self.shown.take().is_some() {
                self.diff
                    .update(cx, |diff, cx| diff.set_files(Vec::new(), cx));
            }
            cx.notify();
            return;
        };
        let git = self.repos.read(cx).git();
        let file = path.clone();
        let task = cx
            .background_executor()
            .spawn(async move { git.patch(&root, &file) });
        self._reading = Some(cx.spawn(async move |this, cx| {
            let patch = task.await;
            this.update(cx, |this, cx| {
                let same = |patch: &String| {
                    this.shown
                        .as_ref()
                        .is_some_and(|(shown, text)| *shown == path && text == patch)
                };
                if patch.as_ref().is_ok_and(same) && this.trouble.is_none() {
                    return;
                }
                let read = patch.map_err(|err| err.to_string()).and_then(|patch| {
                    let files = DiffFile::parse(&patch).map_err(|err| err.message().to_string());
                    files.map(|files| (patch, files))
                });
                let files = match read {
                    Ok((patch, files)) => {
                        this.trouble = None;
                        this.shown = Some((path, patch));
                        files
                    }
                    Err(why) => {
                        this.trouble = Some(why);
                        this.shown = None;
                        Vec::new()
                    }
                };
                this.diff.update(cx, |diff, cx| diff.set_files(files, cx));
                cx.notify();
            })
            .ok();
        }));
    }

    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let repos = self.repos.read(cx).repos();
        if repos.is_empty() {
            return note("No git repository in the linked projects.", cx);
        }
        let view = cx.entity().downgrade();
        let mut list = v_flex().gap_3();
        for (ix, repo) in repos.iter().enumerate() {
            let name = repo
                .root()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| repo.root().display().to_string());
            let head = h_flex()
                .px_2()
                .gap_2()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(name),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(Icon::new(Glyph::GitBranch).xsmall())
                        .child(repo.branch().to_owned()),
                )
                .child(div().flex_1())
                .child(repo.changes().len().to_string());
            let rows: Vec<AnyElement> = if repo.changes().is_empty() {
                vec![
                    div()
                        .px_2()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Nothing changed since the last commit.")
                        .into_any_element(),
                ]
            } else {
                repo.changes()
                    .iter()
                    .map(|changed| self.row(ix, repo.root(), changed, &view, cx))
                    .collect()
            };
            list = list.child(v_flex().gap_0p5().child(head).children(rows));
        }
        list.into_any_element()
    }

    fn row(
        &self,
        repo: usize,
        root: &Path,
        changed: &Changed,
        view: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let color = tint(changed.change, cx);
        let on = self.picked.as_deref() == Some(changed.path.as_path());
        let name = changed
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = changed
            .path
            .parent()
            .and_then(|dir| dir.strip_prefix(root).ok())
            .map(|dir| dir.display().to_string())
            .unwrap_or_default();
        let (picker, opener) = (view.clone(), view.clone());
        let path = changed.path.clone();
        // A file gone from disk has nothing to open.
        let gone = changed.change == Change::Deleted;
        h_flex()
            .id(ElementId::Name(
                format!("git:{repo}:{}", changed.path.display()).into(),
            ))
            .h(px(26.))
            .px_2()
            .gap_2()
            .rounded(px(6.))
            .cursor_pointer()
            .text_sm()
            .hover(|row| row.bg(theme.muted))
            .when(on, |row| row.bg(theme.muted))
            .on_click(move |event, _, cx| {
                let path = path.clone();
                if event.click_count() > 1 && !gone {
                    opener.update(cx, |_, cx| cx.emit(Event::Open(path))).ok();
                } else {
                    picker.update(cx, |this, cx| this.pick(path, cx)).ok();
                }
            })
            .child(FileIcon::new(name.clone()).size(px(14.)))
            .child(div().flex_shrink_0().text_color(color).child(name))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(dir),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(12.))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color)
                    .child(changed.change.letter().to_string()),
            )
            .into_any_element()
    }

    fn patch(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(path) = self.picked.clone() else {
            return note("Pick a file to see what changed in it.", cx);
        };
        if let Some(why) = &self.trouble {
            return note(&format!("The patch did not read: {why}"), cx);
        }
        let opener = cx.entity().downgrade();
        let shown = self
            .repos
            .read(cx)
            .repo_of(&path)
            .and_then(|repo| path.strip_prefix(repo.root()).ok())
            .unwrap_or(&path)
            .display()
            .to_string();
        let gone = self.repos.read(cx).change_of(&path) == Some(Change::Deleted);
        let head = h_flex()
            .h(px(40.))
            .px_3()
            .gap_2()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .font_family("monospace")
                    .child(shown),
            )
            .when(!gone, |head| {
                head.child(
                    Button::new("git-open")
                        .ghost()
                        .small()
                        .icon(Glyph::Code)
                        .label("Open")
                        .tooltip("Open in the code")
                        .on_click(move |_, _, cx| {
                            let path = path.clone();
                            opener.update(cx, |_, cx| cx.emit(Event::Open(path))).ok();
                        }),
                )
            });
        v_flex()
            .size_full()
            .child(head)
            .child(div().flex_1().min_h_0().child(Diff::new(&self.diff).header_visible(false)))
            .into_any_element()
    }
}

impl Render for GitWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let list = div()
            .id("git-list")
            .w(px(LIST))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .py_3()
            .px_1()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(self.list(cx));
        let patch = div().flex_1().min_w_0().h_full().child(self.patch(cx));
        h_flex().size_full().child(list).child(patch)
    }
}

// A line in the middle of an empty part of the window.
fn note(text: &str, cx: &App) -> AnyElement {
    div()
        .size_full()
        .p_4()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_owned())
        .into_any_element()
}
