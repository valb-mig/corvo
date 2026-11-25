use std::collections::HashSet;

use corvo_domain::{Chat, End, Entry, Part, Status, Step, StepKind, ago};
use corvo_ui::agent::{AgentLogo, Sign, diff_lines, step_glyph};
use corvo_ui::field;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, EventEmitter, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, div,
    prelude::*, px, relative,
};

use crate::agents::{Agents, now};
use crate::pane::dock::{Project, clock, relative as from_project};

pub enum Event {
    Close,
}

const OUTPUT: usize = 60;
// The width of the list of chats.
const LIST: f32 = 232.;

// The chats of the workspace in a window over the home: the list on the left, with a search and
// a new chat, and the picked chat's conversation on the right, the agent bar over its foot.
pub struct ChatWindow {
    agents: Entity<Agents>,
    project: Project,
    workspace: SharedString,
    search: Entity<field::State>,
    renaming: Option<(u64, Entity<field::State>, Subscription)>,
    deleting: Option<u64>,
    archived: bool,
    open: HashSet<(u64, String)>,
    scroll: ScrollHandle,
    shown: Option<u64>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for ChatWindow {}

impl ChatWindow {
    pub fn new(
        agents: Entity<Agents>,
        project: Project,
        workspace: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| field::state("Search chats", window, cx));
        let subs = vec![
            cx.observe(&agents, |this: &mut Self, _, cx| {
                // The conversation follows what comes in, unless the dev scrolled up to read.
                if this.at_bottom() {
                    this.scroll.scroll_to_bottom();
                }
                cx.notify();
            }),
            cx.subscribe(&search, |_, _, event: &field::Event, cx| {
                if matches!(event, field::Event::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            agents,
            project,
            workspace,
            search,
            renaming: None,
            deleting: None,
            archived: false,
            open: HashSet::new(),
            scroll: ScrollHandle::new(),
            shown: None,
            _subs: subs,
        }
    }

    // Opened again: the picked chat shows from its end.
    pub fn shown(&mut self, cx: &mut Context<Self>) {
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn at_bottom(&self) -> bool {
        let max = self.scroll.max_offset().y;
        let at = -self.scroll.offset().y;
        max - at < px(48.)
    }

    fn pick(&mut self, id: u64, cx: &mut Context<Self>) {
        self.deleting = None;
        self.agents.update(cx, |agents, cx| agents.pick(id, cx));
        self.scroll.scroll_to_bottom();
    }

    fn new_chat(&mut self, cx: &mut Context<Self>) {
        let project = (self.project)(cx);
        self.agents
            .update(cx, |agents, cx| agents.new_chat(&project, cx));
    }

    fn rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(title) = self
            .agents
            .read(cx)
            .chats()
            .get(id)
            .map(|chat| chat.title().to_owned())
        else {
            return;
        };
        let state = cx.new(|cx| field::state("Chat name", window, cx));
        state.update(cx, |state, cx| {
            state.set_value(title, window, cx);
            state.select_all(window, cx);
            state.focus(window, cx);
        });
        let sub = cx.subscribe_in(&state, window, |this, _, event, _, cx| {
            if matches!(event, field::Event::PressEnter { .. } | field::Event::Blur) {
                this.renamed(cx);
            }
        });
        self.renaming = Some((id, state, sub));
        cx.notify();
    }

    // The name typed in goes to the chat; a blank one leaves it as it was.
    fn renamed(&mut self, cx: &mut Context<Self>) {
        let Some((id, state, _)) = self.renaming.take() else {
            return;
        };
        let title = state.read(cx).value().to_string();
        self.agents
            .update(cx, |agents, cx| agents.rename(id, &title, cx));
    }

    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let query = self.search.read(cx).value().to_string();
        let agents = self.agents.read(cx);
        let chats = agents.chats();
        let active = chats.active_id();
        let now = now();
        let today = midnight(now);

        let found: Vec<&Chat> = chats.list().filter(|chat| chat.matches(&query)).collect();
        let count = if query.trim().is_empty() {
            let live = chats.list().filter(|chat| !chat.is_archived()).count();
            format!("{live} chats")
        } else {
            format!("{} of {}", found.len(), chats.len())
        };

        let mut groups: Vec<(&str, Vec<&Chat>)> = vec![
            ("Today", Vec::new()),
            ("Yesterday", Vec::new()),
            ("Earlier", Vec::new()),
        ];
        let mut archived = Vec::new();
        for chat in &found {
            if chat.is_archived() {
                archived.push(*chat);
            } else if chat.updated() >= today {
                groups[0].1.push(chat);
            } else if chat.updated() >= today.saturating_sub(86_400) {
                groups[1].1.push(chat);
            } else {
                groups[2].1.push(chat);
            }
        }

        let view = cx.entity().downgrade();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (name, items) in groups {
            if items.is_empty() {
                continue;
            }
            rows.push(group(name, cx).into_any_element());
            for chat in items {
                rows.push(self.row(chat, active, &query, now, &view, cx));
            }
        }
        if !archived.is_empty() {
            let open = self.archived || !query.trim().is_empty();
            let toggle = view.clone();
            rows.push(
                div()
                    .id("archived")
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px_2()
                    .pt_3()
                    .pb_1()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .on_click(move |_, _, cx| {
                        toggle
                            .update(cx, |this, cx| {
                                this.archived = !this.archived;
                                cx.notify();
                            })
                            .ok();
                    })
                    .child(
                        Icon::new(if open {
                            Glyph::ChevronDown
                        } else {
                            Glyph::ChevronRight
                        })
                        .size(px(11.)),
                    )
                    .child(format!("ARCHIVED · {}", archived.len()))
                    .into_any_element(),
            );
            if open {
                for chat in archived {
                    rows.push(self.row(chat, active, &query, now, &view, cx));
                }
            }
        }
        if rows.is_empty() && !query.trim().is_empty() {
            rows.push(
                div()
                    .py_4()
                    .text_center()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format!("No chats match “{}”", query.trim()))
                    .into_any_element(),
            );
        }

        let fresh = view.clone();
        v_flex()
            .w(px(LIST))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(
                h_flex()
                    .h(px(44.))
                    .flex_shrink_0()
                    .px_4()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .text_sm()
                    .child(
                        Icon::new(Glyph::LayoutGrid)
                            .size(px(14.))
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(self.workspace.clone()),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(count),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .px_2()
                    .pt_3()
                    .pb_1()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.search).small().prefix(
                                Icon::new(Glyph::Search)
                                    .size(px(13.))
                                    .text_color(theme.muted_foreground),
                            ),
                        ),
                    )
                    .child(
                        Button::new("new-chat")
                            .ghost()
                            .small()
                            .icon(Glyph::Plus)
                            .tooltip("New chat")
                            .on_click(move |_, _, cx| {
                                fresh.update(cx, |this, cx| this.new_chat(cx)).ok();
                            }),
                    ),
            )
            .child(
                div()
                    .id("chat-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_3()
                    .children(rows),
            )
    }

