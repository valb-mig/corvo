use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

use corvo_domain::{Commit, Detail, Holders, Log, LogScope, Op, RefName, Reset, Row, graph};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::Button;
use gpui_kit::component::diff::Diff;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Bounds, ClipboardItem, Context, ElementId, Entity,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, PathBuilder,
    Pixels, Point, Render, StatefulInteractiveElement as _, Styled as _, Subscription, Task,
    UniformListScrollHandle, Window, canvas, div, fill, point, prelude::FluentBuilder as _, px,
    size, uniform_list,
};

use super::parts::{
    self, PatchView, ago_text, ask_name, badge, note, ref_pill, row, since, stat_row,
};
use super::{Ctx, short};
use crate::palette::{Offer, finds};

// The height of a commit's row, and the room each lane of the graph takes.
const ROW: f32 = 28.;
const LANE: f32 = 14.;
// How long typing in the filter waits before looking again.
const SETTLE: Duration = Duration::from_millis(250);

// The history of the repository shown: its commits newest first, with the graph of the branches
// beside them, filtered by a text or by a branch, or the reflog; and the commit picked opened on
// the right, with its message, where it is, what can be done to it, its files and their patch.
pub struct History {
    ctx: Ctx,
    root: Option<PathBuf>,
    scope: LogScope,
    filter: Entity<InputState>,
    log: Log,
    commits: Vec<Commit>,
    rows: Vec<Row>,
    // The last page came full: there may be more.
    more: bool,
    loading: bool,
    branches: Vec<String>,
    picked: Option<String>,
    detail: Option<Detail>,
    holders: Option<Holders>,
    remotes: bool,
    body_open: bool,
    file: Option<String>,
    patch: Entity<PatchView>,
    scroll: UniformListScrollHandle,
    _reading: Option<Task<()>>,
    _opening: Option<Task<()>>,
    _typing: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl History {
    pub fn new(ctx: Ctx, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Message, hash, author or path"));
        let patch = cx.new(PatchView::new);
        let subs = vec![
            cx.observe(&patch, |_, _, cx| cx.notify()),
            cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.typed(cx);
                }
            }),
        ];
        Self {
            ctx,
            root: None,
            scope: LogScope::All,
            filter,
            log: Log::new(LogScope::All, ""),
            commits: Vec::new(),
            rows: Vec::new(),
            more: false,
            loading: false,
            branches: Vec::new(),
            picked: None,
            detail: None,
            holders: None,
            remotes: false,
            body_open: false,
            file: None,
            patch,
            scroll: UniformListScrollHandle::new(),
            _reading: None,
            _opening: None,
            _typing: None,
            _subs: subs,
        }
    }

    pub fn show(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        if root != self.root {
            self.picked = None;
            self.detail = None;
            self.commits.clear();
            self.rows.clear();
            self.scope = LogScope::All;
        }
        self.root = root;
        self.read(false, cx);
    }

    fn typed(&mut self, cx: &mut Context<Self>) {
        self._typing = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |this, cx| this.read(false, cx)).ok();
        }));
    }

    fn set_scope(&mut self, scope: LogScope, cx: &mut Context<Self>) {
        self.scope = scope;
        self.read(false, cx);
    }

    // Reads the first page again, or the next one after those listed.
    fn read(&mut self, more: bool, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let text = self.filter.read(cx).value().to_string();
        let log = if more {
            self.log.next_page()
        } else {
            Log::new(self.scope.clone(), text)
        };
        self.loading = true;
        let git = self.ctx.repos.read(cx).git();
        let asked = log.clone();
        let task = cx.background_executor().spawn(async move {
            let commits = git.log(&root, &asked);
            let refs = if more { None } else { git.refs(&root).ok() };
            (commits, refs)
        });
        self._reading = Some(cx.spawn(async move |this, cx| {
            let (commits, refs) = task.await;
            this.update(cx, |this, cx| {
                this.loading = false;
                let commits = commits.unwrap_or_default();
                this.more = commits.len() == log.limit;
                if more {
                    this.commits.extend(commits);
                } else {
                    this.commits = commits;
                }
                if let Some(refs) = refs {
                    this.branches = refs.local.iter().map(|b| b.name.clone()).collect();
                    this.remotes = !refs.remote.is_empty();
                }
                this.rows = if log.draws_graph() {
                    graph(&this.commits)
                } else {
                    Vec::new()
                };
                this.log = log;
                let still = this
                    .picked
                    .as_ref()
                    .is_some_and(|hash| this.commits.iter().any(|c| c.hash == *hash));
                if !still {
                    match this.commits.first().map(|c| c.hash.clone()) {
                        Some(hash) => this.pick(hash, cx),
                        None => {
                            this.picked = None;
                            this.detail = None;
                            this.patch.update(cx, |patch, cx| patch.clear(cx));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn pick(&mut self, hash: String, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.picked.as_ref() == Some(&hash) && self.detail.is_some() {
            return;
        }
        self.picked = Some(hash.clone());
        self.body_open = false;
        self.file = None;
        let git = self.ctx.repos.read(cx).git();
        let (shown, at) = (git.clone(), root.clone());
        let rev = hash.clone();
        self.patch.update(cx, |patch, cx| {
            patch.load(move || shown.show(&at, &rev), cx)
        });
        let task = cx
            .background_executor()
            .spawn(async move { (git.detail(&root, &hash), git.holders(&root, &hash)) });
        self._opening = Some(cx.spawn(async move |this, cx| {
            let (detail, holders) = task.await;
            this.update(cx, |this, cx| {
                this.detail = detail.ok();
                this.holders = holders.ok();
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

    pub fn offers(&self, query: &str, _: &App) -> Vec<Offer> {
        self.commits
            .iter()
            .filter(|c| finds(query, &format!("{} {} {}", c.subject, c.hash, c.author)))
            .take(200)
            .map(|commit| {
                let (view, hash) = (self.ctx.window.clone(), commit.hash.clone());
                let history = view.clone();
                Offer::new(
                    Glyph::GitCommitHorizontal,
                    commit.subject.clone(),
                    move |_, cx| {
                        let hash = hash.clone();
                        history.update(cx, |git, cx| git.pick_commit(hash, cx)).ok();
                    },
                )
                .detail(format!("{} · {}", commit.short(), commit.author))
            })
            .collect()
    }

    pub fn pick_commit(&mut self, hash: String, cx: &mut Context<Self>) {
        if let Some(ix) = self.commits.iter().position(|c| c.hash == hash) {
            self.scroll
                .scroll_to_item(ix, gpui_kit::ScrollStrategy::Center);
        }
        self.pick(hash, cx);
    }

    fn head(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let view = cx.entity().downgrade();
        let scope_label = match &self.scope {
            LogScope::All => "All branches".to_owned(),
            LogScope::Rev(rev) => rev.clone(),
            LogScope::Reflog => "Reflog".to_owned(),
        };
        let branches = self.branches.clone();
        let scoped = view.clone();
        let scope = Button::new("git-scope")
            .outline()
            .small()
            .icon(Glyph::GitBranch)
            .label(scope_label)
            .dropdown_caret(true)
            .dropdown_menu(move |menu: PopupMenu, _, _| {
                let all = scoped.clone();
                let mut menu = menu.item(PopupMenuItem::new("All branches").on_click(
                    move |_, _, cx| {
                        all.update(cx, |this, cx| this.set_scope(LogScope::All, cx))
                            .ok();
                    },
                ));
                menu = menu.separator();
                for branch in &branches {
                    let (view, name) = (scoped.clone(), branch.clone());
                    menu = menu.item(PopupMenuItem::new(branch.clone()).on_click(
                        move |_, _, cx| {
                            let name = name.clone();
                            view.update(cx, |this, cx| this.set_scope(LogScope::Rev(name), cx))
                                .ok();
                        },
                    ));
                }
                menu
            });
        let reflog = self.scope == LogScope::Reflog;
        let flip = view.clone();
        h_flex()
            .h(px(40.))
            .px_2()
            .gap_1p5()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .child(scope)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.filter).small().cleanable(true)),
            )
            .child(
                Button::new("git-reflog")
                    .outline()
                    .small()
                    .icon(Glyph::Undo2)
                    .label("Reflog")
                    .selected(reflog)
                    .tooltip("Where HEAD has been: the way back after a mistake")
                    .on_click(move |_, _, cx| {
                        flip.update(cx, |this, cx| {
                            let scope = if reflog {
                                LogScope::All
                            } else {
                                LogScope::Reflog
                            };
                            this.set_scope(scope, cx);
                        })
                        .ok();
                    }),
            )
            .into_any_element()
    }

    fn commit_row(&self, ix: usize, lanes: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let commit = &self.commits[ix];
        let on = self.picked.as_ref() == Some(&commit.hash);
        let view = cx.entity().downgrade();
        let hash = commit.hash.clone();
        let graph = self.rows.get(ix).cloned().map(|row| {
            let head = commit.refs.iter().any(|r| matches!(r, RefName::Head(_)));
            let colors = palette(cx);
            div()
                .w(px(lanes as f32 * LANE + 10.))
                .h_full()
                .flex_shrink_0()
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| paint_row(&row, head, &colors, bounds, window),
                    )
                    .size_full(),
                )
        });
        let refs: Vec<AnyElement> = commit.refs.iter().map(|r| ref_pill(r, cx)).collect();
        let note = commit.note.clone();
        let item = row(("git-commit", ix), on, cx)
            .h(px(ROW))
            .w_full()
            .mx_0()
            .when(graph.is_none(), |r| r.pl_3())
            .when(graph.is_some(), |r| r.pl_0())
            .on_click(move |_, _, cx| {
                let hash = hash.clone();
                view.update(cx, |this, cx| this.pick(hash, cx)).ok();
            })
            .children(graph)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(commit.subject.clone()),
            )
            .children(refs)
            .when_some(note, |r, note| {
                r.child(
                    div()
                        .max_w(px(160.))
                        .truncate()
                        .text_xs()
                        .text_color(theme.warning)
                        .child(note),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.5))
                    .font_family("monospace")
                    .text_color(theme.muted_foreground.opacity(0.7))
                    .child(commit.short().to_owned()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(30.))
                    .text_xs()
                    .text_right()
                    .text_color(theme.muted_foreground)
                    .child(since(commit.when)),
            );
        div().w_full().px_1().child(item).into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        if self.commits.is_empty() {
            return note(
                if self.loading {
                    "Reading the history…"
                } else {
                    "No commit here."
                },
                cx,
            );
        }
        let lanes = self.rows.iter().map(Row::width).max().unwrap_or(0).min(12);
        let count = self.commits.len() + usize::from(self.more);
        let list = uniform_list(
            "git-history",
            count,
            cx.processor(move |this, range: Range<usize>, _, cx| {
                range
                    .map(|ix| {
                        if ix < this.commits.len() {
                            this.commit_row(ix, lanes, cx)
                        } else {
                            this.more_row(cx)
                        }
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .text_sm()
            .text_color(theme.foreground)
            .child(list)
            .into_any_element()
    }

    // The last row, reading the next page when clicked.
    fn more_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity().downgrade();
        row("git-more", false, cx)
            .h(px(ROW))
            .w_full()
            .mx_0()
            .justify_center()
            .text_xs()
            .text_color(cx.theme().blue)
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| this.read(true, cx)).ok();
            })
            .child(if self.loading {
                "Reading…"
            } else {
                "Older commits"
            })
            .into_any_element()
    }

    fn detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(detail) = &self.detail else {
            return note(
                if self.picked.is_some() {
                    "Opening the commit…"
                } else {
                    "Pick a commit."
                },
                cx,
            );
        };
        let commit = &detail.commit;
        let hash = commit.hash.clone();
        let view = cx.entity().downgrade();
        let ctx = self.ctx.clone();
        let act = |op: Op| {
            let ctx = ctx.clone();
            move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
                ctx.act(op.clone(), window, cx)
            }
        };
        let body = (!detail.body.is_empty()).then(|| {
            let lines = detail.body.lines().count();
            let open = self.body_open;
            let flip = view.clone();
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .line_height(px(19.))
                        .text_color(theme.muted_foreground)
                        .when(!open, |b| b.max_h(px(38.)).overflow_hidden())
                        .child(detail.body.clone()),
                )
                .when(lines > 2 || detail.body.len() > 160, |b| {
                    b.child(
                        div()
                            .id("git-body")
                            .text_xs()
                            .text_color(theme.blue)
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                flip.update(cx, |this, cx| {
                                    this.body_open = !this.body_open;
                                    cx.notify();
                                })
                                .ok();
                            })
                            .child(if open { "Show less" } else { "Show all" }),
                    )
                })
        });
        let copy = hash.clone();
        let parents: Vec<AnyElement> = commit
            .parents
            .iter()
            .map(|parent| {
                let (view, parent) = (view.clone(), parent.clone());
                div()
                    .id(ElementId::Name(format!("parent:{parent}").into()))
                    .font_family("monospace")
                    .text_color(theme.blue)
                    .cursor_pointer()
                    .child(short(&parent).to_owned())
                    .on_click(move |_, _, cx| {
                        let parent = parent.clone();
                        view.update(cx, |this, cx| this.pick_commit(parent, cx))
                            .ok();
                    })
                    .into_any_element()
            })
            .collect();
        let holders: Vec<AnyElement> = self.holders.as_ref().map_or(Vec::new(), |h| {
            let mut pills: Vec<AnyElement> = h
                .local
                .iter()
                .take(4)
                .map(|name| ref_pill(&RefName::Local(name.clone()), cx))
                .collect();
            if h.local.len() > 4 {
                pills.push(
                    div()
                        .child(format!("+{}", h.local.len() - 4))
                        .into_any_element(),
                );
            }
            if h.remote.is_empty() && self.remotes {
                pills.push(badge("not on a remote yet", theme.warning).into_any_element());
            }
            pills
        });
        let when = commit.when;
        let (cherry, revert, checkout) = (hash.clone(), hash.clone(), hash.clone());
        let (branch_at, tag_at, ctx_b, ctx_t) =
            (hash.clone(), hash.clone(), ctx.clone(), ctx.clone());
        let resets = hash.clone();
        let menu_ctx = ctx.clone();
        let more = Button::new("git-commit-more")
            .outline()
            .small()
            .icon(Glyph::Ellipsis)
            .dropdown_menu(move |menu: PopupMenu, _, _| {
                let reset = |how: Reset, label: &'static str| {
                    let (ctx, rev) = (menu_ctx.clone(), resets.clone());
                    PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        ctx.act(
                            Op::ResetTo {
                                rev: rev.clone(),
                                how,
                            },
                            window,
                            cx,
                        )
                    })
                };
                let copy = copy.clone();
                menu.item(PopupMenuItem::new("Copy hash").icon(Glyph::Copy).on_click(
                    move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                    },
                ))
                .separator()
                .label("Reset the branch here")
                .item(reset(Reset::Soft, "Soft: keep the changes staged"))
                .item(reset(Reset::Mixed, "Mixed: keep the changes, unstaged"))
                .item(reset(Reset::Hard, "Hard: lose the changes…"))
            });
        let files: Vec<AnyElement> = detail
            .files
            .iter()
            .enumerate()
            .map(|(ix, file)| {
                let on = self.file.as_ref() == Some(&file.path);
                let (view, path) = (view.clone(), file.path.clone());
                stat_row(("git-commit-file", ix), file, on, cx)
                    .on_click(move |_, _, cx| {
                        let path = path.clone();
                        view.update(cx, |this, cx| this.pick_file(path, cx)).ok();
                    })
                    .into_any_element()
            })
            .collect();
        let (added, removed) = corvo_domain::totals(&detail.files);
        let patch = self.patch.read(cx);
        let diff = match patch.trouble() {
            Some(why) => note(format!("The patch did not read: {why}"), cx),
            None => Diff::new(patch.state()).into_any_element(),
        };
        v_flex()
            .size_full()
            .child(
                v_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .child(commit.subject.clone()),
                    )
                    .children(body)
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_3()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(
                                div()
                                    .font_family("monospace")
                                    .text_color(theme.foreground)
                                    .child(commit.short().to_owned()),
                            )
                            .child(format!("{} <{}>", commit.author, commit.email))
                            .child(ago_text(when))
                            .when(!parents.is_empty(), |m| {
                                m.child(h_flex().gap_1().child("parent").children(parents))
                            })
                            .when(!holders.is_empty(), |m| {
                                m.child(h_flex().gap_1().child("on").children(holders))
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                Button::new("git-branch-here")
                                    .outline()
                                    .small()
                                    .icon(Glyph::GitBranchPlus)
                                    .label("Branch here")
                                    .on_click(move |_, window, cx| {
                                        let (ctx, at) = (ctx_b.clone(), branch_at.clone());
                                        ask_name(
                                            "New branch",
                                            "Name of the branch",
                                            "",
                                            "Make",
                                            window,
                                            cx,
                                            move |name, window, cx| {
                                                ctx.act(
                                                    Op::Branch {
                                                        name,
                                                        from: at.clone(),
                                                        checkout: false,
                                                    },
                                                    window,
                                                    cx,
                                                )
                                            },
                                        );
                                    }),
                            )
                            .child(
                                Button::new("git-tag-here")
                                    .outline()
                                    .small()
                                    .icon(Glyph::Tag)
                                    .label("Tag")
                                    .on_click(move |_, window, cx| {
                                        let (ctx, at) = (ctx_t.clone(), tag_at.clone());
                                        ask_name(
                                            "New tag",
                                            "v1.0.0",
                                            "",
                                            "Tag",
                                            window,
                                            cx,
                                            move |name, window, cx| {
                                                ctx.act(
                                                    Op::Tag {
                                                        name,
                                                        at: at.clone(),
                                                    },
                                                    window,
                                                    cx,
                                                )
                                            },
                                        );
                                    }),
                            )
                            .child(
                                Button::new("git-cherry-pick")
                                    .outline()
                                    .small()
                                    .icon(Glyph::Cherry)
                                    .label("Cherry-pick")
                                    .on_click(act(Op::CherryPick(cherry))),
                            )
                            .child(
                                Button::new("git-revert")
                                    .outline()
                                    .small()
                                    .icon(Glyph::RotateCcw)
                                    .label("Revert")
                                    .on_click(act(Op::Revert(revert))),
                            )
                            .child(
                                Button::new("git-checkout-commit")
                                    .outline()
                                    .small()
                                    .icon(Glyph::ArrowRight)
                                    .label("Checkout")
                                    .tooltip("Detaches HEAD at this commit")
                                    .on_click(act(Op::Checkout(checkout))),
                            )
                            .child(more),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(
                        v_flex()
                            .id("git-commit-files")
                            .w(px(250.))
                            .h_full()
                            .flex_shrink_0()
                            .overflow_y_scroll()
                            .py_1p5()
                            .border_r_1()
                            .border_color(theme.border)
                            .child(
                                parts::group("Files", detail.files.len(), cx)
                                    .child(parts::lines(added, removed, cx)),
                            )
                            .children(files),
                    )
                    .child(div().flex_1().min_w_0().h_full().child(diff)),
            )
            .into_any_element()
    }
}

