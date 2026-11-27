use std::path::{Path, PathBuf};

use corvo_domain::{Chat, Op, Refs, Repo, Worktree};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div,
    prelude::FluentBuilder as _, px, rgb,
};

use super::compare::Compare;
use super::parts::{apart, ask_name, badge, note, pill};
use super::{Ctx, Event};
use crate::agents::Agents;
use crate::palette::{Offer, finds};

// A worktree as the list shows it: the folder, and how it stands as git last read it there.
#[derive(Clone)]
struct Tree {
    tree: Worktree,
    // Read in its own folder: what changed there, and what is half done.
    repo: Option<Repo>,
}

// The folders checked out from the repository shown: the main one and those added with git
// worktree, an agent's among them. Each shows its branch, what changed in it and how far it went
// from the main branch, and the chat working in it; the one picked is set against the main
// branch, to review what was done there and merge it.
pub struct Worktrees {
    ctx: Ctx,
    root: Option<PathBuf>,
    agents: Option<Entity<Agents>>,
    trees: Vec<Tree>,
    refs: Option<Refs>,
    picked: Option<PathBuf>,
    compare: Entity<Compare>,
    _reading: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl Worktrees {
    pub fn new(ctx: Ctx, cx: &mut Context<Self>) -> Self {
        let compare = cx.new(|cx| Compare::new(ctx.clone(), cx));
        let subs = vec![cx.observe(&compare, |_, _, cx| cx.notify())];
        Self {
            ctx,
            root: None,
            agents: None,
            trees: Vec::new(),
            refs: None,
            picked: None,
            compare,
            _reading: None,
            _subs: subs,
        }
    }

    pub fn set_agents(&mut self, agents: Entity<Agents>, cx: &mut Context<Self>) {
        self._subs.push(cx.observe(&agents, |_, _, cx| cx.notify()));
        self.agents = Some(agents);
    }

    // The chat whose folder is inside a worktree, the newest first.
    fn chat_in<'a>(&self, path: &Path, cx: &'a App) -> Option<&'a Chat> {
        let agents = self.agents.as_ref()?.read(cx);
        agents
            .chats()
            .list()
            .filter(|chat| !chat.is_archived() && chat.project().starts_with(path))
            .max_by_key(|chat| (chat.is_working(), chat.id()))
    }

    // Whether an agent works in a worktree other than the main one.
    pub fn agent_working(&self, cx: &App) -> bool {
        self.trees
            .iter()
            .filter(|t| !t.tree.main)
            .any(|t| self.chat_in(&t.tree.path, cx).is_some_and(Chat::is_working))
    }

    pub fn show(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        if root != self.root {
            self.trees.clear();
            self.picked = None;
        }
        self.root = root;
        let Some(root) = self.root.clone() else {
            return;
        };
        let git = self.ctx.repos.read(cx).git();
        let task = cx.background_executor().spawn(async move {
            let trees = git.worktrees(&root).unwrap_or_default();
            let refs = git.refs(&root).ok();
            let trees: Vec<Tree> = trees
                .into_iter()
                .map(|tree| {
                    let repo = if tree.prunable {
                        None
                    } else {
                        git.repo(&tree.path).ok().flatten()
                    };
                    Tree { tree, repo }
                })
                .collect();
            (trees, refs)
        });
        self._reading = Some(cx.spawn(async move |this, cx| {
            let (trees, refs) = task.await;
            this.update(cx, |this, cx| {
                this.trees = trees;
                this.refs = refs;
                let still = this
                    .picked
                    .as_ref()
                    .is_some_and(|p| this.trees.iter().any(|t| t.tree.path == *p));
                if !still {
                    this.picked = this
                        .trees
                        .iter()
                        .find(|t| !t.tree.main)
                        .or(this.trees.first())
                        .map(|t| t.tree.path.clone());
                }
                this.set_compare(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn pick(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.picked = Some(path);
        self.set_compare(cx);
        cx.notify();
    }

    fn main(&self) -> Option<String> {
        self.refs.as_ref()?.main.clone()
    }

    fn set_compare(&mut self, cx: &mut Context<Self>) {
        let (Some(root), Some(main)) = (self.root.clone(), self.main()) else {
            return;
        };
        let branch = self
            .picked
            .as_ref()
            .and_then(|p| self.trees.iter().find(|t| t.tree.path == *p))
            .and_then(|t| t.tree.branch.clone());
        match branch {
            Some(branch) if branch != main => {
                self.compare
                    .update(cx, |c, cx| c.set(root, main, branch, cx));
            }
            _ => self.compare.update(cx, |c, cx| c.clear(cx)),
        }
    }

    // Where a new worktree goes: beside the repository, in a folder of its own named after it,
    // so it is never inside the repository's own files.
    fn place(&self, branch: &str) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        let name = root.file_name()?.to_string_lossy().into_owned();
        let slug: String = branch
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        Some(root.parent()?.join(format!("{name}-worktrees")).join(slug))
    }

    fn add(
        &self,
        branch: String,
        from: Option<String>,
        agent: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (Some(root), Some(path)) = (self.root.clone(), self.place(&branch)) else {
            return;
        };
        if !agent {
            self.ctx
                .act(Op::AddWorktree { path, branch, from }, window, cx);
            return;
        }
        // For an agent: once the worktree is there, a chat opens in it.
        let git = self.ctx.repos.read(cx).git();
        let ctx = self.ctx.clone();
        let op = Op::AddWorktree {
            path: path.clone(),
            branch,
            from,
        };
        let task = cx
            .background_executor()
            .spawn(async move { git.run(&root, &op) });
        window
            .spawn(cx, async move |cx| {
                let done = task.await;
                cx.update(|window, cx| match done {
                    Ok(()) => {
                        ctx.repos.update(cx, |repos, cx| repos.refresh(cx));
                        ctx.window.update(cx, |git, cx| git.reread(cx)).ok();
                        ctx.emit(
                            Event::Ask {
                                project: path,
                                prompt: String::new(),
                            },
                            cx,
                        );
                    }
                    Err(why) => corvo_ui::notice::warn(why.to_string(), window, cx),
                })
                .ok();
            })
            .detach();
    }

    pub fn offers(&self, query: &str, _: &App) -> Vec<Offer> {
        self.trees
            .iter()
            .filter(|t| finds(query, &t.tree.path.to_string_lossy()))
            .map(|t| {
                let (view, path) = (self.ctx.window.clone(), t.tree.path.clone());
                Offer::new(Glyph::FolderGit2, t.tree.name(), move |_, cx| {
                    let path = path.clone();
                    view.update(cx, |git, cx| git.pick_worktree(path, cx)).ok();
                })
                .detail(t.tree.branch.clone().unwrap_or_else(|| "detached".into()))
            })
            .collect()
    }

    fn card(&self, ix: usize, t: &Tree, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let claude: Hsla = rgb(0xd97757).into();
        let on = self.picked.as_ref() == Some(&t.tree.path);
        let here = self.root.as_deref() == Some(t.tree.path.as_path());
        let branch = self
            .refs
            .as_ref()
            .zip(t.tree.branch.as_deref())
            .and_then(|(refs, name)| refs.branch(name).cloned());
        let changed = t.repo.as_ref().map_or(0, |r| r.changes().len());
        let chat = self
            .chat_in(&t.tree.path, cx)
            .map(|c| (c.title().to_owned(), c.is_working()));
        let view = cx.entity().downgrade();
        let path = t.tree.path.clone();
        let merged = branch.as_ref().is_some_and(|b| b.merged);
        let base = branch.as_ref().and_then(|b| b.base);
        let main = self.main().unwrap_or_else(|| "main".into());
        let remove_ctx = self.ctx.clone();
        let remove_path = t.tree.path.clone();
        let dirty = changed > 0;
        v_flex()
            .id(("git-tree", ix))
            .mx_1p5()
            .px_2p5()
            .py_2p5()
            .gap_1p5()
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|r| r.bg(theme.muted))
            .when(on, |r| r.bg(theme.secondary))
            .when(t.tree.prunable, |r| r.opacity(0.5))
            .on_click(move |_, _, cx| {
                let path = path.clone();
                view.update(cx, |this, cx| this.pick(path, cx)).ok();
            })
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Icon::new(Glyph::FolderGit2)
                            .small()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(t.tree.name()),
                    )
                    .child(pill(
                        t.tree.branch.as_deref().unwrap_or("detached"),
                        if chat.is_some() { claude } else { theme.blue },
                        false,
                        cx,
                    ))
                    .child(div().flex_1())
                    .when(here, |r| {
                        r.child(badge("this window", theme.muted_foreground))
                    })
                    .when_some(chat.clone(), |r, (_, working)| {
                        r.child(badge(
                            if working { "Agent · working" } else { "Agent" },
                            claude,
                        ))
                    }),
            )
            .child(
                div()
                    .truncate()
                    .text_size(px(11.5))
                    .font_family("monospace")
                    .text_color(theme.muted_foreground.opacity(0.7))
                    .child(t.tree.path.display().to_string()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if t.tree.prunable {
                        "its folder is gone".to_owned()
                    } else if changed == 0 {
                        "clean".to_owned()
                    } else {
                        format!("{changed} changed")
                    })
                    .when_some(base, |r, base| {
                        r.child(apart(base.ahead, base.behind, cx))
                            .child(main.clone())
                    })
                    .when(merged && !t.tree.main, |r| {
                        r.child(badge("merged", theme.green))
                    })
                    .child(div().flex_1())
                    .when(merged && !t.tree.main && !here, |r| {
                        r.child(
                            Button::new(("git-tree-remove", ix))
                                .outline()
                                .xsmall()
                                .label("Remove")
                                .on_click(move |_, window, cx| {
                                    remove_ctx.act(
                                        Op::RemoveWorktree {
                                            path: remove_path.clone(),
                                            force: dirty,
                                        },
                                        window,
                                        cx,
                                    )
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    fn detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let claude: Hsla = rgb(0xd97757).into();
        let Some(t) = self
            .picked
            .as_ref()
            .and_then(|p| self.trees.iter().find(|t| t.tree.path == *p))
            .cloned()
        else {
            return note("Pick a worktree.", cx);
        };
        let path = t.tree.path.clone();
        let chat = self
            .chat_in(&path, cx)
            .map(|c| (c.id(), c.title().to_owned(), c.is_working()));
        let ctx = self.ctx.clone();
        let main = self.main();
        let current = self
            .refs
            .as_ref()
            .and_then(|r| r.local.iter().find(|b| b.head))
            .map(|b| b.name.clone());
        let branch = t.tree.branch.clone();
        let (copy, term, start) = (path.clone(), path.clone(), path.clone());
        let (ctx_t, ctx_a, ctx_c) = (ctx.clone(), ctx.clone(), ctx.clone());
        let strip = match chat {
            Some((id, title, working)) => h_flex()
                .gap_2()
                .px_2p5()
                .py_2()
                .rounded(px(8.))
                .bg(claude.opacity(0.14))
                .child(Icon::new(Glyph::Bot).small().text_color(claude))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .child(if working {
                            format!("An agent is working here · {title}")
                        } else {
                            format!("An agent's chat is here · {title}")
                        }),
                )
                .child(
                    Button::new("git-tree-chat")
                        .ghost()
                        .small()
                        .icon(Glyph::MessageSquare)
                        .label("Open chat")
                        .on_click(move |_, _, cx| ctx_c.emit(Event::Chat(id), cx)),
                ),
            None => h_flex().child(
                Button::new("git-tree-agent")
                    .ghost()
                    .small()
                    .icon(Glyph::Bot)
                    .label("Start an agent here")
                    .on_click(move |_, _, cx| {
                        ctx_a.emit(
                            Event::Ask {
                                project: start.clone(),
                                prompt: String::new(),
                            },
                            cx,
                        )
                    }),
            ),
        };
        let mut actions = h_flex().gap_1p5().child(
            Button::new("git-tree-term")
                .outline()
                .small()
                .icon(Glyph::SquareTerminal)
                .label("Terminal here")
                .on_click(move |_, _, cx| ctx_t.emit(Event::Terminal(term.clone()), cx)),
        );
        if let (Some(branch), Some(current)) = (branch.clone(), current.clone())
            && Some(&branch) != Some(&current)
        {
            let ctx_m = ctx.clone();
            actions = actions.child(
                Button::new("git-tree-merge")
                    .primary()
                    .small()
                    .icon(Glyph::GitMerge)
                    .label(format!("Merge into {current}"))
                    .on_click(move |_, window, cx| {
                        ctx_m.act(Op::Merge(branch.clone()), window, cx)
                    }),
            );
        }
        let dirty = t.repo.as_ref().is_some_and(|r| !r.changes().is_empty());
        if !t.tree.main && self.root.as_deref() != Some(path.as_path()) {
            let (ctx_r, remove) = (ctx.clone(), path.clone());
            actions = actions.child(
                Button::new("git-tree-more")
                    .outline()
                    .small()
                    .icon(Glyph::Ellipsis)
                    .dropdown_menu(move |menu: PopupMenu, _, _| {
                        let (ctx, remove, copy) = (ctx_r.clone(), remove.clone(), copy.clone());
                        menu.item(PopupMenuItem::new("Copy path").icon(Glyph::Copy).on_click(
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    copy.display().to_string(),
                                ))
                            },
                        ))
                        .separator()
                        .item(
                            PopupMenuItem::new("Remove…").icon(Glyph::Trash).on_click(
                                move |_, window, cx| {
                                    ctx.act(
                                        Op::RemoveWorktree {
                                            path: remove.clone(),
                                            force: dirty,
                                        },
                                        window,
                                        cx,
                                    )
                                },
                            ),
                        )
                    }),
            );
        }
        let reviewing = branch.is_some() && branch != main;
        let compare = self.compare.clone();
        let changed = t.repo.as_ref().map_or(0, |r| r.changes().len());
        v_flex()
            .size_full()
            .child(
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
                            .child(div().text_base().font_weight(FontWeight::MEDIUM).child(t.tree.name()))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .font_family("monospace")
                                    .text_color(theme.muted_foreground)
                                    .child(path.display().to_string()),
                            )
                            .child(div().flex_1())
                            .child(actions),
                    )
                    .child(strip)
                    .when(changed > 0, |c| {
                        c.child(
                            div()
                                .text_xs()
                                .text_color(theme.warning)
                                .child(format!(
                                    "{changed} files changed there are not committed yet: they are not in what merges."
                                )),
                        )
                    })
                    .when(reviewing, |c| {
                        c.child(compare.update(cx, |c, cx| c.summary(cx)))
                            .child(compare.update(cx, |c, cx| c.tabs(cx)))
                    }),
            )
            .child(div().flex_1().min_h_0().child(if reviewing {
                self.compare.clone().into_any_element()
            } else {
                note("The main branch's own folder: nothing to review against it.", cx)
            }))
            .into_any_element()
    }
}

impl Render for Worktrees {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let current = self
            .refs
            .as_ref()
            .and_then(|r| r.local.iter().find(|b| b.head))
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "HEAD".into());
        let view = cx.entity().downgrade();
        let new = Button::new("git-tree-new")
            .outline()
            .small()
            .icon(Glyph::Plus)
            .label("New worktree")
            .dropdown_caret(true)
            .dropdown_menu(move |menu: PopupMenu, _, _| {
                let (a, b, c) = (view.clone(), view.clone(), view.clone());
                let (from_b, from_c) = (current.clone(), current.clone());
                menu.item(
                    PopupMenuItem::new("From a branch…")
                        .icon(Glyph::GitBranch)
                        .on_click(move |_, window, cx| {
                            let view = a.clone();
                            ask_name(
                                "Worktree from a branch",
                                "Branch to check out there",
                                "",
                                "Make",
                                window,
                                cx,
                                move |branch, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.add(branch, None, false, window, cx)
                                    })
                                    .ok();
                                },
                            );
                        }),
                )
                .item(
                    PopupMenuItem::new("On a new branch…")
                        .icon(Glyph::GitBranchPlus)
                        .on_click(move |_, window, cx| {
                            let (view, from) = (b.clone(), from_b.clone());
                            ask_name(
                                format!("Worktree on a new branch from {from}"),
                                "Name of the branch",
                                "",
                                "Make",
                                window,
                                cx,
                                move |branch, window, cx| {
                                    let from = from.clone();
                                    view.update(cx, |this, cx| {
                                        this.add(branch, Some(from), false, window, cx)
                                    })
                                    .ok();
                                },
                            );
                        }),
                )
                .separator()
                .item(
                    PopupMenuItem::new("For an agent…")
                        .icon(Glyph::Bot)
                        .on_click(move |_, window, cx| {
                            let (view, from) = (c.clone(), from_c.clone());
                            ask_name(
                                "A worktree for an agent",
                                "What it works on, as a branch name",
                                "agent/",
                                "Make and open a chat",
                                window,
                                cx,
                                move |branch, window, cx| {
                                    let from = from.clone();
                                    view.update(cx, |this, cx| {
                                        this.add(branch, Some(from), true, window, cx)
                                    })
                                    .ok();
                                },
                            );
                        }),
                )
            });
        let prunable = self.trees.iter().any(|t| t.tree.prunable);
        let prune_ctx = self.ctx.clone();
        let trees = self.trees.clone();
        let cards: Vec<AnyElement> = trees
            .iter()
            .enumerate()
            .map(|(ix, t)| self.card(ix, t, cx))
            .collect();
        let detail = self.detail(cx);
        h_flex()
            .size_full()
            .child(
                v_flex()
                    .w(px(420.))
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
                            .child(div().font_weight(FontWeight::MEDIUM).child("Worktrees"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(self.trees.len().to_string()),
                            )
                            .child(div().flex_1())
                            .child(new),
                    )
                    .child(
                        v_flex()
                            .id("git-tree-list")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .py_1p5()
                            .gap_0p5()
                            .children(cards)
                            .when(prunable, |l| {
                                l.child(
                                    h_flex()
                                        .mx_3()
                                        .mt_2()
                                        .gap_2()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(Icon::new(Glyph::CircleAlert).xsmall())
                                        .child(
                                            div().flex_1().child("A worktree whose folder is gone"),
                                        )
                                        .child(
                                            Button::new("git-prune")
                                                .ghost()
                                                .xsmall()
                                                .label("Prune")
                                                .on_click(move |_, window, cx| {
                                                    prune_ctx.act(Op::PruneWorktrees, window, cx)
                                                }),
                                        ),
                                )
                            }),
                    ),
            )
            .child(div().flex_1().min_w_0().h_full().child(detail))
    }
}