    fn row(
        &self,
        chat: &Chat,
        active: Option<u64>,
        query: &str,
        now: u64,
        view: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let id = chat.id();
        let on = Some(id) == active;
        let renaming = self
            .renaming
            .as_ref()
            .filter(|(which, _, _)| *which == id)
            .map(|(_, state, _)| state.clone());
        let title: AnyElement = match renaming {
            Some(state) => Input::new(&state).xsmall().into_any_element(),
            None => div()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(if chat.is_archived() {
                    theme.muted_foreground
                } else {
                    theme.foreground
                })
                .child(chat.title().to_owned())
                .into_any_element(),
        };

        let second: AnyElement = if self.deleting == Some(id) {
            let (cancel, delete) = (view.clone(), view.clone());
            h_flex()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Delete this chat?"),
                )
                .child(
                    Button::new(("cancel-delete", id))
                        .ghost()
                        .xsmall()
                        .label("Cancel")
                        .on_click(move |_, _, cx| {
                            cancel
                                .update(cx, |this, cx| {
                                    this.deleting = None;
                                    cx.notify();
                                })
                                .ok();
                        }),
                )
                .child(
                    Button::new(("delete", id))
                        .danger()
                        .xsmall()
                        .label("Delete")
                        .on_click(move |_, _, cx| {
                            delete
                                .update(cx, |this, cx| {
                                    this.deleting = None;
                                    this.agents.update(cx, |agents, cx| agents.delete(id, cx));
                                })
                                .ok();
                        }),
                )
                .into_any_element()
        } else {
            status(chat, query, cx)
        };

