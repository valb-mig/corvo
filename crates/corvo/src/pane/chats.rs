use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;

use corvo_domain::{Chat, End, Entry, Message, Run, Status, Step, StepKind, ago, tally};
use corvo_ui::agent::{AgentLogo, Sign, diff_lines, step_glyph};
use corvo_ui::drawer::Drawer;
use corvo_ui::field;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Div, ElementId, Entity, EventEmitter,
    FontWeight, HighlightStyle, Hsla, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled as _, StyledImage as _, StyledText, Subscription, WeakEntity, Window,
    div, img, prelude::*, px, relative,
};

use crate::agents::{Agents, now};
use crate::pane::dock::{Project, clock, one_line, relative as from_project};

pub enum Event {
    // An image sent in the chat, to see in the code.
    Open(PathBuf),
}

const OUTPUT: usize = 60;
// The side of an image sent in the chat.
const IMAGE: f32 = 120.;
// The lines a long message of the dev shows until it is unfolded.
const FOLDED: usize = 8;

// The chats of the workspace in a window over the home: the picked chat's conversation, the
// agent bar over its foot, and the list of chats in a panel floating on its right, opened and
// closed with ctrl-b or the button of the window's bar. Its palette searches the chats and
// starts a new one.
pub struct ChatWindow {
    agents: Entity<Agents>,
    project: Project,
    workspace: SharedString,
    renaming: Option<(u64, Entity<field::State>, Subscription)>,
    deleting: Option<u64>,
    archived: bool,
    open: HashSet<(u64, String)>,
    // Groups of steps the dev opened or folded, by chat, entry and first part; the others are
    // open while the agent is at them.
    groups: HashMap<(u64, usize, usize), bool>,
    // The long messages of the dev shown whole, by chat and place in it.
    unfolded: HashSet<(u64, usize)>,
    scroll: ScrollHandle,
    shown: Option<u64>,
    // The list of chats is open.
    panel: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for ChatWindow {}

impl ChatWindow {
    pub fn new(
        agents: Entity<Agents>,
        project: Project,
        workspace: SharedString,
        cx: &mut Context<Self>,
    ) -> Self {
        let subs = vec![cx.observe(&agents, |this: &mut Self, _, cx| {
            // The conversation follows what comes in, unless the dev scrolled up to read.
            if this.at_bottom() {
                this.scroll.scroll_to_bottom();
            }
            cx.notify();
        })];
        Self {
            agents,
            project,
            workspace,
            renaming: None,
            deleting: None,
            archived: false,
            open: HashSet::new(),
            groups: HashMap::new(),
            unfolded: HashSet::new(),
            scroll: ScrollHandle::new(),
            shown: None,
            panel: false,
            _subs: subs,
        }
    }

    // Opened again: the picked chat shows from its end.
    pub fn shown(&mut self, cx: &mut Context<Self>) {
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    pub fn has_panel(&self) -> bool {
        self.panel
    }

    // Opens the list of chats, or closes it; tells whether it is open now.
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) -> bool {
        self.panel = !self.panel;
        // Closed, a name being typed is kept, as when the field is left.
        if !self.panel {
            self.renamed(cx);
            self.deleting = None;
        }
        cx.notify();
        self.panel
    }

    fn at_bottom(&self) -> bool {
        let max = self.scroll.max_offset().y;
        let at = -self.scroll.offset().y;
        max - at < px(48.)
    }

    pub fn pick(&mut self, id: u64, cx: &mut Context<Self>) {
        self.deleting = None;
        self.agents.update(cx, |agents, cx| agents.pick(id, cx));
        self.scroll.scroll_to_bottom();
    }

    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
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

    // The workspace and how many chats it has, for the window's bar.
    pub fn subtitle(&self, cx: &App) -> String {
        let chats = self.agents.read(cx).chats();
        let count = match chats.list().filter(|chat| !chat.is_archived()).count() {
            1 => "1 chat".to_owned(),
            live => format!("{live} chats"),
        };
        format!("{} · {count}", self.workspace)
    }

    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let agents = self.agents.read(cx);
        let chats = agents.chats();
        let active = chats.active_id();
        let now = now();
        let today = midnight(now);

