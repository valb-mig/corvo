use std::path::PathBuf;

use corvo_domain::{Change, Changed, Doing, Op};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::diff::{Diff, DiffEvent, DiffFile, DiffState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, div, prelude::FluentBuilder as _, px,
};

use super::parts::{file_label, group, note, pill, row};
use super::{Ctx, Event};
use crate::palette::{Offer, finds};

// What a merge, rebase, cherry-pick or revert stopped on, while it waits on the dev: a band on
// top telling what is half done, to go on, skip the step or give up; the files in conflict and
// those ready; and the file picked with each conflict in place, its sides kept one by one.
pub struct Conflicts {
    ctx: Ctx,
    root: Option<PathBuf>,
    picked: Option<PathBuf>,
    state: Entity<DiffState>,
    // The file's text as read, to tell whether a conflict was resolved since.
    trouble: Option<String>,
    resolved: usize,
    _reading: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl Conflicts {
    pub fn new(ctx: Ctx, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| DiffState::new(Vec::new(), cx));
        let subs = vec![cx.subscribe(&state, |this, state, event: &DiffEvent, cx| {
            if let DiffEvent::ConflictResolved(path, _) = event {
                let state = state.read(cx);
                let count = state
                    .files()
                    .first()
                    .map_or(0, |file| file.conflicts().len());
                this.resolved = (0..count)
                    .filter(|ix| state.conflict_resolution(path, *ix).is_some())
                    .count();
                cx.notify();
            }
        })];
        Self {
            ctx,
            root: None,
            picked: None,
            state,
            trouble: None,
            resolved: 0,
            _reading: None,
            _subs: subs,
        }
    }

