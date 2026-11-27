use std::path::{Path, PathBuf};

use corvo_domain::port::Side;
use corvo_domain::{Change, Changed, Op, Patch};
use corvo_ui::drawer::{self, Drawer};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::diff::{
    Diff, DiffEvent, DiffGutterMarker, DiffLinePosition, DiffLineRange, DiffSide,
};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, ElementExt as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, prelude::FluentBuilder as _, px,
};

use super::parts::{self, PatchView, badge, group, lines, note, row};
use super::{Ctx, Event, Tab};
use crate::palette::{Offer, finds};

// How wide the list of changed files floats on the right.
const PANEL: f32 = 316.;

// The changes of the repository shown: the patch of the file picked, staged or not, in the
// middle, and floating on the right the files staged and those not, with the box the commit is
// written in under them. A hunk stages, or unstages, by the button beside its first line, and
// lines picked in the gutter by the buttons on top.
pub struct Changes {
    ctx: Ctx,
    root: Option<PathBuf>,
    picked: Option<(PathBuf, Side)>,
    patch: Entity<PatchView>,
    // Lines picked in the patch, to stage, unstage or discard them alone.
    lines: Option<DiffLineRange>,
    message: Entity<TextareaState>,
    amend: bool,
    panel: bool,
    _subs: Vec<Subscription>,
}