        let menu = {
            let view = view.clone();
            let archived = chat.is_archived();
            move |menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                let (rename, archive, delete) = (view.clone(), view.clone(), view.clone());
                menu.item(PopupMenuItem::new("Rename").icon(Glyph::Pencil).on_click(
                    move |_, window, cx| {
                        rename
                            .update(cx, |this, cx| this.rename(id, window, cx))
                            .ok();
                    },
                ))
                .item(
                    PopupMenuItem::new(if archived { "Unarchive" } else { "Archive" })
                        .icon(if archived {
                            Glyph::ArchiveRestore
                        } else {
                            Glyph::Archive
                        })
                        .on_click(move |_, _, cx| {
                            archive
                                .update(cx, |this, cx| {
                                    this.agents
                                        .update(cx, |agents, cx| agents.archive(id, !archived, cx));
                                })
                                .ok();
                        }),
                )
                .separator()
                .item(
                    PopupMenuItem::new("Delete…")
                        .icon(Glyph::Trash)
                        .on_click(move |_, _, cx| {
                            delete
                                .update(cx, |this, cx| {
                                    this.deleting = Some(id);
                                    cx.notify();
                                })
                                .ok();
                        }),
                )
            }
        };

        let pick = view.clone();
        div()
            .id(("chat", id))
            .group("chat-row")
            .flex()
            .flex_col()
            .gap(px(3.))
            .px_2()
            .py(px(7.))
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|row| row.bg(theme.muted))
            .when(on, |row| row.bg(theme.secondary))
            .on_click(move |_, _, cx| {
                pick.update(cx, |this, cx| this.pick(id, cx)).ok();
            })
            .context_menu({
                let menu = menu.clone();
                move |popup, window, cx| menu(popup, window, cx)
            })
            .child(
                h_flex()
                    .gap_2()
                    .min_w_0()
                    .child(AgentLogo::new(chat.agent().to_owned()).size(px(18.)))
                    .child(div().flex_1().min_w_0().child(title))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .group_hover("chat-row", |when| when.invisible())
                            .child(ago(chat.updated(), now)),
                    )
                    .child(
                        div()
                            .absolute()
                            .right(px(6.))
                            .top(px(5.))
                            .invisible()
                            .group_hover("chat-row", |more| more.visible())
                            .child(
                                Button::new(("more", id))
                                    .ghost()
                                    .xsmall()
                                    .icon(Glyph::Ellipsis)
                                    .dropdown_menu(menu),
                            ),
                    ),
            )
            .child(div().pl(px(26.)).min_w_0().child(second))
            .into_any_element()
    }

    fn conversation(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity().downgrade();
        let theme = cx.theme();
        let agents = self.agents.read(cx);
        let chat = agents.active();
        let agent = chat.map_or_else(|| agents.agent().to_owned(), |chat| chat.agent().to_owned());
        let name = agents.agent_name(&agent);
        let model = agents
            .options()
            .and_then(|options| options.model.as_ref())
            .map(|model| model.name().to_owned());
        let turns = chat.map_or(0, Chat::turns);
        let mut sub = vec![name.clone()];
        sub.extend(model);
        sub.push(match turns {
            0 => "new".to_owned(),
            1 => "1 turn".to_owned(),
            n => format!("{n} turns"),
        });
        let title = chat.map_or_else(|| "New chat".to_owned(), |chat| chat.title().to_owned());

        let body: AnyElement = match chat.filter(|chat| chat.turns() > 0) {
            Some(chat) => self.thread(chat, agents, &view, cx).into_any_element(),
            None => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .pb(px(140.))
                .child(AgentLogo::new(agent.clone()).size(px(36.)))
                .child(
                    div()
                        .mt_2()
                        .text_base()
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("New chat in {}", self.workspace)),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(format!(
                            "Ask {name} below. The chat stays with this workspace."
                        )),
                )
                .into_any_element(),
        };

        let close = cx.entity().downgrade();
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                h_flex()
                    .h(px(44.))
                    .flex_shrink_0()
                    .pl_4()
                    .pr_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(AgentLogo::new(agent))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(sub.join(" · ")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .px_1()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Esc"),
                    )
                    .child(
                        Button::new("close-chat")
                            .ghost()
                            .small()
                            .icon(Glyph::Close)
                            .on_click(move |_, _, cx| {
                                close.update(cx, |_, cx| cx.emit(Event::Close)).ok();
                            }),
                    ),
            )
            .child(
                div()
                    .id("thread")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(body),
            )
    }

    fn thread(
        &self,
        chat: &Chat,
        agents: &Agents,
        view: &WeakEntity<Self>,
        cx: &App,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let working = chat.is_working();
        let count = chat.entries().len();
        let entries = chat
            .entries()
            .iter()
            .enumerate()
            .map(|(ix, entry)| match entry {
                Entry::You(text) => h_flex()
                    .justify_end()
                    .child(
                        div()
                            .max_w(relative(0.78))
                            .px(px(14.))
                            .py(px(10.))
                            .rounded(px(12.))
                            .bg(theme.secondary)
                            .border_1()
                            .border_color(theme.border)
                            .text_sm()
                            .child(text.clone()),
                    )
                    .into_any_element(),
                Entry::Switch(agent) => h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(div().flex_1().h(px(1.)).bg(theme.border))
                    .child(AgentLogo::new(agent.clone()).size(px(16.)))
                    .child(format!("Switched to {}", agents.agent_name(agent)))
                    .child(div().flex_1().h(px(1.)).bg(theme.border))
                    .into_any_element(),
                Entry::Reply(reply) => {
                    let live = working && ix + 1 == count;
                    let parts = reply
                        .parts()
                        .iter()
                        .enumerate()
                        .map(|(px_ix, part)| match part {
                            Part::Text(text) => div()
                                .text_sm()
                                .child(TextView::markdown(
                                    ElementId::Name(
                                        format!("text-{}-{ix}-{px_ix}", chat.id()).into(),
                                    ),
                                    text.clone(),
                                ))
                                .into_any_element(),
                            Part::Step(step) => self.step(chat, step, live, view, cx),
                        });
                    let foot: Option<AnyElement> = match reply.end() {
                        Some(end) => {
                            let (glyph, tint) = match end {
                                End::Done => (Glyph::Check, theme.success),
                                End::Stopped => (Glyph::Square, theme.muted_foreground),
                                End::Failed(_) => (Glyph::CircleAlert, theme.danger),
                            };
                            Some(
                                h_flex()
                                    .gap_1p5()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(Icon::new(glyph).size(px(12.)).text_color(tint))
                                    .child(format!("{} · {}", reply.summary(), clock(reply.secs())))
                                    .into_any_element(),
                            )
                        }
                        None if live
                            && (agents.is_thinking(chat.id())
                                || !reply.steps().any(|step| {
                                    matches!(step.status(), Status::Pending | Status::Running)
                                })) =>
                        {
                            Some(
                                h_flex()
                                    .gap_2()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(Spinner::new().xsmall())
                                    .child("Thinking…")
                                    .into_any_element(),
                            )
                        }
                        None => None,
                    };
                    h_flex()
                        .items_start()
                        .gap_3()
                        .child(AgentLogo::new(reply.agent().to_owned()))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_2()
                                .children(parts)
                                .children(foot),
                        )
                        .into_any_element()
                }
            });
        v_flex()
            .w_full()
            .max_w(px(780.))
            .mx_auto()
            .px_6()
            .pt_8()
            .pb(px(168.))
            .gap_5()
            .children(entries)
    }

    fn step(
        &self,
        chat: &Chat,
        step: &Step,
        live: bool,
        view: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let key = (chat.id(), step.id().to_owned());
        let open = self.open.contains(&key);
        let foldable = !step.diffs().is_empty() || !step.output().trim().is_empty();
        let running = live && matches!(step.status(), Status::Pending | Status::Running);
        let ask = step.ask().cloned();
        let failed = step.status() == Status::Failed;
        let tint = if ask.is_some() {
            theme.warning
        } else if failed {
            theme.danger
        } else {
            theme.muted_foreground
        };

        let lead: AnyElement = if running && ask.is_none() {
            Spinner::new().xsmall().into_any_element()
        } else {
            Icon::new(step_glyph(step.kind()))
                .size(px(14.))
                .text_color(tint)
                .into_any_element()
        };
        let target = step
            .target()
            .map(|target| from_project(target, chat))
            .filter(|target| !step.title().contains(target.as_str()));
        let extra = if matches!(step.kind(), StepKind::Edit) && !step.diffs().is_empty() {
            let (add, del) = step.lines();
            Some(
                h_flex()
                    .gap_1()
                    .child(div().text_color(theme.success).child(format!("+{add}")))
                    .child(div().text_color(theme.danger).child(format!("−{del}")))
                    .into_any_element(),
            )
        } else if failed {
            Some(
                div()
                    .text_color(theme.danger)
                    .child("failed")
                    .into_any_element(),
            )
        } else {
            None
        };

        let toggle = view.clone();
        let row_key = key.clone();
        let mut row = div()
            .id(ElementId::Name(
                format!("step-{}-{}", chat.id(), step.id()).into(),
            ))
            .flex()
            .items_center()
            .gap_2()
            .min_h(px(28.))
            .px_2()
            .mx(px(-8.))
            .rounded(px(6.))
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(
                div()
                    .w(px(16.))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(lead),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(relative(0.6))
                    .truncate()
                    .text_color(if ask.is_some() {
                        theme.warning
                    } else {
                        theme.foreground
                    })
                    .child(step.title().to_owned()),
            )
            .children(target.map(|target| {
                div()
                    .min_w_0()
                    .truncate()
                    .font_family("monospace")
                    .child(target)
            }))
            .child(div().flex_1())
            .children(extra);
        if foldable {
            row = row
                .cursor_pointer()
                .hover(|row| row.bg(theme.muted))
                .on_click(move |_, _, cx| {
                    let key = row_key.clone();
                    toggle
                        .update(cx, |this, cx| {
                            if !this.open.remove(&key) {
                                this.open.insert(key);
                            }
                            cx.notify();
                        })
                        .ok();
                })
                .child(
                    Icon::new(if open {
                        Glyph::ChevronDown
                    } else {
                        Glyph::ChevronRight
                    })
                    .size(px(12.)),
                );
        }
        if let Some(ask) = &ask {
            let buttons = ask.options.iter().enumerate().map(|(ix, answer)| {
                let agents = self.agents.clone();
                let (chat, step_id, ask_id, option) =
                    (chat.id(), step.id().to_owned(), ask.id, answer.id.clone());
                let button = Button::new(ElementId::Name(
                    format!("answer-{}-{}-{ix}", chat, step.id()).into(),
                ))
                .xsmall()
                .label(answer.name.clone())
                .on_click(move |_, _, cx| {
                    agents.update(cx, |agents, cx| {
                        agents.answer(chat, &step_id, ask_id, Some(&option), cx);
                    });
                });
                if answer.allow {
                    button.warning()
                } else {
                    button.ghost()
                }
            });
            row = row.child(h_flex().gap_1().children(buttons));
        }

        let body: Option<AnyElement> = (open && foldable).then(|| {
            if !step.diffs().is_empty() {
                v_flex()
                    .ml(px(24.))
                    .mb_2()
                    .gap_2()
                    .children(step.diffs().iter().map(|diff| {
                        let lines = diff_lines(diff.old.as_deref().unwrap_or(""), &diff.new);
                        v_flex()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .overflow_hidden()
                            .font_family("monospace")
                            .text_xs()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .text_color(theme.muted_foreground)
                                    .child(from_project(&diff.path.display().to_string(), chat)),
                            )
                            .children(lines.into_iter().map(|(sign, text)| {
                                let (mark, bg) = match sign {
                                    Sign::Add => ("+", Some(theme.success.opacity(0.1))),
                                    Sign::Del => ("−", Some(theme.danger.opacity(0.1))),
                                    Sign::Same => (" ", None),
                                };
                                h_flex()
                                    .when_some(bg, |line, bg| line.bg(bg))
                                    .child(
                                        div()
                                            .w(px(18.))
                                            .flex_shrink_0()
                                            .text_center()
                                            .text_color(theme.muted_foreground)
                                            .child(mark),
                                    )
                                    .child(div().whitespace_nowrap().child(text))
                            }))
                    }))
                    .into_any_element()
            } else {
                let lines: Vec<&str> = step.output().lines().collect();
                let cut = lines.len() > OUTPUT;
                let mut text = lines[..lines.len().min(OUTPUT)].join("\n");
                if cut {
                    text.push_str(&format!("\n… {} more lines", lines.len() - OUTPUT));
                }
                div()
                    .ml(px(24.))
                    .mb_2()
                    .px_3()
                    .py_2()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .font_family("monospace")
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(text)
                    .into_any_element()
            }
        });
        v_flex().child(row).children(body).into_any_element()
    }
}