        let mut groups: Vec<(&str, Vec<&Chat>)> = vec![
            ("Today", Vec::new()),
            ("Yesterday", Vec::new()),
            ("Earlier", Vec::new()),
        ];
        let mut archived = Vec::new();
        for chat in chats.list() {
            if chat.is_archived() {
                archived.push(chat);
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
                rows.push(self.row(chat, active, now, &view, cx));
            }
        }
        if !archived.is_empty() {
            let open = self.archived;
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
                    rows.push(self.row(chat, active, now, &view, cx));
                }
            }
        }

        Drawer::new("chat-panel").child(
            div()
                .id("chat-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_2()
                .pt_1()
                .pb_3()
                .children(rows),
        )
    }

    fn row(
        &self,
        chat: &Chat,
        active: Option<u64>,
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
            status(chat, cx)
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

        v_flex()
            .size_full()
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

    // What the dev wrote: the images above, each opening in the code, and the words under them,
    // a long message folded to its first lines.
    fn you(
        &self,
        chat: u64,
        message: &Message,
        ix: usize,
        view: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let (border, muted, secondary) = (theme.border, theme.muted_foreground, theme.secondary);
        let images = (!message.images.is_empty()).then(|| {
            h_flex()
                .max_w(relative(0.78))
                .flex_wrap()
                .justify_end()
                .gap_2()
                .children(message.images.iter().enumerate().map(|(n, path)| {
                    let view = view.clone();
                    let open = path.clone();
                    div()
                        .id(ElementId::Name(format!("image-{ix}-{n}").into()))
                        .size(px(IMAGE))
                        .rounded(px(10.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(border)
                        .bg(secondary)
                        .cursor_pointer()
                        .child(
                            img(path.clone())
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .with_fallback(move || {
                                    v_flex()
                                        .size_full()
                                        .items_center()
                                        .justify_center()
                                        .gap_1()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(Icon::new(Glyph::ImageOff).size(px(18.)))
                                        .child("Missing")
                                        .into_any_element()
                                }),
                        )
                        .on_click(move |_, _, cx| {
                            view.update(cx, |_, cx| cx.emit(Event::Open(open.clone())))
                                .ok();
                        })
                }))
        });
        let key = (chat, ix);
        let long = is_long(&message.text);
        let folded = long && !self.unfolded.contains(&key);
        let text = (!message.text.is_empty()).then(|| {
            div()
                .max_w(relative(0.78))
                .px(px(14.))
                .py(px(10.))
                .rounded(px(12.))
                .bg(secondary)
                .border_1()
                .border_color(border)
                .text_sm()
                .when(folded, |text| text.overflow_hidden().line_clamp(FOLDED))
                .child(message.text.clone())
        });
        let fold = long.then(|| {
            let view = view.clone();
            Button::new(ElementId::Name(format!("fold-{chat}-{ix}").into()))
                .ghost()
                .xsmall()
                .label(if folded { "Show more" } else { "Show less" })
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        if !this.unfolded.remove(&key) {
                            this.unfolded.insert(key);
                        }
                        cx.notify();
                    })
                    .ok();
                })
        });
        v_flex()
            .items_end()
            .gap_2()
            .children(images)
            .children(text)
            .children(fold)
            .into_any_element()
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
                Entry::You(message) => self.you(chat.id(), message, ix, view, cx),
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
                    let runs = reply.runs();
                    let last = runs.len().saturating_sub(1);
                    let parts = runs.iter().enumerate().map(|(n, run)| match run {
                        Run::Text(at, text) => prose(
                            ElementId::Name(format!("text-{}-{ix}-{at}", chat.id()).into()),
                            text,
                            cx,
                        ),
                        Run::Steps(_, steps) if steps.len() == 1 => {
                            self.step(chat, steps[0], live, Rail::default(), view, cx)
                        }
                        Run::Steps(at, steps) => {
                            self.steps(chat, (ix, *at), steps, live, live && n == last, view, cx)
                        }
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

    // Steps taken one after another, under a line that tells what they did. The group is open
    // while the agent is at it and folds once the agent says something after it, unless the dev
    // opened or folded it.
    #[allow(clippy::too_many_arguments)]
    fn steps(
        &self,
        chat: &Chat,
        (entry, at): (usize, usize),
        steps: &[&Step],
        live: bool,
        tail: bool,
        view: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let key = (chat.id(), entry, at);
        let busy = live
            && steps
                .iter()
                .any(|step| matches!(step.status(), Status::Pending | Status::Running));
        let asking = steps.iter().any(|step| step.ask().is_some());
        let open = self
            .groups
            .get(&key)
            .copied()
            .unwrap_or(tail || busy || asking);
        let failed = steps
            .iter()
            .filter(|step| step.status() == Status::Failed)
            .count();
        let (add, del) = steps
            .iter()
            .filter(|step| step.kind() == StepKind::Edit)
            .fold((0, 0), |(add, del), step| {
                let (more, less) = step.lines();
                (add + more, del + less)
            });
        let clauses = tally(steps);
        let first = clauses.first().map_or(0, String::len);

        let toggle = view.clone();
        let head = h_flex()
            .id(ElementId::Name(
                format!("steps-{}-{entry}-{at}", chat.id()).into(),
            ))
            .gap_2()
            .h(px(28.))
            .px_2()
            .mx(px(-8.))
            .rounded(px(6.))
            .text_xs()
            .text_color(theme.muted_foreground)
            .cursor_pointer()
            .hover(|head| head.bg(theme.muted))
            .on_click(move |_, _, cx| {
                toggle
                    .update(cx, |this, cx| {
                        this.groups.insert(key, !open);
                        cx.notify();
                    })
                    .ok();
            })
            .child(
                div()
                    .w(px(RAIL))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(
                        Icon::new(if open {
                            Glyph::ChevronDown
                        } else {
                            Glyph::ChevronRight
                        })
                        .size(px(12.)),
                    ),
            )
            .child(div().flex_1().min_w_0().truncate().child(
                StyledText::new(clauses.join(", ")).with_highlights([(
                    0..first,
                    HighlightStyle {
                        color: Some(bright(cx)),
                        font_weight: Some(FontWeight::MEDIUM),
                        ..Default::default()
                    },
                )]),
            ))
            .when(add + del > 0, |head| {
                head.child(div().text_color(theme.success).child(format!("+{add}")))
                    .child(div().text_color(theme.danger).child(format!("−{del}")))
            })
            .when(failed > 0, |head| {
                head.child(
                    div()
                        .text_color(theme.danger)
                        .child(format!("{failed} failed")),
                )
            })
            .when(busy, |head| head.child(Spinner::new().xsmall()));
        let count = steps.len();
        let list = open.then(|| {
            v_flex().children(steps.iter().enumerate().map(|(n, step)| {
                let rail = Rail {
                    above: n > 0,
                    below: n + 1 < count,
                };
                self.step(chat, step, live, rail, view, cx)
            }))
        });
        v_flex().child(head).children(list).into_any_element()
    }

    // One step in a line, under the words of the agent: smaller and greyer than them, its glyph in
    // the color of how it went. A command reads in mono, the programs it runs brighter; a file, as
    // the verb and its path. It opens on its change, or on the command and what it printed. On a
    // group's rail, the line goes up and down from its glyph.
    fn step(
        &self,
        chat: &Chat,
        step: &Step,
        live: bool,
        rail: Rail,
        view: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let (muted, bright, faint) = (theme.muted_foreground, bright(cx), faint(cx));
        let key = (chat.id(), step.id().to_owned());
        let open = self.open.contains(&key);
        let command = (step.kind() == StepKind::Run)
            .then(|| step.target())
            .flatten()
            .map(str::to_owned);
        let foldable =
            !step.diffs().is_empty() || !step.output().trim().is_empty() || command.is_some();
        let running = live && matches!(step.status(), Status::Pending | Status::Running);
        let ask = step.ask().cloned();
        let failed = step.status() == Status::Failed;
        let voice = if ask.is_some() { theme.warning } else { muted };

        let lead: AnyElement = if running && ask.is_none() {
            Spinner::new().xsmall().into_any_element()
        } else {
            let (glyph, tint) = if ask.is_some() {
                (step_glyph(step.kind()), theme.warning)
            } else if failed {
                (Glyph::CircleAlert, theme.danger)
            } else {
                (step_glyph(step.kind()), faint)
            };
            Icon::new(glyph)
                .size(px(13.))
                .text_color(tint)
                .into_any_element()
        };
        let mono = |text: String| {
            div()
                .min_w_0()
                .truncate()
                .font_family(theme.mono_font_family.clone())
                .text_size(px(11.5))
                .text_color(faint)
                .child(text)
        };
        let label: AnyElement = match label(step, chat) {
            Label::Command(command) => {
                let program = if running { theme.foreground } else { bright };
                let marks = command_marks(&command).into_iter().map(|(range, mark)| {
                    let color = match mark {
                        Mark::Program => program,
                        Mark::Flag => faint,
                    };
                    (
                        range,
                        HighlightStyle {
                            color: Some(color),
                            ..Default::default()
                        },
                    )
                });
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(theme.mono_font_family.clone())
                    .text_size(px(11.5))
                    .text_color(muted)
                    .child(StyledText::new(command).with_highlights(marks))
                    .into_any_element()
            }
            Label::Path { verb, path } => {
                let base = path.rfind('/').map_or(0, |at| at + 1);
                let end = path.len();
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1p5()
                    .when(!verb.is_empty(), |line| {
                        line.child(div().flex_shrink_0().text_color(voice).child(verb))
                    })
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(theme.mono_font_family.clone())
                            .text_size(px(11.5))
                            .text_color(faint)
                            .child(StyledText::new(path).with_highlights([(
                                base..end,
                                HighlightStyle {
                                    color: Some(bright),
                                    ..Default::default()
                                },
                            )])),
                    )
                    .into_any_element()
            }
            Label::Plain { title, target } => h_flex()
                .flex_1()
                .min_w_0()
                .gap_1p5()
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(relative(0.6))
                        .truncate()
                        .text_color(voice)
                        .child(title),
                )
                .children(target.map(mono))
                .into_any_element(),
        };
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
        let mut row = h_flex()
            .id(ElementId::Name(
                format!("step-{}-{}", chat.id(), step.id()).into(),
            ))
            .gap_2()
            .min_h(px(26.))
            .px_2()
            .mx(px(-8.))
            .rounded(px(6.))
            .text_xs()
            .text_color(muted)
            .child(
                div()
                    .w(px(RAIL))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(lead),
            )
            .child(label)
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
                    .size(px(12.))
                    .text_color(faint),
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

        // A command still running shows the last line it printed.
        let tail = (running && !open)
            .then(|| {
                unfence(step.output())
                    .lines()
                    .rev()
                    .find(|line| !line.trim().is_empty())
            })
            .flatten()
            .map(|line| {
                div()
                    .ml(px(RAIL + 8.))
                    .mb_1()
                    .truncate()
                    .font_family(theme.mono_font_family.clone())
                    .text_size(px(11.))
                    .text_color(faint)
                    .child(line.trim_end().to_owned())
            });

        let body: Option<AnyElement> = (open && foldable).then(|| {
            if !step.diffs().is_empty() {
                v_flex()
                    .ml(px(RAIL + 8.))
                    .mb_2()
                    .gap_2()
                    .children(step.diffs().iter().map(|diff| {
                        let lines = diff_lines(diff.old.as_deref().unwrap_or(""), &diff.new);
                        v_flex()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .overflow_hidden()
                            .font_family(theme.mono_font_family.clone())
                            .text_size(px(11.5))
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .text_color(muted)
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
                                            .text_color(muted)
                                            .child(mark),
                                    )
                                    .child(div().whitespace_nowrap().child(text))
                            }))
                    }))
                    .into_any_element()
            } else if let Some(command) = command.clone() {
                // The command whole, as typed in a terminal, and what it printed under it.
                let copy = command.clone();
                let output = step.output().trim_end();
                v_flex()
                    .ml(px(RAIL + 8.))
                    .mb_2()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .overflow_hidden()
                    .font_family(theme.mono_font_family.clone())
                    .text_size(px(11.5))
                    .child(
                        h_flex()
                            .items_start()
                            .gap_2()
                            .pl_3()
                            .pr_1()
                            .py_1()
                            .child(div().py(px(3.)).text_color(theme.success).child("$"))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .py(px(3.))
                                    .text_color(bright)
                                    .child(command),
                            )
                            .child(
                                Button::new(ElementId::Name(
                                    format!("copy-{}-{}", chat.id(), step.id()).into(),
                                ))
                                .ghost()
                                .xsmall()
                                .icon(Glyph::Copy)
                                .tooltip("Copy command")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                                }),
                            ),
                    )
                    .when(!output.is_empty(), |term| {
                        term.child(div().px_3().pb_2().text_color(muted).child(cut(output)))
                    })
                    .into_any_element()
            } else {
                div()
                    .ml(px(RAIL + 8.))
                    .mb_2()
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .font_family(theme.mono_font_family.clone())
                    .text_size(px(11.5))
                    .text_color(muted)
                    .child(cut(step.output()))
                    .into_any_element()
            }
        });

        // The rail: from the glyph up to the step before, and down past what this one shows to
        // the step after.
        let line = |line: Div| {
            line.absolute()
                .left(px(RAIL_X))
                .w(px(1.))
                .bg(theme.muted_foreground.opacity(0.3))
        };
        v_flex()
            .relative()
            .child(row)
            .children(tail)
            .children(body)
            .when(rail.above, |item| item.child(line(div().top_0().h(px(5.)))))
            .when(rail.below, |item| {
                item.child(line(div().top(px(21.)).bottom_0()))
            })
            .into_any_element()
    }
}