impl Changes {
    pub fn new(ctx: Ctx, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let patch = cx.new(PatchView::new);
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message (Ctrl+Enter commits)")
                .auto_grow(3, 8)
        });
        let state = patch.read(cx).state().clone();
        let subs = vec![
            cx.observe(&patch, |_, _, cx| cx.notify()),
            cx.subscribe(&state, |this, _, event: &DiffEvent, cx| match event {
                DiffEvent::SelectionEnded(range) => {
                    this.lines = Some(range.clone());
                    cx.notify();
                }
                DiffEvent::SelectionChanged(None) => {
                    this.lines = None;
                    cx.notify();
                }
                _ => {}
            }),
        ];
        Self {
            ctx,
            root: None,
            picked: None,
            patch,
            lines: None,
            message,
            amend: false,
            panel: true,
            _subs: subs,
        }
    }

    pub fn has_panel(&self) -> bool {
        self.panel
    }

    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) -> bool {
        self.panel = !self.panel;
        cx.notify();
        self.panel
    }

    fn changes<'a>(&self, cx: &'a App) -> &'a [Changed] {
        self.root
            .as_deref()
            .and_then(|root| self.ctx.repo(root, cx))
            .map_or(&[], |repo| repo.changes())
    }

    fn change(&self, path: &Path, cx: &App) -> Option<Changed> {
        self.changes(cx).iter().find(|c| c.path == path).cloned()
    }

    // Shows the repository's changes, keeping the file picked while it still differs.
    pub fn show(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        if root != self.root {
            self.picked = None;
        }
        self.root = root;
        let still = self.picked.as_ref().is_some_and(|(path, side)| {
            self.change(path, cx).is_some_and(|c| match side {
                Side::Staged => c.stage.staged(),
                Side::Unstaged => c.stage.unstaged() || c.change == Change::Conflicted,
            })
        });
        if !still {
            let changes = self.changes(cx);
            // The file picked moved to the other side, or the first one there is.
            let moved = self
                .picked
                .as_ref()
                .and_then(|(path, _)| changes.iter().find(|c| c.path == *path));
            self.picked = moved
                .or_else(|| changes.first())
                .map(|c| (c.path.clone(), side_of(c)));
        }
        self.read(cx);
        cx.notify();
    }

    pub fn pick(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let side = self
            .change(&path, cx)
            .map_or(Side::Unstaged, |c| side_of(&c));
        self.pick_side(path, side, cx);
    }

    fn pick_side(&mut self, path: PathBuf, side: Side, cx: &mut Context<Self>) {
        self.picked = Some((path, side));
        self.lines = None;
        self.read(cx);
        cx.notify();
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        let (Some(root), Some((path, side))) = (self.root.clone(), self.picked.clone()) else {
            self.patch.update(cx, |patch, cx| patch.clear(cx));
            return;
        };
        let conflicted = self
            .change(&path, cx)
            .is_some_and(|c| c.change == Change::Conflicted);
        let git = self.ctx.repos.read(cx).git();
        self.patch.update(cx, |patch, cx| {
            patch.load(
                move || {
                    if conflicted {
                        git.patch(&root, &path)
                    } else {
                        git.side_patch(&root, &path, side)
                    }
                },
                cx,
            )
        });
    }

    // The commit went in: its message is cleared.
    pub fn committed(&mut self, cx: &mut Context<Self>) {
        self.amend = false;
        cx.notify();
    }

    fn inner(&self, path: &Path) -> Option<String> {
        let inner = path.strip_prefix(self.root.as_deref()?).ok()?;
        Some(inner.to_string_lossy().replace('\\', "/"))
    }

    fn act(&self, op: Op, window: &mut Window, cx: &mut App) {
        self.ctx.act(op, window, cx);
    }

    fn stage(&self, paths: &[PathBuf], window: &mut Window, cx: &mut App) {
        let inner: Vec<String> = paths.iter().filter_map(|p| self.inner(p)).collect();
        if !inner.is_empty() {
            self.act(Op::Stage(inner), window, cx);
        }
    }

    fn unstage(&self, paths: &[PathBuf], window: &mut Window, cx: &mut App) {
        let inner: Vec<String> = paths.iter().filter_map(|p| self.inner(p)).collect();
        if !inner.is_empty() {
            self.act(Op::Unstage(inner), window, cx);
        }
    }

    fn discard(&self, files: &[Changed], window: &mut Window, cx: &mut App) {
        let (mut tracked, mut untracked) = (Vec::new(), Vec::new());
        for file in files {
            let Some(inner) = self.inner(&file.path) else {
                continue;
            };
            if file.change == Change::Untracked {
                untracked.push(inner);
            } else {
                tracked.push(inner);
            }
        }
        if !(tracked.is_empty() && untracked.is_empty()) {
            self.act(Op::Discard { tracked, untracked }, window, cx);
        }
    }

    // Stages or unstages one hunk of the patch shown.
    fn hunk(&self, ix: usize, window: &mut Window, cx: &mut App) {
        let Some((_, side)) = &self.picked else {
            return;
        };
        let Some(patch) = self.patch.read(cx).text().map(Patch::parse) else {
            return;
        };
        let Some(text) = patch.hunk(ix) else {
            return;
        };
        let reverse = *side == Side::Staged;
        self.act(
            Op::Apply {
                patch: text,
                cached: true,
                reverse,
            },
            window,
            cx,
        );
    }

    // Stages, unstages or discards the lines picked; `discard` puts them back in the work tree.
    fn picked_lines(&mut self, discard: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(range), Some((_, side))) = (self.lines.clone(), self.picked.clone()) else {
            return;
        };
        let Some(patch) = self.patch.read(cx).text().map(Patch::parse) else {
            return;
        };
        // A pick across both sides stays inside the hunk it starts in.
        let start_new = range.side() == DiffSide::Modified;
        let home = patch
            .hunks
            .iter()
            .find(|hunk| hunk.holds(start_new, range.start()))
            .cloned();
        let mixed = range.side() != range.end_side();
        let picked = |new_side: bool, line: usize| {
            within(&range, new_side, line)
                && (!mixed || home.as_ref().is_some_and(|h| h.holds(new_side, line)))
        };
        let staged = side == Side::Staged;
        let reverse = staged || discard;
        let Some(text) = patch.lines(picked, reverse) else {
            return;
        };
        self.lines = None;
        self.act(
            Op::Apply {
                patch: text,
                cached: !discard,
                reverse,
            },
            window,
            cx,
        );
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let message = self.message.read(cx).value().to_string();
        let staged = self.changes(cx).iter().filter(|c| c.stage.staged()).count();
        if (message.trim().is_empty() || staged == 0) && !self.amend {
            return;
        }
        let amend = self.amend;
        self.message
            .update(cx, |box_, cx| box_.set_value("", window, cx));
        self.act(Op::Commit { message, amend }, window, cx);
    }

    // Amending starts from the last commit's message, when nothing was written yet.
    fn toggle_amend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.amend = !self.amend;
        cx.notify();
        let Some(root) = self.root.clone() else {
            return;
        };
        if !self.amend || !self.message.read(cx).value().trim().is_empty() {
            return;
        }
        let git = self.ctx.repos.read(cx).git();
        let task = cx
            .background_executor()
            .spawn(async move { git.detail(&root, "HEAD") });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(detail) = task.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                if this.amend && this.message.read(cx).value().trim().is_empty() {
                    let mut text = detail.commit.subject.clone();
                    if !detail.body.is_empty() {
                        text = format!("{text}\n\n{}", detail.body);
                    }
                    this.message
                        .update(cx, |box_, cx| box_.set_value(text, window, cx));
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn offers(&self, query: &str, cx: &App) -> Vec<Offer> {
        let mut offers = Vec::new();
        for changed in self.changes(cx) {
            let Some(inner) = self.inner(&changed.path) else {
                continue;
            };
            if !finds(query, &inner) {
                continue;
            }
            let (name, _) = parts::split(&inner);
            let detail = format!("{} · {inner}", changed.change.letter());
            let (git, path) = (self.ctx.window.clone(), changed.path.clone());
            offers.push(
                Offer::new(Glyph::GitBranch, name, move |_, cx| {
                    git.update(cx, |git, cx| git.pick(path.clone(), cx)).ok();
                })
                .detail(detail),
            );
        }
        offers
    }

    fn file_row(&self, changed: &Changed, side: Side, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let on = self
            .picked
            .as_ref()
            .is_some_and(|(path, s)| *path == changed.path && *s == side);
        let inner = self.inner(&changed.path).unwrap_or_default();
        let change = match side {
            Side::Staged => changed.stage.index.unwrap_or(changed.change),
            Side::Unstaged => changed.stage.tree.unwrap_or(changed.change),
        };
        let view = cx.entity().downgrade();
        let path = changed.path.clone();
        let gone = change == Change::Deleted;
        let group = format!("git-row:{side:?}:{inner}");
        let (opener, picker) = (self.ctx.clone(), view.clone());
        let (a, b, c) = (view.clone(), view.clone(), view.clone());
        let (pa, pb, pc) = (path.clone(), path.clone(), changed.clone());
        let acts = match side {
            Side::Staged => h_flex().child(
                Button::new(ElementId::Name(format!("unstage:{group}").into()))
                    .ghost()
                    .xsmall()
                    .icon(Glyph::Minus)
                    .tooltip("Unstage")
                    .on_click(move |_, window, cx| {
                        let path = pa.clone();
                        a.update(cx, |this, cx| this.unstage(&[path], window, cx))
                            .ok();
                    }),
            ),
            Side::Unstaged => h_flex()
                .when(changed.change != Change::Conflicted, |h| {
                    h.child(
                        Button::new(ElementId::Name(format!("discard:{group}").into()))
                            .ghost()
                            .xsmall()
                            .icon(Glyph::Undo2)
                            .tooltip("Discard")
                            .on_click(move |_, window, cx| {
                                let file = pc.clone();
                                c.update(cx, |this, cx| this.discard(&[file], window, cx))
                                    .ok();
                            }),
                    )
                })
                .child(
                    Button::new(ElementId::Name(format!("stage:{group}").into()))
                        .ghost()
                        .xsmall()
                        .icon(Glyph::Plus)
                        .tooltip(if changed.change == Change::Conflicted {
                            "Mark resolved"
                        } else {
                            "Stage"
                        })
                        .on_click(move |_, window, cx| {
                            let path = pb.clone();
                            b.update(cx, |this, cx| this.stage(&[path], window, cx))
                                .ok();
                        }),
                ),
        };
        let letter = change.letter().to_string();
        row(ElementId::from(group.clone()), on, cx)
            .group(group.clone())
            .on_click(move |event, _, cx| {
                let path = path.clone();
                if event.click_count() > 1 && !gone {
                    opener.emit(Event::Open(path), cx);
                } else {
                    picker
                        .update(cx, |this, cx| this.pick_side(path, side, cx))
                        .ok();
                }
            })
            .children(parts::file_label(&inner, change, cx))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(12.))
                    .text_xs()
                    .text_color(corvo_ui::git::tint(change, cx))
                    .group_hover(group.clone(), |l| l.invisible())
                    .child(letter),
            )
            .child(
                div()
                    .absolute()
                    .right(px(4.))
                    .invisible()
                    .group_hover(group, |a| a.visible())
                    .bg(theme.muted)
                    .rounded(px(4.))
                    .child(acts),
            )
            .relative()
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        let changes: Vec<Changed> = self.changes(cx).to_vec();
        if changes.is_empty() {
            return div()
                .px_4()
                .py_3()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Nothing changed since the last commit.")
                .into_any_element();
        }
        let conflicted: Vec<&Changed> = changes
            .iter()
            .filter(|c| c.change == Change::Conflicted)
            .collect();
        let staged: Vec<&Changed> = changes.iter().filter(|c| c.stage.staged()).collect();
        let unstaged: Vec<&Changed> = changes
            .iter()
            .filter(|c| c.stage.unstaged() && c.change != Change::Conflicted)
            .collect();
        let view = cx.entity().downgrade();
        let mut list = v_flex().gap_0p5();
        if !conflicted.is_empty() {
            let ctx = self.ctx.clone();
            list = list.child(
                group("Conflicts", conflicted.len(), cx)
                    .text_color(cx.theme().warning)
                    .child(
                        Button::new("git-to-conflicts")
                            .ghost()
                            .xsmall()
                            .label("Resolve")
                            .on_click(move |_, _, cx| {
                                ctx.window
                                    .update(cx, |git, cx| git.show_tab(Tab::Conflicts, cx))
                                    .ok();
                            }),
                    ),
            );
            for changed in &conflicted {
                list = list.child(self.file_row(changed, Side::Unstaged, cx));
            }
        }
        if !staged.is_empty() {
            let paths: Vec<PathBuf> = staged.iter().map(|c| c.path.clone()).collect();
            let all = view.clone();
            list = list.child(
                group("Staged", staged.len(), cx).child(
                    Button::new("git-unstage-all")
                        .ghost()
                        .xsmall()
                        .icon(Glyph::Minus)
                        .tooltip("Unstage all")
                        .on_click(move |_, window, cx| {
                            all.update(cx, |this, cx| this.unstage(&paths, window, cx))
                                .ok();
                        }),
                ),
            );
            for changed in &staged {
                list = list.child(self.file_row(changed, Side::Staged, cx));
            }
        }
        if !unstaged.is_empty() {
            let paths: Vec<PathBuf> = unstaged.iter().map(|c| c.path.clone()).collect();
            let files: Vec<Changed> = unstaged.iter().map(|c| (*c).clone()).collect();
            let (all, every) = (view.clone(), view.clone());
            list = list.child(
                group("Changes", unstaged.len(), cx)
                    .when(!staged.is_empty() || !conflicted.is_empty(), |g| g.mt_1p5())
                    .child(
                        Button::new("git-discard-all")
                            .ghost()
                            .xsmall()
                            .icon(Glyph::Undo2)
                            .tooltip("Discard all")
                            .on_click(move |_, window, cx| {
                                let files = files.clone();
                                every
                                    .update(cx, |this, cx| this.discard(&files, window, cx))
                                    .ok();
                            }),
                    )
                    .child(
                        Button::new("git-stage-all")
                            .ghost()
                            .xsmall()
                            .icon(Glyph::Plus)
                            .tooltip("Stage all")
                            .on_click(move |_, window, cx| {
                                all.update(cx, |this, cx| this.stage(&paths, window, cx))
                                    .ok();
                            }),
                    ),
            );
            for changed in &unstaged {
                list = list.child(self.file_row(changed, Side::Unstaged, cx));
            }
        }
        list.into_any_element()
    }

    fn commit_box(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let staged = self.changes(cx).iter().filter(|c| c.stage.staged()).count();
        let empty = self.message.read(cx).value().trim().is_empty();
        let ready = self.amend || (staged > 0 && !empty);
        let (commit, amend, write, keys) = (
            cx.entity().downgrade(),
            cx.entity().downgrade(),
            self.ctx.clone(),
            cx.entity().downgrade(),
        );
        let root = self.root.clone();
        let label = match (self.amend, staged) {
            (true, _) => "Amend".to_owned(),
            (false, 0) => "Commit".to_owned(),
            (false, n) => format!("Commit {n}"),
        };
        v_flex()
            .flex_shrink_0()
            .gap_2()
            .p_2p5()
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("git-message")
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        let key = &event.keystroke;
                        if key.key == "enter" && key.modifiers.control {
                            keys.update(cx, |this, cx| this.commit(window, cx)).ok();
                            cx.stop_propagation();
                        }
                    })
                    .child(Textarea::new(&self.message)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("git-write")
                            .ghost()
                            .small()
                            .icon(Glyph::Sparkles)
                            .label("Write")
                            .tooltip("Ask the agent for a message for what is staged")
                            .on_click(move |_, _, cx| {
                                let Some(project) = root.clone() else {
                                    return;
                                };
                                write.emit(
                                    Event::Ask {
                                        project,
                                        prompt: "Write a commit message for what is staged in \
                                                 this repository (git diff --cached), in the \
                                                 style of its last commits. Reply with the \
                                                 message alone."
                                            .into(),
                                    },
                                    cx,
                                );
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        Checkbox::new("git-amend")
                            .label("Amend")
                            .checked(self.amend)
                            .small()
                            .on_click(move |_, window, cx| {
                                amend
                                    .update(cx, |this, cx| this.toggle_amend(window, cx))
                                    .ok();
                            }),
                    )
                    .child(
                        Button::new("git-commit")
                            .primary()
                            .small()
                            .icon(Glyph::Check)
                            .label(label)
                            .tooltip("Ctrl+Enter")
                            .disabled(!ready)
                            .on_click(move |_, window, cx| {
                                commit.update(cx, |this, cx| this.commit(window, cx)).ok();
                            }),
                    ),
            )
            .into_any_element()
    }

    fn patch_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some((path, side)) = self.picked.clone() else {
            return note("Pick a file to see what changed in it.", cx);
        };
        let patch = self.patch.read(cx);
        if let Some(why) = patch.trouble() {
            return note(format!("The patch did not read: {why}"), cx);
        }
        let parsed = patch.text().map(Patch::parse);
        let state = patch.state().clone();
        let inner = self.inner(&path).unwrap_or_default();
        let (name, dir) = parts::split(&inner);
        let changed = self.change(&path, cx);
        let conflicted = changed
            .as_ref()
            .is_some_and(|c| c.change == Change::Conflicted);
        let gone = changed
            .as_ref()
            .is_some_and(|c| c.change == Change::Deleted);
        let (added, removed) = parsed.as_ref().map_or((0, 0), count);
        let staged = side == Side::Staged;
        let view = cx.entity().downgrade();

        let mut head = h_flex()
            .h(px(40.))
            .px_3()
            .gap_2()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .when(self.panel, |head| head.pr(drawer::room_of(PANEL)))
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
                    .child(div().flex_shrink_0().child(name)),
            )
            .child(if conflicted {
                badge("Conflict", theme.warning)
            } else if staged {
                badge("Staged", theme.green)
            } else {
                badge("Unstaged", theme.muted_foreground)
            })
            .child(lines(added, removed, cx))
            .child(div().flex_1());
        if self.lines.is_some() && !conflicted {
            let (a, b) = (view.clone(), view.clone());
            // A press anywhere else clears the window's text selection, the lines picked in the
            // patch with it, before the click comes: the buttons that act on them keep it.
            let actions = h_flex()
                .gap_2()
                .keep_text_selection("git-line-actions")
                .child(
                    Button::new("git-lines")
                        .ghost()
                        .small()
                        .icon(if staged { Glyph::Minus } else { Glyph::Plus })
                        .label(if staged {
                            "Unstage lines"
                        } else {
                            "Stage lines"
                        })
                        .on_click(move |_, window, cx| {
                            a.update(cx, |this, cx| this.picked_lines(false, window, cx))
                                .ok();
                        }),
                )
                .when(!staged, |h| {
                    h.child(
                        Button::new("git-discard-lines")
                            .ghost()
                            .small()
                            .icon(Glyph::Undo2)
                            .label("Discard lines")
                            .on_click(move |_, window, cx| {
                                b.update(cx, |this, cx| this.picked_lines(true, window, cx))
                                    .ok();
                            }),
                    )
                });
            head = head.child(actions);
        } else if let Some(file) = changed.clone() {
            let (a, b) = (view.clone(), view.clone());
            let p = path.clone();
            head = head
                .when(!staged && !conflicted, |h| {
                    h.child(
                        Button::new("git-discard-file")
                            .ghost()
                            .small()
                            .icon(Glyph::Undo2)
                            .label("Discard")
                            .on_click(move |_, window, cx| {
                                let file = file.clone();
                                b.update(cx, |this, cx| this.discard(&[file], window, cx))
                                    .ok();
                            }),
                    )
                })
                .child(
                    Button::new("git-stage-file")
                        .ghost()
                        .small()
                        .icon(if staged { Glyph::Minus } else { Glyph::Plus })
                        .label(match (staged, conflicted) {
                            (true, _) => "Unstage file",
                            (false, true) => "Mark resolved",
                            (false, false) => "Stage file",
                        })
                        .on_click(move |_, window, cx| {
                            let path = p.clone();
                            a.update(cx, |this, cx| {
                                if staged {
                                    this.unstage(&[path], window, cx);
                                } else {
                                    this.stage(&[path], window, cx);
                                }
                            })
                            .ok();
                        }),
                );
        }
        if !gone {
            let (ctx, p) = (self.ctx.clone(), path.clone());
            head = head.child(
                Button::new("git-open")
                    .ghost()
                    .small()
                    .icon(Glyph::Code)
                    .label("Open")
                    .tooltip("Open in the code")
                    .on_click(move |_, _, cx| ctx.emit(Event::Open(p.clone()), cx)),
            );
        }

        // A button beside the first line each hunk changes stages it, or unstages it.
        let file_path = state
            .read(cx)
            .files()
            .first()
            .map(|file| file.path().clone());
        let markers: Vec<DiffGutterMarker> = match (&parsed, file_path, conflicted) {
            (Some(parsed), Some(file), false) => parsed
                .hunks
                .iter()
                .enumerate()
                .filter_map(|(ix, hunk)| {
                    let (new_side, line) = hunk.first_change()?;
                    let side = if new_side {
                        DiffSide::Modified
                    } else {
                        DiffSide::Original
                    };
                    Some(
                        DiffGutterMarker::new(
                            ElementId::named_usize("hunk", ix),
                            DiffLinePosition::new(file.clone(), side, line),
                        )
                        .with_label(if staged {
                            "Unstage hunk"
                        } else {
                            "Stage hunk"
                        }),
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        let marked = view.clone();
        let diff = Diff::new(&state)
            .header_visible(false)
            .gutter_markers(markers)
            .render_gutter_marker(move |marker, _, _| {
                let ix = match marker.id() {
                    ElementId::NamedInteger(_, ix) => *ix as usize,
                    _ => 0,
                };
                let view = marked.clone();
                Button::new(marker.id().clone())
                    .ghost()
                    .xsmall()
                    .icon(if staged { Glyph::Minus } else { Glyph::Plus })
                    .tooltip(marker.label().clone())
                    .on_click(move |_, window, cx| {
                        view.update(cx, |this, cx| this.hunk(ix, window, cx)).ok();
                    })
            });
        v_flex()
            .size_full()
            .child(head)
            .child(div().flex_1().min_h_0().child(diff))
            .into_any_element()
    }
}

impl Render for Changes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self.panel.then(|| {
            Drawer::new("git-panel")
                .width(PANEL)
                .child(
                    div()
                        .id("git-list")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .py_1p5()
                        .child(self.list(cx)),
                )
                .child(self.commit_box(cx))
        });
        div()
            .relative()
            .size_full()
            .child(self.patch_view(cx))
            .children(panel)
    }
}

// The side a file's patch opens on: what is not staged yet, else what is.
fn side_of(changed: &Changed) -> Side {
    if changed.stage.unstaged() || !changed.stage.staged() {
        Side::Unstaged
    } else {
        Side::Staged
    }
}

// The lines a patch adds and takes out.
fn count(patch: &Patch) -> (usize, usize) {
    patch
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .fold((0, 0), |(a, r), line| match line.as_bytes().first() {
            Some(b'+') => (a + 1, r),
            Some(b'-') => (a, r + 1),
            _ => (a, r),
        })
}

// Whether a line of the patch is among those picked: lines on one side between the first and
// the last picked, or, for a pick from a line taken out to a line added, the ones taken out from
// the first and the ones added up to the last.
fn within(range: &DiffLineRange, new_side: bool, line: usize) -> bool {
    let (start, end) = (range.start(), range.end());
    let side = |s: DiffSide| s == DiffSide::Modified;
    match (side(range.side()), side(range.end_side())) {
        (a, b) if a == b => new_side == a && (start..=end).contains(&line),
        (false, true) => {
            if new_side {
                line <= end
            } else {
                line >= start
            }
        }
        _ => {
            if new_side {
                line >= start
            } else {
                line <= end
            }
        }
    }
}
