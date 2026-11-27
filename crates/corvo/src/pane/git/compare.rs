// What a branch has that a base lacks, as a pull request would show it, shared by the branches
// and the worktrees: the commits since they parted, the files changed since then and their
// patch, and whether merging it would stop on conflicts.

use std::path::PathBuf;

use corvo_domain::{Comparison, totals};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::diff::Diff;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Subscription, Task,
    Window, div, prelude::FluentBuilder as _, px,
};

use super::Ctx;
use super::parts::{PatchView, badge, group, lines, note, row, since, stat_row};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Commits,
    Files,
}

pub struct Compare {
    ctx: Ctx,
    root: Option<PathBuf>,
    // The revisions set against each other.
    pair: Option<(String, String)>,
    comparison: Option<Comparison>,
    trouble: Option<String>,
    shown: Shown,
    file: Option<String>,
    patch: Entity<PatchView>,
    _reading: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl Compare {
    pub fn new(ctx: Ctx, cx: &mut Context<Self>) -> Self {
        let patch = cx.new(PatchView::new);
        let subs = vec![cx.observe(&patch, |_, _, cx| cx.notify())];
        Self {
            ctx,
            root: None,
            pair: None,
            comparison: None,
            trouble: None,
            shown: Shown::Files,
            file: None,
            patch,
            _reading: None,
            _subs: subs,
        }
    }

    pub fn comparison(&self) -> Option<&Comparison> {
        self.comparison.as_ref()
    }

    // Sets `head` against `base`, read again even when they are the same, as either may have
    // moved.
    pub fn set(&mut self, root: PathBuf, base: String, head: String, cx: &mut Context<Self>) {
        let pair = (base.clone(), head.clone());
        if self.pair.as_ref() != Some(&pair) || self.root.as_ref() != Some(&root) {
            self.comparison = None;
            self.file = None;
        }
        self.pair = Some(pair);
        self.root = Some(root.clone());
        let git = self.ctx.repos.read(cx).git();
        let (patcher, at, b, h) = (git.clone(), root.clone(), base.clone(), head.clone());
        self.patch.update(cx, |patch, cx| {
            patch.load(move || patcher.compare_patch(&at, &b, &h), cx)
        });
        let task = cx
            .background_executor()
            .spawn(async move { git.compare(&root, &base, &head) });
        self._reading = Some(cx.spawn(async move |this, cx| {
            let compared = task.await;
            this.update(cx, |this, cx| {
                match compared {
                    Ok(comparison) => {
                        this.comparison = Some(comparison);
                        this.trouble = None;
                    }
                    Err(why) => {
                        this.comparison = None;
                        this.trouble = Some(why.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.pair = None;
        self.comparison = None;
        self.patch.update(cx, |patch, cx| patch.clear(cx));
        cx.notify();
    }

    fn pick_file(&mut self, path: String, cx: &mut Context<Self>) {
        let state = self.patch.read(cx).state().clone();
        state.update(cx, |diff, cx| diff.scroll_to_file(&path, cx));
        self.file = Some(path);
        cx.notify();
    }

    fn show(&mut self, shown: Shown, cx: &mut Context<Self>) {
        self.shown = shown;
        cx.notify();
    }

    // The line under the owner's header: how much there is, and whether it merges clean.
    pub fn summary(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(c) = &self.comparison else {
            return div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(match &self.trouble {
                    Some(why) => why.clone(),
                    None => "Comparing…".to_owned(),
                })
                .into_any_element();
        };
        let (added, removed) = totals(&c.files);
        let count = |n: usize, one: &str, many: &str| {
            h_flex()
                .gap_1()
                .child(
                    div()
                        .text_color(theme.foreground)
                        .font_weight(FontWeight::MEDIUM)
                        .child(n.to_string()),
                )
                .child(if n == 1 {
                    one.to_owned()
                } else {
                    many.to_owned()
                })
        };
        let merge = if c.commits.is_empty() {
            badge("Nothing to merge", theme.muted_foreground)
        } else if c.conflicts.is_empty() {
            badge("Merges clean", theme.green).child(Icon::new(Glyph::Check).xsmall())
        } else {
            let first = c.conflicts[0]
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned();
            let more = c.conflicts.len() - 1;
            let text = if more == 0 {
                format!("Conflicts in {first}")
            } else {
                format!("Conflicts in {first} and {more} more")
            };
            badge(text, theme.warning)
        };
        h_flex()
            .gap_3()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(count(c.commits.len(), "commit", "commits"))
            .child(count(c.files.len(), "file", "files"))
            .child(lines(added, removed, cx))
            .when(c.behind > 0, |s| {
                s.child(format!("{} behind {}", c.behind, c.base))
            })
            .child(merge)
            .into_any_element()
    }

    pub fn tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let (commits, files) = self
            .comparison
            .as_ref()
            .map_or((0, 0), |c| (c.commits.len(), c.files.len()));
        let tab = |id: &'static str,
                   label: &str,
                   n: usize,
                   on: bool,
                   to: Shown,
                   cx: &mut Context<Self>| {
            let view = cx.entity().downgrade();
            h_flex()
                .id(id)
                .h(px(22.))
                .px_2p5()
                .gap_1p5()
                .rounded(px(4.))
                .cursor_pointer()
                .text_sm()
                .when(on, |t| t.bg(theme.secondary).text_color(theme.foreground))
                .when(!on, |t| t.text_color(theme.muted_foreground))
                .child(label.to_owned())
                .child(div().text_xs().opacity(0.7).child(n.to_string()))
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| this.show(to, cx)).ok();
                })
        };
        let range = self
            .pair
            .as_ref()
            .map(|(b, h)| format!("git diff {b}...{h}"))
            .unwrap_or_default();
        h_flex()
            .gap_3()
            .child(
                h_flex()
                    .p(px(2.))
                    .gap(px(2.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme.border)
                    .child(tab(
                        "git-cmp-commits",
                        "Commits",
                        commits,
                        self.shown == Shown::Commits,
                        Shown::Commits,
                        cx,
                    ))
                    .child(tab(
                        "git-cmp-files",
                        "Files",
                        files,
                        self.shown == Shown::Files,
                        Shown::Files,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .font_family("monospace")
                    .text_color(theme.muted_foreground)
                    .child(range),
            )
            .into_any_element()
    }
}

impl Render for Compare {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let Some(c) = self.comparison.clone() else {
            return note(
                self.trouble
                    .clone()
                    .unwrap_or_else(|| "Comparing…".to_owned()),
                cx,
            );
        };
        let view = cx.entity().downgrade();
        if self.shown == Shown::Commits {
            if c.commits.is_empty() {
                return note(format!("{} has nothing {} lacks.", c.head, c.base), cx);
            }
            let rows: Vec<AnyElement> = c
                .commits
                .iter()
                .enumerate()
                .map(|(ix, commit)| {
                    let (ctx, hash) = (self.ctx.clone(), commit.hash.clone());
                    row(("git-cmp-commit", ix), false, cx)
                        .on_click(move |_, _, cx| {
                            let hash = hash.clone();
                            ctx.window
                                .update(cx, |git, cx| git.pick_commit(hash, cx))
                                .ok();
                        })
                        .child(Icon::new(Glyph::GitCommitHorizontal).xsmall())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(commit.subject.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_family("monospace")
                                .text_color(theme.muted_foreground)
                                .child(commit.short().to_owned()),
                        )
                        .child(
                            div()
                                .w(px(30.))
                                .text_xs()
                                .text_right()
                                .text_color(theme.muted_foreground)
                                .child(since(commit.when)),
                        )
                        .into_any_element()
                })
                .collect();
            return div()
                .id("git-cmp-commits-list")
                .size_full()
                .overflow_y_scroll()
                .py_1p5()
                .children(rows)
                .into_any_element();
        }
        let files: Vec<AnyElement> = c
            .files
            .iter()
            .enumerate()
            .map(|(ix, file)| {
                let on = self.file.as_ref() == Some(&file.path);
                let conflicting = c.conflicts.contains(&file.path);
                let (view, path) = (view.clone(), file.path.clone());
                stat_row(("git-cmp-file", ix), file, on, cx)
                    .when(conflicting, |r| {
                        r.child(
                            Icon::new(Glyph::TriangleAlert)
                                .xsmall()
                                .text_color(theme.warning),
                        )
                    })
                    .on_click(move |_, _, cx| {
                        let path = path.clone();
                        view.update(cx, |this, cx| this.pick_file(path, cx)).ok();
                    })
                    .into_any_element()
            })
            .collect();
        let (added, removed) = totals(&c.files);
        let patch = self.patch.read(cx);
        let diff = match patch.trouble() {
            Some(why) => note(format!("The patch did not read: {why}"), cx),
            None if c.files.is_empty() => note("No file changed.", cx),
            None => Diff::new(patch.state()).into_any_element(),
        };
        h_flex()
            .size_full()
            .items_start()
            .child(
                v_flex()
                    .id("git-cmp-files-list")
                    .w(px(250.))
                    .h_full()
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .py_1p5()
                    .border_r_1()
                    .border_color(theme.border)
                    .child(group("Files", c.files.len(), cx).child(lines(added, removed, cx)))
                    .children(files),
            )
            .child(div().flex_1().min_w_0().h_full().child(diff))
            .into_any_element()
    }
}
