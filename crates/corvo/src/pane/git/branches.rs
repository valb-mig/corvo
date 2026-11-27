use std::path::PathBuf;

use corvo_domain::{Branch, Op, Refs, Tag};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, div, prelude::FluentBuilder as _, px,
};

use super::compare::Compare;
use super::parts::{apart, ask_name, badge, group, note, pill, row};
use super::{Ctx, Tab};
use crate::palette::{Offer, finds};

// What is picked on the list.
#[derive(Clone, PartialEq, Eq)]
enum Pick {
    Branch(String),
    Tag(String),
}

// The branches of the repository shown, here and on its remotes, and its tags; the branch picked
// set against a base, the branch checked out by default, as a pull request would show it.
pub struct Branches {
    ctx: Ctx,
    root: Option<PathBuf>,
    refs: Option<Refs>,
    filter: Entity<InputState>,
    picked: Option<Pick>,
    // What the branch picked is set against; the branch checked out when none was chosen.
    base: Option<String>,
    compare: Entity<Compare>,
    _reading: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl Branches {
    pub fn new(ctx: Ctx, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a branch or tag"));
        let compare = cx.new(|cx| Compare::new(ctx.clone(), cx));
        let subs = vec![
            cx.observe(&compare, |_, _, cx| cx.notify()),
            cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            ctx,
            root: None,
            refs: None,
            filter,
            picked: None,
            base: None,
            compare,
            _reading: None,
            _subs: subs,
        }
    }

    pub fn show(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        if root != self.root {
            self.refs = None;
            self.picked = None;
            self.base = None;
        }
        self.root = root;
        self.read(cx);
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let git = self.ctx.repos.read(cx).git();
        let task = cx
            .background_executor()
            .spawn(async move { git.refs(&root) });
        self._reading = Some(cx.spawn(async move |this, cx| {
            let refs = task.await;
            this.update(cx, |this, cx| {
                this.refs = refs.ok();
                let still = match &this.picked {
                    Some(Pick::Branch(name)) => this.branch(name).is_some(),
                    Some(Pick::Tag(name)) => this.tag(name).is_some(),
                    None => false,
                };
                if !still {
                    // The first branch that is not the one checked out, else that one.
                    let first = this.refs.as_ref().and_then(|refs| {
                        refs.local
                            .iter()
                            .find(|b| !b.head)
                            .or(refs.local.first())
                            .map(|b| b.name.clone())
                    });
                    this.picked = first.map(Pick::Branch);
                }
                this.set_compare(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    fn branch(&self, name: &str) -> Option<&Branch> {
        self.refs.as_ref()?.branch(name)
    }

    fn tag(&self, name: &str) -> Option<&Tag> {
        self.refs.as_ref()?.tags.iter().find(|t| t.name == name)
    }

    fn current(&self) -> Option<&Branch> {
        self.refs.as_ref()?.local.iter().find(|b| b.head)
    }

    // The base the pick is set against: the one chosen, else the branch checked out, else the
    // main one; for the branch checked out itself, its upstream or the main one.
    fn base_of(&self, picked: &str) -> Option<String> {
        if let Some(base) = &self.base
            && base != picked
        {
            return Some(base.clone());
        }
        let refs = self.refs.as_ref()?;
        let current = self.current().map(|b| b.name.clone());
        if current.as_deref() != Some(picked) {
            return current.or_else(|| refs.main.clone());
        }
        let branch = self.branch(picked)?;
        branch
            .upstream
            .clone()
            .or_else(|| refs.main.clone().filter(|main| main != picked))
    }

    fn set_compare(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let head = match &self.picked {
            Some(Pick::Branch(name)) => name.clone(),
            Some(Pick::Tag(name)) => name.clone(),
            None => {
                self.compare.update(cx, |c, cx| c.clear(cx));
                return;
            }
        };
        match self.base_of(&head) {
            Some(base) => self.compare.update(cx, |c, cx| c.set(root, base, head, cx)),
            None => self.compare.update(cx, |c, cx| c.clear(cx)),
        }
    }

    fn pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        if self.picked.as_ref() != Some(&pick) {
            self.picked = Some(pick);
            self.set_compare(cx);
            cx.notify();
        }
    }

    pub fn pick_branch(&mut self, name: String, cx: &mut Context<Self>) {
        self.pick(Pick::Branch(name), cx);
    }

    fn set_base(&mut self, base: String, cx: &mut Context<Self>) {
        self.base = Some(base);
        self.set_compare(cx);
        cx.notify();
    }

    pub fn offers(&self, query: &str, _: &App) -> Vec<Offer> {
        let Some(refs) = &self.refs else {
            return Vec::new();
        };
        let mut offers = Vec::new();
        for branch in refs.local.iter().chain(&refs.remote) {
            if !finds(query, &branch.name) {
                continue;
            }
            let (view, name) = (self.ctx.window.clone(), branch.name.clone());
            offers.push(
                Offer::new(Glyph::GitBranch, branch.name.clone(), move |_, cx| {
                    let name = name.clone();
                    view.update(cx, |git, cx| git.pick_branch(name, cx)).ok();
                })
                .detail(branch.subject.clone()),
            );
        }
        offers
    }

    fn act(&self, op: Op, window: &mut Window, cx: &mut App) {
        self.ctx.act(op, window, cx);
    }

    fn branch_menu(&self, branch: &Branch, menu: PopupMenu) -> PopupMenu {
        let current = self.current().map(|b| b.name.clone());
        let name = branch.name.clone();
        let local = !branch.remote;
        let is_current = branch.head;
        let elsewhere = branch.worktree.is_some() && !branch.head;
        let ctx = self.ctx.clone();
        let item = |label: String, glyph: Glyph, op: Op| {
            let ctx = ctx.clone();
            PopupMenuItem::new(label)
                .icon(glyph)
                .on_click(move |_, window, cx| ctx.act(op.clone(), window, cx))
        };
        // A remote branch checks out as the local one tracking it.
        let checkout = if local {
            name.clone()
        } else {
            name.split_once('/')
                .map_or(name.clone(), |(_, n)| n.to_owned())
        };
        let mut menu = menu;
        if !is_current && !elsewhere {
            menu = menu.item(item(
                "Checkout".into(),
                Glyph::ArrowRight,
                Op::Checkout(checkout),
            ));
        }
        if let Some(current) = &current
            && !is_current
        {
            menu = menu
                .item(item(
                    format!("Merge into {current}"),
                    Glyph::GitMerge,
                    Op::Merge(name.clone()),
                ))
                .item(item(
                    format!("Rebase {current} onto it"),
                    Glyph::GitCompare,
                    Op::Rebase(name.clone()),
                ));
        }
        if is_current {
            menu = menu.item(item(
                if branch.upstream.is_some() {
                    "Push".into()
                } else {
                    "Publish to origin".into()
                },
                Glyph::ArrowUp,
                Op::Push {
                    upstream: branch.upstream.is_none().then(|| "origin".to_owned()),
                },
            ));
        }
        menu = menu.separator();
        let (from, ctx_new) = (name.clone(), ctx.clone());
        menu = menu.item(
            PopupMenuItem::new("New branch from here…")
                .icon(Glyph::GitBranchPlus)
                .on_click(move |_, window, cx| {
                    let (from, ctx) = (from.clone(), ctx_new.clone());
                    ask_name(
                        "New branch",
                        "Name of the branch",
                        "",
                        "Make",
                        window,
                        cx,
                        move |n, window, cx| {
                            ctx.act(
                                Op::Branch {
                                    name: n,
                                    from: from.clone(),
                                    checkout: true,
                                },
                                window,
                                cx,
                            )
                        },
                    );
                }),
        );
        if local {
            let (old, ctx_rename) = (name.clone(), ctx.clone());
            menu = menu.item(PopupMenuItem::new("Rename…").icon(Glyph::Pencil).on_click(
                move |_, window, cx| {
                    let (old, ctx) = (old.clone(), ctx_rename.clone());
                    ask_name(
                        "Rename the branch",
                        "New name",
                        old.clone(),
                        "Rename",
                        window,
                        cx,
                        move |to, window, cx| {
                            ctx.act(
                                Op::Rename {
                                    from: old.clone(),
                                    to,
                                },
                                window,
                                cx,
                            )
                        },
                    );
                },
            ));
        }
        let copy = name.clone();
        menu = menu.item(PopupMenuItem::new("Copy name").icon(Glyph::Copy).on_click(
            move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
            },
        ));
        if local && !is_current && !elsewhere {
            menu = menu.separator().item(item(
                "Delete…".into(),
                Glyph::Trash,
                Op::DeleteBranch {
                    name: name.clone(),
                    force: !branch.merged,
                },
            ));
        }
        menu
    }

    fn branch_row(&self, ix: usize, branch: &Branch, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let on = self.picked == Some(Pick::Branch(branch.name.clone()));
        let glyph = if branch.head {
            Glyph::Check
        } else if branch.worktree.is_some() {
            Glyph::FolderGit2
        } else if branch.remote {
            Glyph::Cloud
        } else {
            Glyph::GitBranch
        };
        let tone = if branch.head {
            theme.blue
        } else {
            theme.muted_foreground
        };
        let (ahead, behind) = match (&branch.upstream, branch.base) {
            (Some(_), _) => (branch.ahead, branch.behind),
            (None, Some(base)) => (base.ahead, base.behind),
            _ => (0, 0),
        };
        let agent = branch.name.starts_with("claude/")
            || branch.name.starts_with("codex/")
            || branch.name.starts_with("editr/");
        let view = cx.entity().downgrade();
        let name = branch.name.clone();
        let menu_branch = branch.clone();
        let menu_view = cx.entity().downgrade();
        row(("git-branch", ix), on, cx)
            .when(branch.merged, |r| r.opacity(0.6))
            .on_click(move |_, _, cx| {
                let name = name.clone();
                view.update(cx, |this, cx| this.pick(Pick::Branch(name), cx))
                    .ok();
            })
            .child(Icon::new(glyph).xsmall().text_color(tone))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family("monospace")
                    .text_size(px(12.5))
                    .child(branch.name.clone()),
            )
            .when(agent, |r| {
                r.child(badge("agent", gpui_kit::rgb(0xd97757).into()))
            })
            .when(branch.merged, |r| r.child(badge("merged", theme.green)))
            .when(branch.gone, |r| {
                r.child(badge("upstream gone", theme.warning))
            })
            .child(apart(ahead, behind, cx))
            .context_menu(move |menu, _, cx| match menu_view.upgrade() {
                Some(view) => view.read(cx).branch_menu(&menu_branch, menu),
                None => menu,
            })
            .into_any_element()
    }

    fn tag_row(&self, ix: usize, tag: &Tag, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let on = self.picked == Some(Pick::Tag(tag.name.clone()));
        let view = cx.entity().downgrade();
        let name = tag.name.clone();
        let (ctx, del, checkout, copy) = (
            self.ctx.clone(),
            tag.name.clone(),
            tag.name.clone(),
            tag.name.clone(),
        );
        row(("git-tag", ix), on, cx)
            .on_click(move |_, _, cx| {
                let name = name.clone();
                view.update(cx, |this, cx| this.pick(Pick::Tag(name), cx))
                    .ok();
            })
            .child(Icon::new(Glyph::Tag).xsmall().text_color(theme.warning))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family("monospace")
                    .text_size(px(12.5))
                    .child(tag.name.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .font_family("monospace")
                    .text_color(theme.muted_foreground)
                    .child(tag.hash[..tag.hash.len().min(7)].to_owned()),
            )
            .context_menu(move |menu, _, _| {
                let (a, b) = (ctx.clone(), ctx.clone());
                let (del, checkout, copy) = (del.clone(), checkout.clone(), copy.clone());
                menu.item(
                    PopupMenuItem::new("Checkout")
                        .icon(Glyph::ArrowRight)
                        .on_click(move |_, window, cx| {
                            a.act(Op::Checkout(checkout.clone()), window, cx)
                        }),
                )
                .item(PopupMenuItem::new("Copy name").icon(Glyph::Copy).on_click(
                    move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())),
                ))
                .separator()
                .item(
                    PopupMenuItem::new("Delete tag")
                        .icon(Glyph::Trash)
                        .on_click(move |_, window, cx| {
                            b.act(Op::DeleteTag(del.clone()), window, cx)
                        }),
                )
            })
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(refs) = self.refs.clone() else {
            return note("Reading the branches…", cx);
        };
        let query = self.filter.read(cx).value().to_string();
        let keep = |name: &str| finds(&query, name);
        let local: Vec<&Branch> = refs.local.iter().filter(|b| keep(&b.name)).collect();
        let remote: Vec<&Branch> = refs.remote.iter().filter(|b| keep(&b.name)).collect();
        let tags: Vec<&Tag> = refs.tags.iter().filter(|t| keep(&t.name)).collect();
        let mut list = v_flex().gap_0p5().py_1();
        list = list.child(group("Local", local.len(), cx));
        for (ix, branch) in local.iter().enumerate() {
            list = list.child(self.branch_row(ix, branch, cx));
        }
        if !remote.is_empty() {
            list = list.child(group("Remote", remote.len(), cx).mt_1p5());
            for (ix, branch) in remote.iter().enumerate() {
                list = list.child(self.branch_row(1000 + ix, branch, cx));
            }
        }
        if !tags.is_empty() {
            list = list.child(group("Tags", tags.len(), cx).mt_1p5());
            for (ix, tag) in tags.iter().enumerate() {
                list = list.child(self.tag_row(ix, tag, cx));
            }
        }
        list.into_any_element()
    }

    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(pick) = self.picked.clone() else {
            return div().into_any_element();
        };
        let head = match &pick {
            Pick::Branch(name) | Pick::Tag(name) => name.clone(),
        };
        let base = self.base_of(&head);
        let current = self.current().map(|b| b.name.clone());
        let branch = match &pick {
            Pick::Branch(name) => self.branch(name).cloned(),
            Pick::Tag(_) => None,
        };
        let refs = self.refs.clone().unwrap_or_default();
        let view = cx.entity().downgrade();
        let base_button = Button::new("git-base")
            .outline()
            .small()
            .label(base.clone().unwrap_or_else(|| "—".into()))
            .dropdown_caret(true)
            .dropdown_menu(move |menu: PopupMenu, _, _| {
                let mut menu = menu.label("Set against");
                for b in refs.local.iter().chain(&refs.remote) {
                    let (view, name) = (view.clone(), b.name.clone());
                    menu = menu.item(PopupMenuItem::new(b.name.clone()).on_click(
                        move |_, _, cx| {
                            let name = name.clone();
                            view.update(cx, |this, cx| this.set_base(name, cx)).ok();
                        },
                    ));
                }
                menu
            });
        let ctx = self.ctx.clone();
        let mut actions = h_flex().gap_1p5();
        if let (Some(branch), Some(current)) = (&branch, &current)
            && !branch.head
        {
            if let Some(tree) = branch.worktree.clone() {
                let ctx = ctx.clone();
                actions = actions.child(
                    Button::new("git-open-worktree")
                        .outline()
                        .small()
                        .icon(Glyph::FolderGit2)
                        .label("Its worktree")
                        .tooltip(tree.display().to_string())
                        .on_click(move |_, _, cx| {
                            let tree = tree.clone();
                            ctx.window
                                .update(cx, |git, cx| {
                                    git.show_tab(Tab::Worktrees, cx);
                                    git.pick_worktree(tree, cx);
                                })
                                .ok();
                        }),
                );
            } else {
                let (ctx, name) = (ctx.clone(), branch.name.clone());
                let checkout = if branch.remote {
                    name.split_once('/')
                        .map_or(name.clone(), |(_, n)| n.to_owned())
                } else {
                    name
                };
                actions = actions.child(
                    Button::new("git-checkout")
                        .outline()
                        .small()
                        .icon(Glyph::ArrowRight)
                        .label("Checkout")
                        .on_click(move |_, window, cx| {
                            ctx.act(Op::Checkout(checkout.clone()), window, cx)
                        }),
                );
            }
            let (ctx_m, name) = (ctx.clone(), branch.name.clone());
            actions = actions.child(
                Button::new("git-merge")
                    .primary()
                    .small()
                    .icon(Glyph::GitMerge)
                    .label(format!("Merge into {current}"))
                    .on_click(move |_, window, cx| ctx_m.act(Op::Merge(name.clone()), window, cx)),
            );
        }
        if let Some(branch) = branch.clone() {
            let menu_view = cx.entity().downgrade();
            actions = actions.child(
                Button::new("git-branch-more")
                    .outline()
                    .small()
                    .icon(Glyph::Ellipsis)
                    .dropdown_menu(move |menu: PopupMenu, _, cx| match menu_view.upgrade() {
                        Some(view) => view.read(cx).branch_menu(&branch, menu),
                        None => menu,
                    }),
            );
        }
        let tone = if matches!(pick, Pick::Tag(_)) {
            theme.warning
        } else {
            theme.blue
        };
        let compare = self.compare.clone();
        v_flex()
            .flex_shrink_0()
            .gap_2p5()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_2()
                    .child(pill(&head, tone, false, cx))
                    .child(
                        Icon::new(Glyph::ArrowRight)
                            .xsmall()
                            .text_color(theme.muted_foreground),
                    )
                    .child(base_button)
                    .child(div().flex_1())
                    .child(actions),
            )
            .child(compare.update(cx, |c, cx| c.summary(cx)))
            .child(compare.update(cx, |c, cx| c.tabs(cx)))
            .into_any_element()
    }
}

impl Render for Branches {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let view = cx.entity().downgrade();
        let (root, ctx) = (self.root.clone(), self.ctx.clone());
        let new = Button::new("git-new-branch")
            .outline()
            .small()
            .icon(Glyph::Plus)
            .label("New")
            .on_click(move |_, window, cx| {
                let (ctx, view) = (ctx.clone(), view.clone());
                let from = view
                    .upgrade()
                    .and_then(|v| v.read(cx).current().map(|b| b.name.clone()))
                    .unwrap_or_else(|| "HEAD".into());
                let _ = &root;
                ask_name(
                    "New branch",
                    "Name of the branch",
                    "",
                    "Make and check out",
                    window,
                    cx,
                    move |name, window, cx| {
                        ctx.act(
                            Op::Branch {
                                name,
                                from: from.clone(),
                                checkout: true,
                            },
                            window,
                            cx,
                        )
                    },
                );
            });
        let body = if self.picked.is_some() {
            v_flex()
                .size_full()
                .child(self.header(cx))
                .child(div().flex_1().min_h_0().child(self.compare.clone()))
                .into_any_element()
        } else {
            note("Pick a branch.", cx)
        };
        h_flex()
            .size_full()
            .child(
                v_flex()
                    .w(px(360.))
                    .h_full()
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(border)
                    .child(
                        h_flex()
                            .h(px(40.))
                            .px_2()
                            .gap_1p5()
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(border)
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(&self.filter).small().cleanable(true)),
                            )
                            .child(new),
                    )
                    .child(
                        div()
                            .id("git-branch-list")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(self.list(cx)),
                    ),
            )
            .child(div().flex_1().min_w_0().h_full().child(body))
    }
}
