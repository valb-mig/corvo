use std::path::PathBuf;

use corvo_domain::{FileStat, Op, Stash, totals};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::diff::Diff;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, div, prelude::FluentBuilder as _, px,
};

use super::Ctx;
use super::parts::{PatchView, ago_text, ask_name, group, lines, note, pill, stat_row};
use crate::palette::{Offer, finds};

// The work put aside in the repository shown: each stash with what it was saved as, where and
// when, and the one picked opened with its files and patch, to apply, pop, drop or turn into a
// branch. Stash changes puts aside what changed now, with a message.
pub struct Stashes {
    ctx: Ctx,
    root: Option<PathBuf>,
    stashes: Vec<Stash>,
    picked: Option<usize>,
    files: Vec<FileStat>,
    file: Option<String>,
    patch: Entity<PatchView>,
    // The form that stashes what changed, while open.
    saving: bool,
    message: Entity<InputState>,
    untracked: bool,
    keep_index: bool,
    _reading: Option<Task<()>>,
    _opening: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl Stashes {
    pub fn new(ctx: Ctx, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let patch = cx.new(PatchView::new);
        let message = cx.new(|cx| InputState::new(window, cx).placeholder("Message (optional)"));
        let subs = vec![cx.observe(&patch, |_, _, cx| cx.notify())];
        Self {
            ctx,
            root: None,
            stashes: Vec::new(),
            picked: None,
            files: Vec::new(),
            file: None,
            patch,
            saving: false,
            message,
            untracked: true,
            keep_index: false,
            _reading: None,
            _opening: None,
            _subs: subs,
        }
    }

    pub fn show(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        if root != self.root {
            self.picked = None;
            self.stashes.clear();
        }
        self.root = root;
        let Some(root) = self.root.clone() else {
            return;
        };
        let git = self.ctx.repos.read(cx).git();
        let task = cx
            .background_executor()
            .spawn(async move { git.stashes(&root) });
        self._reading = Some(cx.spawn(async move |this, cx| {
            let stashes = task.await.unwrap_or_default();
            this.update(cx, |this, cx| {
                // A stash goes by its place, which moves as others come and go: the pick
                // follows its hash.
                let hash = this
                    .picked
                    .and_then(|ix| this.stashes.get(ix))
                    .map(|s| s.hash.clone());
                this.stashes = stashes;
                let at = hash
                    .and_then(|hash| this.stashes.iter().position(|s| s.hash == hash))
                    .or((!this.stashes.is_empty()).then_some(0));
                this.picked = None;
                match at {
                    Some(ix) => this.pick(ix, cx),
                    None => {
                        this.files.clear();
                        this.patch.update(cx, |patch, cx| patch.clear(cx));
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn pick(&mut self, ix: usize, cx: &mut Context<Self>) {
        let (Some(root), Some(stash)) = (self.root.clone(), self.stashes.get(ix)) else {
            return;
        };
        if self.picked == Some(ix) {
            return;
        }
        self.picked = Some(ix);
        self.file = None;
        let index = stash.index;
        let git = self.ctx.repos.read(cx).git();
        let (patcher, at) = (git.clone(), root.clone());
        self.patch.update(cx, |patch, cx| {
            patch.load(move || patcher.stash_patch(&at, index), cx)
        });
        let task = cx
            .background_executor()
            .spawn(async move { git.stash_files(&root, index) });
        self._opening = Some(cx.spawn(async move |this, cx| {
            let files = task.await.unwrap_or_default();
            this.update(cx, |this, cx| {
                this.files = files;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn pick_file(&mut self, path: String, cx: &mut Context<Self>) {
        let state = self.patch.read(cx).state().clone();
        state.update(cx, |diff, cx| diff.scroll_to_file(&path, cx));
        self.file = Some(path);
        cx.notify();
    }

    fn toggle_saving(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.saving = !self.saving;
        if self.saving {
            self.message.update(cx, |m, cx| m.focus(window, cx));
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let message = self.message.read(cx).value().to_string();
        self.message.update(cx, |m, cx| m.set_value("", window, cx));
        self.saving = false;
        self.ctx.act(
            Op::Stash {
                message,
                untracked: self.untracked,
                keep_index: self.keep_index,
            },
            window,
            cx,
        );
        cx.notify();
    }

    pub fn offers(&self, query: &str, _: &App) -> Vec<Offer> {
        self.stashes
            .iter()
            .enumerate()
            .filter(|(_, s)| finds(query, &format!("{} {}", s.message, s.branch)))
            .map(|(ix, stash)| {
                let view = self.ctx.window.clone();
                Offer::new(Glyph::Archive, stash.message.clone(), move |_, cx| {
                    view.update(cx, |git, cx| git.pick_stash(ix, cx)).ok();
                })
                .detail(format!("{} · {}", stash.name(), stash.branch))
            })
            .collect()
    }

    pub fn pick_stash(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.picked = None;
        self.pick(ix, cx);
    }

    fn form(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let changed = self
            .root
            .as_deref()
            .and_then(|root| self.ctx.repo(root, cx))
            .map_or(0, |repo| repo.changes().len());
        let (cancel, save, untracked, keep) = (
            cx.entity().downgrade(),
            cx.entity().downgrade(),
            cx.entity().downgrade(),
            cx.entity().downgrade(),
        );
        v_flex()
            .m_2p5()
            .p_2p5()
            .gap_2()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary.opacity(0.4))
            .child(Input::new(&self.message).small())
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        Checkbox::new("git-stash-untracked")
                            .label("Include untracked")
                            .small()
                            .checked(self.untracked)
                            .on_click(move |on, _, cx| {
                                let on = *on;
                                untracked
                                    .update(cx, |this, cx| {
                                        this.untracked = on;
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    )
                    .child(
                        Checkbox::new("git-stash-keep")
                            .label("Keep staged")
                            .small()
                            .checked(self.keep_index)
                            .on_click(move |on, _, cx| {
                                let on = *on;
                                keep.update(cx, |this, cx| {
                                    this.keep_index = on;
                                    cx.notify();
                                })
                                .ok();
                            }),
                    ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_1p5()
                    .child(
                        Button::new("git-stash-cancel")
                            .ghost()
                            .small()
                            .label("Cancel")
                            .on_click(move |_, window, cx| {
                                cancel
                                    .update(cx, |this, cx| this.toggle_saving(window, cx))
                                    .ok();
                            }),
                    )
                    .child(
                        Button::new("git-stash-save")
                            .primary()
                            .small()
                            .icon(Glyph::Archive)
                            .label(match changed {
                                1 => "Stash 1 file".to_owned(),
                                n => format!("Stash {n} files"),
                            })
                            .on_click(move |_, window, cx| {
                                save.update(cx, |this, cx| this.save(window, cx)).ok();
                            }),
                    ),
            )
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        if self.stashes.is_empty() {
            return div()
                .px_4()
                .py_3()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Nothing stashed.")
                .into_any_element();
        }
        let mut list = v_flex().gap_0p5().py_1();
        for (ix, stash) in self.stashes.iter().enumerate() {
            let on = self.picked == Some(ix);
            let view = cx.entity().downgrade();
            list = list.child(
                v_flex()
                    .id(("git-stash", ix))
                    .mx_1p5()
                    .px_2()
                    .py_2()
                    .gap_1()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|r| r.bg(theme.muted))
                    .when(on, |r| r.bg(theme.secondary))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.pick(ix, cx)).ok();
                    })
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .child(stash.message.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_family("monospace")
                                    .text_color(theme.muted_foreground.opacity(0.7))
                                    .child(stash.name()),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .when(!stash.branch.is_empty(), |m| {
                                m.child(pill(&stash.branch, theme.blue, false, cx))
                            })
                            .child(ago_text(stash.when)),
                    ),
            );
        }
        list.into_any_element()
    }

    fn detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(stash) = self.picked.and_then(|ix| self.stashes.get(ix)).cloned() else {
            return note("Pick a stash.", cx);
        };
        let ctx = self.ctx.clone();
        let act = |op: Op| {
            let ctx = ctx.clone();
            move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
                ctx.act(op.clone(), window, cx)
            }
        };
        let index = stash.index;
        let branch_ctx = ctx.clone();
        let files: Vec<AnyElement> = self
            .files
            .iter()
            .enumerate()
            .map(|(ix, file)| {
                let on = self.file.as_ref() == Some(&file.path);
                let (view, path) = (cx.entity().downgrade(), file.path.clone());
                stat_row(("git-stash-file", ix), file, on, cx)
                    .on_click(move |_, _, cx| {
                        let path = path.clone();
                        view.update(cx, |this, cx| this.pick_file(path, cx)).ok();
                    })
                    .into_any_element()
            })
            .collect();
        let (added, removed) = totals(&self.files);
        let patch = self.patch.read(cx);
        let diff = match patch.trouble() {
            Some(why) => note(format!("The patch did not read: {why}"), cx),
            None => Diff::new(patch.state()).into_any_element(),
        };
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .truncate()
                                    .text_base()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(stash.message.clone()),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(div().font_family("monospace").child(stash.name()))
                                    .when(!stash.branch.is_empty(), |m| {
                                        m.child(format!("on {}", stash.branch))
                                    })
                                    .child(ago_text(stash.when)),
                            ),
                    )
                    .child(
                        Button::new("git-stash-branch")
                            .outline()
                            .small()
                            .icon(Glyph::GitBranchPlus)
                            .label("Branch…")
                            .tooltip("A new branch where it was saved, with it applied")
                            .on_click(move |_, window, cx| {
                                let ctx = branch_ctx.clone();
                                ask_name(
                                    "Branch from the stash",
                                    "Name of the branch",
                                    "",
                                    "Make",
                                    window,
                                    cx,
                                    move |name, window, cx| {
                                        ctx.act(Op::StashBranch { name, index }, window, cx)
                                    },
                                );
                            }),
                    )
                    .child(
                        Button::new("git-stash-drop")
                            .outline()
                            .small()
                            .icon(Glyph::Trash)
                            .label("Drop")
                            .on_click(act(Op::StashDrop(index))),
                    )
                    .child(
                        Button::new("git-stash-apply")
                            .outline()
                            .small()
                            .label("Apply")
                            .tooltip("Bring it back and keep it")
                            .on_click(act(Op::StashApply(index))),
                    )
                    .child(
                        Button::new("git-stash-pop")
                            .primary()
                            .small()
                            .label("Pop")
                            .tooltip("Bring it back and drop it")
                            .on_click(act(Op::StashPop(index))),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(
                        v_flex()
                            .id("git-stash-files")
                            .w(px(250.))
                            .h_full()
                            .flex_shrink_0()
                            .overflow_y_scroll()
                            .py_1p5()
                            .border_r_1()
                            .border_color(theme.border)
                            .child(
                                group("Files", self.files.len(), cx)
                                    .child(lines(added, removed, cx)),
                            )
                            .children(files),
                    )
                    .child(div().flex_1().min_w_0().h_full().child(diff)),
            )
            .into_any_element()
    }
}

impl Render for Stashes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let view = cx.entity().downgrade();
        let saving = self.saving;
        let form = saving.then(|| self.form(cx));
        let list = self.list(cx);
        let detail = self.detail(cx);
        h_flex()
            .size_full()
            .child(
                v_flex()
                    .w(px(380.))
                    .h_full()
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(border)
                    .child(
                        h_flex()
                            .h(px(40.))
                            .px_3()
                            .gap_2()
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(border)
                            .child(div().font_weight(FontWeight::MEDIUM).child("Stash"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(self.stashes.len().to_string()),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("git-stash-open")
                                    .outline()
                                    .small()
                                    .icon(Glyph::Archive)
                                    .label("Stash changes")
                                    .selected(saving)
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| this.toggle_saving(window, cx))
                                            .ok();
                                    }),
                            ),
                    )
                    .children(form)
                    .child(
                        div()
                            .id("git-stash-list")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(list),
                    ),
            )
            .child(div().flex_1().min_w_0().h_full().child(detail))
    }
}