impl Render for History {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .size_full()
            .child(
                v_flex()
                    .w(px(560.))
                    .h_full()
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(theme.border)
                    .child(self.head(cx))
                    .child(self.list(cx)),
            )
            .child(div().flex_1().min_w_0().h_full().child(self.detail(cx)))
    }
}

// The colors the lanes of the graph go round.
fn palette(cx: &App) -> [Hsla; 4] {
    let theme = cx.theme();
    [
        theme.blue,
        gpui_kit::rgb(0xc678dd).into(),
        theme.warning,
        theme.green,
    ]
}

// One row of the graph: the lanes running past, those coming down into the commit, those leaving
// it, and its dot; HEAD's a ring.
fn paint_row(
    row: &Row,
    head: bool,
    colors: &[Hsla; 4],
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let x = |lane: usize| bounds.origin.x + px(7. + lane as f32 * LANE);
    let (top, bottom) = (bounds.origin.y, bounds.origin.y + bounds.size.height);
    let mid = bounds.origin.y + bounds.size.height / 2.;
    let color = |lane: usize| colors[lane % colors.len()];
    let line = |from: Point<Pixels>, to: Point<Pixels>, color: Hsla, window: &mut Window| {
        let mut path = PathBuilder::stroke(px(1.6));
        path.move_to(from);
        if from.x == to.x {
            path.line_to(to);
        } else {
            let half = (to.y - from.y) / 2.;
            path.cubic_bezier_to(to, point(from.x, from.y + half), point(to.x, to.y - half));
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    };
    for lane in &row.through {
        line(
            point(x(*lane), top),
            point(x(*lane), bottom),
            color(*lane),
            window,
        );
    }
    for lane in &row.into {
        line(
            point(x(*lane), top),
            point(x(row.lane), mid),
            color(*lane),
            window,
        );
    }
    for lane in &row.out {
        line(
            point(x(row.lane), mid),
            point(x(*lane), bottom),
            color(*lane),
            window,
        );
    }
    let dot = color(row.lane);
    let center = point(x(row.lane), mid);
    let r = if head { 5. } else { 4. };
    window.paint_quad(
        fill(
            Bounds::centered_at(center, size(px(r * 2.), px(r * 2.))),
            dot,
        )
        .corner_radii(px(r)),
    );
    if head {
        window.paint_quad(
            fill(
                Bounds::centered_at(center, size(px(5.), px(5.))),
                gpui_kit::black(),
            )
            .corner_radii(px(2.5)),
        );
    }
}