    fn files<'a>(&self, cx: &'a App) -> Vec<&'a Changed> {
        self.root
            .as_deref()
            .and_then(|root| self.ctx.repo(root, cx))
            .map_or(Vec::new(), |repo| repo.changes().iter().collect())
    }

    fn doing<'a>(&self, cx: &'a App) -> Option<&'a Doing> {
        self.ctx.repo(self.root.as_deref()?, cx)?.doing()
    }

    pub fn show(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        self.root = root;
        let conflicted: Vec<PathBuf> = self
            .files(cx)
            .into_iter()
            .filter(|c| c.change == Change::Conflicted)
            .map(|c| c.path.clone())
            .collect();
        let still = self.picked.as_ref().is_some_and(|p| conflicted.contains(p));
        if !still {
            self.picked = conflicted.first().cloned();
            self.read(cx);
        }
        cx.notify();
    }

    fn pick(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.picked = Some(path);
        self.read(cx);
        cx.notify();
    }

    fn inner(&self, path: &std::path::Path) -> Option<String> {
        let inner = path.strip_prefix(self.root.as_deref()?).ok()?;
        Some(inner.to_string_lossy().replace('\\', "/"))
    }

    // Reads the file picked from disk, with its conflict markers.
    fn read(&mut self, cx: &mut Context<Self>) {
        self.resolved = 0;
        let Some(path) = self.picked.clone() else {
            self.state.update(cx, |s, cx| s.set_files(Vec::new(), cx));
            return;
        };
        let inner = self.inner(&path).unwrap_or_default();
        let task = cx
            .background_executor()
            .spawn(async move { std::fs::read_to_string(&path) });
        self._reading = Some(cx.spawn(async move |this, cx| {
            let text = task.await;
            this.update(cx, |this, cx| {
                let file = text.map_err(|err| err.to_string()).and_then(|text| {
                    DiffFile::parse_conflicts(inner.clone(), &text)
                        .map_err(|err| err.message().to_string())
                });
                match file {
                    Ok(file) => {
                        this.trouble = None;
                        this.state.update(cx, |s, cx| s.set_files(vec![file], cx));
                    }
                    Err(why) => {
                        this.trouble = Some(why);
                        this.state.update(cx, |s, cx| s.set_files(Vec::new(), cx));
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn conflicts(&self, cx: &App) -> usize {
        self.state
            .read(cx)
            .files()
            .first()
            .map_or(0, |file| file.conflicts().len())
    }

    // Writes the file as resolved in the window, then marks it resolved. A file with no
    // conflict left in it, resolved in the code, is only marked.
    fn mark_resolved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(path), Some(inner)) = (
            self.picked.clone(),
            self.picked.as_deref().and_then(|p| self.inner(p)),
        ) else {
            return;
        };
        let text = self.state.read(cx).resolved_text(&inner);
        let ctx = self.ctx.clone();
        let task = cx.background_executor().spawn(async move {
            match text {
                Some(text) => std::fs::write(&path, text),
                None => Ok(()),
            }
        });
        cx.spawn_in(window, async move |_, cx| {
            let written = task.await;
            cx.update(|window, cx| match written {
                Ok(()) => ctx.act(Op::Stage(vec![inner]), window, cx),
                Err(why) => corvo_ui::notice::warn(why.to_string(), window, cx),
            })
            .ok();
        })
        .detach();
    }

    pub fn offers(&self, query: &str, cx: &App) -> Vec<Offer> {
        self.files(cx)
            .into_iter()
            .filter(|c| c.change == Change::Conflicted)
            .filter_map(|c| {
                let inner = self.inner(&c.path)?;
                if !finds(query, &inner) {
                    return None;
                }
                let (view, path) = (self.ctx.window.clone(), c.path.clone());
                Some(
                    Offer::new(Glyph::GitMergeConflict, inner.clone(), move |_, cx| {
                        let path = path.clone();
                        view.update(cx, |git, cx| git.pick_conflict(path, cx)).ok();
                    })
                    .detail("in conflict"),
                )
            })
            .collect()
    }

    pub fn pick_conflict(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.pick(path, cx);
    }

    fn band(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let doing = self.doing(cx).cloned();
        let branch = self
            .root
            .as_deref()
            .and_then(|root| self.ctx.repo(root, cx))
            .map(|r| r.branch().to_owned())
            .unwrap_or_default();
        let left = self
            .files(cx)
            .iter()
            .filter(|c| c.change == Change::Conflicted)
            .count();
        let what: Vec<AnyElement> = match &doing {
            Some(Doing::Merge { theirs }) => vec![
                div().child("Merging").into_any_element(),
                pill(theirs, gpui_kit::rgb(0xc678dd).into(), false, cx),
                div().child("into").into_any_element(),
                pill(&branch, theme.blue, false, cx),
            ],
            Some(Doing::Rebase {
                branch,
                onto,
                step,
                steps,
            }) => vec![
                div().child("Rebasing").into_any_element(),
                pill(branch, gpui_kit::rgb(0xc678dd).into(), false, cx),
                div().child("onto").into_any_element(),
                pill(onto, theme.blue, false, cx),
                div()
                    .text_color(theme.muted_foreground)
                    .child(format!("step {step} of {steps}"))
                    .into_any_element(),
            ],
            Some(Doing::CherryPick { theirs }) => vec![
                div().child("Cherry-picking").into_any_element(),
                pill(theirs, theme.blue, false, cx),
            ],
            Some(Doing::Revert { theirs }) => vec![
                div().child("Reverting").into_any_element(),
                pill(theirs, theme.blue, false, cx),
            ],
            None => vec![div().child("Files left in conflict").into_any_element()],
        };
        let ctx = self.ctx.clone();
        let act = |op: Op| {
            let ctx = ctx.clone();
            move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
                ctx.act(op.clone(), window, cx)
            }
        };
        let can_skip = doing.as_ref().is_some_and(Doing::can_skip);
        let finish = match &doing {
            Some(Doing::Merge { .. }) => "Commit merge",
            _ => "Continue",
        };
        h_flex()
            .h(px(46.))
            .flex_shrink_0()
            .px_4()
            .gap_2()
            .bg(theme.warning.opacity(0.1))
            .border_b_1()
            .border_color(theme.warning.opacity(0.35))
            .child(
                Icon::new(Glyph::GitMergeConflict)
                    .small()
                    .text_color(theme.warning),
            )
            .children(what)
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(match left {
                        0 => "· everything resolved".to_owned(),
                        1 => "· 1 file in conflict".to_owned(),
                        n => format!("· {n} files in conflict"),
                    }),
            )
            .child(div().flex_1())
            .when(doing.is_some(), |b| {
                b.child(
                    Button::new("git-abort")
                        .ghost()
                        .small()
                        .with_variant(ButtonVariant::Danger)
                        .icon(Glyph::X)
                        .label("Abort")
                        .on_click(act(Op::Abort)),
                )
                .when(can_skip, |b| {
                    b.child(
                        Button::new("git-skip")
                            .outline()
                            .small()
                            .icon(Glyph::SkipForward)
                            .label("Skip")
                            .tooltip("Leave out the commit it stopped on")
                            .on_click(act(Op::Skip)),
                    )
                })
                .child(
                    Button::new("git-continue")
                        .primary()
                        .small()
                        .icon(Glyph::Check)
                        .label(finish)
                        .disabled(left > 0)
                        .on_click(act(Op::Continue)),
                )
            })
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let files: Vec<Changed> = self.files(cx).into_iter().cloned().collect();
        let conflicted: Vec<&Changed> = files
            .iter()
            .filter(|c| c.change == Change::Conflicted)
            .collect();
        let ready: Vec<&Changed> = files.iter().filter(|c| c.stage.staged()).collect();
        let mut list = v_flex().gap_0p5().py_1p5();
        list = list.child(group("In conflict", conflicted.len(), cx).text_color(theme.warning));
        for (ix, c) in conflicted.iter().enumerate() {
            let on = self.picked.as_ref() == Some(&c.path);
            let (view, path) = (cx.entity().downgrade(), c.path.clone());
            let inner = self.inner(&c.path).unwrap_or_default();
            list = list.child(
                row(("git-conflict", ix), on, cx)
                    .on_click(move |_, _, cx| {
                        let path = path.clone();
                        view.update(cx, |this, cx| this.pick(path, cx)).ok();
                    })
                    .children(file_label(&inner, Change::Conflicted, cx)),
            );
        }
        if !ready.is_empty() {
            list = list.child(group("Ready", ready.len(), cx).mt_1p5());
            for (ix, c) in ready.iter().enumerate() {
                let inner = self.inner(&c.path).unwrap_or_default();
                let (ctx, path) = (self.ctx.clone(), c.path.clone());
                list = list.child(
                    row(("git-ready", ix), false, cx)
                        .on_click(move |_, _, cx| ctx.emit(Event::Open(path.clone()), cx))
                        .children(file_label(&inner, c.stage.index.unwrap_or(c.change), cx))
                        .child(Icon::new(Glyph::Check).xsmall().text_color(theme.green)),
                );
            }
        }
        list = list.child(
            div()
                .px_4()
                .py_3()
                .text_xs()
                .line_height(px(17.))
                .text_color(theme.muted_foreground)
                .child(
                    "Keep a side of each conflict, or both, then Mark resolved. It goes on once \
                     no file is left in conflict; Abort puts everything back as it was.",
                ),
        );
        list.into_any_element()
    }

    fn file(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(path) = self.picked.clone() else {
            return note("Nothing left in conflict.", cx);
        };
        if let Some(why) = &self.trouble {
            return note(format!("The file did not read: {why}"), cx);
        }
        let inner = self.inner(&path).unwrap_or_default();
        let (name, dir) = super::parts::split(&inner);
        let total = self.conflicts(cx);
        let ctx = self.ctx.clone();
        let side = |ours: bool| {
            let (ctx, path) = (ctx.clone(), inner.clone());
            move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
                ctx.act(
                    Op::TakeSide {
                        path: path.clone(),
                        ours,
                    },
                    window,
                    cx,
                )
            }
        };
        let rebasing = matches!(self.doing(cx), Some(Doing::Rebase { .. }));
        // In a rebase, ours is the branch rebased onto and theirs the commit being replayed.
        let (ours, theirs) = if rebasing {
            ("All upstream", "All mine")
        } else {
            ("All ours", "All theirs")
        };
        let (ask_ctx, root, ask_inner) = (ctx.clone(), self.root.clone(), inner.clone());
        let (open_ctx, open_path) = (ctx.clone(), path.clone());
        let mark = cx.entity().downgrade();
        let progress = if total == 0 {
            "no conflict left in it".to_owned()
        } else {
            format!("{} of {total} resolved", self.resolved)
        };
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .h(px(40.))
                    .flex_shrink_0()
                    .px_3()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        h_flex()
                            .min_w_0()
                            .text_sm()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(theme.muted_foreground)
                                    .child(if dir.is_empty() {
                                        dir
                                    } else {
                                        format!("{dir}/")
                                    }),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(name),
                            ),
                    )
                    .child(div().text_xs().text_color(theme.warning).child(progress))
                    .child(div().flex_1())
                    .child(
                        Button::new("git-ours")
                            .ghost()
                            .small()
                            .label(ours)
                            .on_click(side(true)),
                    )
                    .child(
                        Button::new("git-theirs")
                            .ghost()
                            .small()
                            .label(theirs)
                            .on_click(side(false)),
                    )
                    .child(
                        Button::new("git-ask-conflict")
                            .ghost()
                            .small()
                            .icon(Glyph::Sparkles)
                            .label("Ask the agent")
                            .on_click(move |_, _, cx| {
                                let Some(project) = root.clone() else {
                                    return;
                                };
                                ask_ctx.emit(
                                    Event::Ask {
                                        project,
                                        prompt: format!(
                                            "Resolve the merge conflicts in {ask_inner}: keep \
                                             what both sides meant, take out the markers, and \
                                             do not stage it."
                                        ),
                                    },
                                    cx,
                                );
                            }),
                    )
                    .child(
                        Button::new("git-open-conflict")
                            .ghost()
                            .small()
                            .icon(Glyph::Code)
                            .label("Edit")
                            .on_click(move |_, _, cx| {
                                open_ctx.emit(Event::Open(open_path.clone()), cx)
                            }),
                    )
                    .child(
                        Button::new("git-mark-resolved")
                            .primary()
                            .small()
                            .icon(Glyph::Check)
                            .label("Mark resolved")
                            .disabled(total > 0 && self.resolved < total)
                            .on_click(move |_, window, cx| {
                                mark.update(cx, |this, cx| this.mark_resolved(window, cx))
                                    .ok();
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Diff::new(&self.state).header_visible(false).soft_wrap(true)),
            )
            .into_any_element()
    }
}

impl Render for Conflicts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let band = self.band(cx);
        let list = self.list(cx);
        let file = self.file(cx);
        v_flex().size_full().child(band).child(
            h_flex()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id("git-conflict-list")
                        .w(px(300.))
                        .h_full()
                        .flex_shrink_0()
                        .overflow_y_scroll()
                        .border_r_1()
                        .border_color(border)
                        .child(list),
                )
                .child(div().flex_1().min_w_0().h_full().child(file)),
        )
    }
}