impl Render for ChatWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.agents.read(cx).chats().active_id();
        if active != self.shown {
            self.shown = active;
            self.scroll.scroll_to_bottom();
        }
        h_flex()
            .size_full()
            .child(self.list(cx))
            .child(self.conversation(cx))
    }
}

// How a chat is doing, under its title; with a search, where it matched.
fn status(chat: &Chat, query: &str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let line = h_flex()
        .gap_1p5()
        .min_w_0()
        .text_xs()
        .text_color(theme.muted_foreground);
    if let Some((text, _)) = chat.snippet(query) {
        return line.child(div().truncate().child(text)).into_any_element();
    }
    let Some(reply) = chat.last_reply() else {
        return line.child("No messages yet").into_any_element();
    };
    if chat.is_waiting() {
        return line
            .text_color(theme.warning)
            .child(div().size(px(6.)).rounded_full().bg(theme.warning))
            .child("Waiting for you")
            .into_any_element();
    }
    match reply.end() {
        None => {
            let doing = reply
                .current()
                .map_or("Thinking".to_owned(), |step| step.title().to_owned());
            line.child(Spinner::new().xsmall())
                .child(div().truncate().child(doing))
                .into_any_element()
        }
        Some(End::Done) => line
            .child(
                div()
                    .truncate()
                    .child(format!("Done · {}", reply.summary())),
            )
            .into_any_element(),
        Some(_) => line
            .child(div().truncate().child(reply.summary()))
            .into_any_element(),
    }
}

fn group(name: &str, cx: &App) -> impl IntoElement + use<> {
    div()
        .px_2()
        .pt_3()
        .pb_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(name.to_uppercase())
}

// The start of today where the dev is, in seconds since the epoch.
fn midnight(now: u64) -> u64 {
    let time = now as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let got = unsafe { libc::localtime_r(&time, &mut tm) };
    if got.is_null() {
        return now - now % 86_400;
    }
    let into = tm.tm_hour as u64 * 3600 + tm.tm_min as u64 * 60 + tm.tm_sec as u64;
    now.saturating_sub(into)
}