impl Render for ChatWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.agents.read(cx).chats().active_id();
        if active != self.shown {
            self.shown = active;
            self.scroll.scroll_to_bottom();
        }
        let list = self.panel.then(|| self.list(cx));
        div()
            .relative()
            .size_full()
            .child(self.conversation(cx))
            .children(list)
    }
}

// How a chat is doing, under its title.
fn status(chat: &Chat, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let line = h_flex()
        .gap_1p5()
        .min_w_0()
        .text_xs()
        .text_color(theme.muted_foreground);
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
                .map_or("Thinking".to_owned(), |step| one_line(step.title()));
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
            .child(div().truncate().child(one_line(&reply.summary())))
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

// A message too long to show whole at first: more lines than shown folded, or text enough to
// wrap past them.
fn is_long(text: &str) -> bool {
    text.lines().count() > FOLDED || text.chars().count() > FOLDED * 100
}

// The column of a step's glyph, and where the rail of its group runs down it.
const RAIL: f32 = 15.;
const RAIL_X: f32 = 7.;

// Where a step sits on its group's rail: the line goes up from its glyph to the step before, and
// down to the one after. A step alone has none.
#[derive(Clone, Copy, Default)]
struct Rail {
    above: bool,
    below: bool,
}

// The words of the agent: what the chat is read for, a size over its steps and brighter than
// them, with code in them in mono.
fn prose(id: ElementId, text: &str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .text_size(px(14.))
        .line_height(relative(1.6))
        .text_color(theme.foreground)
        .child(
            TextView::markdown(id, text.to_owned()).inline_code_style(
                StyleRefinement::default()
                    .font_family(theme.mono_font_family.clone())
                    .text_size(px(12.5))
                    .px(px(4.))
                    .rounded(px(4.))
                    .bg(theme.secondary)
                    .text_color(bright(cx)),
            ),
        )
        .into_any_element()
}

// What stands out in a step's line: the program a command runs, the name of a file.
fn bright(cx: &App) -> Hsla {
    cx.theme().foreground.opacity(0.82)
}

// What steps back in it: a flag, the folder a file is in, the glyph.
fn faint(cx: &App) -> Hsla {
    cx.theme().muted_foreground.opacity(0.7)
}

// What a step printed, cut to its first lines.
fn cut(output: &str) -> String {
    let lines: Vec<&str> = unfence(output).lines().collect();
    let mut text = lines[..lines.len().min(OUTPUT)].join("\n");
    if lines.len() > OUTPUT {
        text.push_str(&format!("\n… {} more lines", lines.len() - OUTPUT));
    }
    text
}

// What a step printed without the fence of markdown some agents put it in.
fn unfence(output: &str) -> &str {
    let trimmed = output.trim();
    let Some(body) = trimmed
        .strip_prefix("```")
        .and_then(|rest| rest.strip_suffix("```"))
    else {
        return output;
    };
    // The fence's first line names the language.
    body.split_once('\n')
        .map_or(output, |(_, inner)| inner.trim_end())
}

// How a step reads in its line.
#[derive(Debug, PartialEq, Eq)]
enum Label {
    // A command line, whole.
    Command(String),
    // What it did to a file, and the file, from the chat's folder.
    Path {
        verb: String,
        path: String,
    },
    // Its title, and what it worked on when the title doesn't say it.
    Plain {
        title: String,
        target: Option<String>,
    },
}

fn label(step: &Step, chat: &Chat) -> Label {
    let title = one_line(step.title());
    let target = step.target().map(one_line);
    if step.kind() == StepKind::Run
        && let Some(command) = target
    {
        return Label::Command(command);
    }
    match target {
        Some(target) if title == target => Label::Path {
            verb: String::new(),
            path: from_project(&target, chat),
        },
        Some(target) if title.ends_with(&target) => Label::Path {
            verb: title[..title.len() - target.len()].trim_end().to_owned(),
            path: from_project(&target, chat),
        },
        Some(target) if title.contains(&target) => Label::Plain {
            title,
            target: None,
        },
        target => Label::Plain {
            title,
            target: target.map(|target| from_project(&target, chat)),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Program,
    Flag,
}

// The programs a command line runs and its flags, by where they are in it. A program comes first
// and after each `&&`, `||`, `|` or `;`; a word with `=` before it sets a variable for it.
fn command_marks(command: &str) -> Vec<(Range<usize>, Mark)> {
    let mut marks = Vec::new();
    let mut program = true;
    let mut at = 0;
    for word in command.split(' ') {
        let range = at..at + word.len();
        at = range.end + 1;
        if word.is_empty() {
            continue;
        }
        if matches!(word, "&&" | "||" | "|" | ";") {
            program = true;
        } else if word.starts_with('-') {
            marks.push((range, Mark::Flag));
            program = false;
        } else if program && word.contains('=') {
            continue;
        } else if program {
            marks.push((range, Mark::Program));
            program = word.ends_with(';');
        } else if word.ends_with(';') {
            program = true;
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use corvo_domain::{Chat, Step, StepKind};

    use super::{Label, Mark, command_marks, label, unfence};

    fn programs(command: &str) -> Vec<&str> {
        command_marks(command)
            .into_iter()
            .filter(|(_, mark)| *mark == Mark::Program)
            .map(|(range, _)| &command[range])
            .collect()
    }

    #[test]
    fn a_command_line_marks_each_program_it_runs_and_its_flags() {
        assert_eq!(
            command_marks("cargo build --release -p corvo"),
            [
                (0..5, Mark::Program),
                (12..21, Mark::Flag),
                (22..24, Mark::Flag)
            ]
        );
        assert_eq!(
            programs("cd /tmp && FOO=1 eu-stack -p 7 | grep main; ls"),
            ["cd", "eu-stack", "grep", "ls"]
        );
        assert_eq!(programs("tail -3 a.log; ls -la"), ["tail", "ls"]);
    }

    #[test]
    fn output_in_a_fence_shows_without_it() {
        assert_eq!(unfence("```console\nok\n  done\n```"), "ok\n  done");
        assert_eq!(unfence("```\n```"), "");
        assert_eq!(unfence("plain\n```x```"), "plain\n```x```");
        assert_eq!(unfence("```only"), "```only");
    }

    #[test]
    fn a_step_reads_as_a_command_a_file_or_its_title() {
        let chat = Chat::new(1, "claude", "/p", 0);

        let mut run = Step::new("t1", StepKind::Run, "cargo test -p corvo");
        run.set_target(Some("cargo test -p corvo".to_owned()));
        assert_eq!(
            label(&run, &chat),
            Label::Command("cargo test -p corvo".to_owned())
        );

        let mut read = Step::new("t2", StepKind::Read, "Read /p/src/main.rs");
        read.set_target(Some("/p/src/main.rs".to_owned()));
        assert_eq!(
            label(&read, &chat),
            Label::Path {
                verb: "Read".to_owned(),
                path: "src/main.rs".to_owned()
            }
        );

        let mut search = Step::new("t3", StepKind::Search, "grep \"palette\"");
        search.set_target(Some("palette".to_owned()));
        assert_eq!(
            label(&search, &chat),
            Label::Plain {
                title: "grep \"palette\"".to_owned(),
                target: None
            }
        );

        let waiting = Step::new("t4", StepKind::Run, "Terminal");
        assert_eq!(
            label(&waiting, &chat),
            Label::Plain {
                title: "Terminal".to_owned(),
                target: None
            }
        );
    }
}
