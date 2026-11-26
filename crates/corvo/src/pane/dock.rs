use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use corvo_domain::{Chat, Choice, End, Message, Pick};
use corvo_ui::agent::AgentLogo;
use corvo_ui::{field, notice};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::attachment::{Attachment, AttachmentMedia};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Axis, Context, Entity, EventEmitter, ExternalPaths,
    FontWeight, Hsla, IntoElement, ParentElement as _, PathPromptOptions, Render, SharedString,
    Styled as _, Subscription, Task, Window, div, prelude::*, px,
};

use crate::agents::{Agents, now};
use crate::attach::{self, Attached};

// The folder a new chat works in: the project in focus.
pub type Project = Rc<dyn Fn(&App) -> PathBuf>;

pub enum Event {
    // The chat button: the chat window comes, or goes.
    Chat,
}

const WIDTH: f32 = 680.;

// The agent, always in view at the foot of the window: above the bar, with no background, the
// last message of the dev and what the agent is at; the bar with the agent button, the field and
// the chat button.
pub struct Dock {
    agents: Entity<Agents>,
    project: Project,
    field: Entity<field::State>,
    // The images going with the next message.
    images: Vec<Attached>,
    picker: Entity<Picker>,
    picking: bool,
    chatting: bool,
    hint: SharedString,
    _subs: Vec<Subscription>,
    _tick: Task<()>,
}

impl EventEmitter<Event> for Dock {}

impl Dock {
    pub fn new(
        agents: Entity<Agents>,
        project: Project,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| field::state("Ask the agent…", window, cx));
        let picker = cx.new(|_| Picker {
            agents: agents.clone(),
            project: project.clone(),
        });
        let subs = vec![
            cx.subscribe_in(&field, window, Self::on_field),
            cx.observe(&agents, |_, _, cx| cx.notify()),
        ];
        // The time of a reply at work goes on by the second.
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |this, cx| {
                    if this.agents.read(cx).active().is_some_and(Chat::is_working) {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            agents,
            project,
            field,
            images: Vec::new(),
            picker,
            picking: false,
            chatting: false,
            hint: SharedString::default(),
            _subs: subs,
            _tick: tick,
        }
    }

    pub fn set_chatting(&mut self, chatting: bool, cx: &mut Context<Self>) {
        self.chatting = chatting;
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.field.update(cx, |field, cx| field.focus(window, cx));
    }

    fn on_field(
        &mut self,
        _: &Entity<field::State>,
        event: &field::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field::Event::PressEnter { .. } = event else {
            return;
        };
        let text = self.field.read(cx).value().to_string();
        if text.trim().is_empty() && self.images.is_empty() {
            return;
        }
        // While the agent works, what was written stays in the field, and the images with it.
        let project = (self.project)(cx);
        let images = self.images.clone();
        let sent = self
            .agents
            .update(cx, |agents, cx| agents.send(&text, &images, &project, cx));
        if sent {
            self.images.clear();
            self.field
                .update(cx, |field, cx| field.set_value("", window, cx));
        }
    }

    // Images come to the message; those turned away are told.
    fn attach(
        &mut self,
        images: Vec<Attached>,
        refused: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.images.extend(images);
        for why in refused {
            notice::warn(why, window, cx);
        }
        self.focus(window, cx);
        cx.notify();
    }

    fn attach_paths(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let (images, refused) = paths.iter().map(|path| attach::from_path(path)).fold(
            (Vec::new(), Vec::new()),
            |(mut images, mut refused), image| {
                match image {
                    Ok(image) => images.push(image),
                    Err(why) => refused.push(why),
                }
                (images, refused)
            },
        );
        self.attach(images, refused, window, cx);
    }

    // The files picker, for images to attach.
    fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let paths = match paths.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Err(err)) => {
                    this.update_in(cx, |_, window, cx| {
                        notice::error(format!("The file picker did not open: {err}"), window, cx);
                    })
                    .ok();
                    return;
                }
                _ => return,
            };
            this.update_in(cx, |this, window, cx| this.attach_paths(&paths, window, cx))
                .ok();
        })
        .detach();
    }

    fn detach(&mut self, id: u64, cx: &mut Context<Self>) {
        self.images.retain(|image| image.id != id);
        cx.notify();
    }

    // The images going with the message, each to take back out.
    fn attached(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        if self.images.is_empty() {
            return None;
        }
        let tiles = self.images.iter().map(|image| {
            let id = image.id;
            Attachment::new()
                .id(("attached", id))
                .axis(Axis::Vertical)
                .media(AttachmentMedia::new().src(image.preview.clone()))
                .tooltip(image.name.clone())
                .on_remove(cx.listener(move |this, _, _, cx| this.detach(id, cx)))
        });
        Some(
            h_flex()
                .flex_wrap()
                .gap_2()
                .px(px(10.))
                .pt(px(10.))
                .children(tiles),
        )
    }

    fn pick(&mut self, open: bool, cx: &mut Context<Self>) {
        self.picking = open;
        if open {
            let project = (self.project)(cx);
            self.agents
                .update(cx, |agents, cx| agents.open(&project, cx));
        }
        cx.notify();
    }

    // What the dev asked last, and what came of it so far.
    fn feed(&self, agents: &Agents, cx: &App) -> Option<AnyElement> {
        if self.chatting || self.picking {
            return None;
        }
        let theme = cx.theme();
        let chat = agents.active()?;
        let ask = chat.last_ask()?;
        let reply = chat.last_reply()?;
        let you = h_flex()
            .gap_2()
            .min_w_0()
            .text_color(theme.muted_foreground)
            .child(lead(
                Icon::new(Glyph::ChevronRight)
                    .size(px(12.))
                    .text_color(theme.muted_foreground),
            ))
            .child(div().truncate().child(said(ask)));

        let (lead_el, verb, detail, time, tint): (AnyElement, String, String, Option<u64>, Hsla) =
            match reply.end() {
                None if chat.is_waiting() => (
                    dot(theme.warning).into_any_element(),
                    "Waiting for you".to_owned(),
                    reply
                        .asking()
                        .map(|step| step.title().to_owned())
                        .unwrap_or_default(),
                    None,
                    theme.warning,
                ),
                None => {
                    let current = reply.current().filter(|_| !agents.is_thinking(chat.id()));
                    (
                        Spinner::new().xsmall().into_any_element(),
                        current.map_or("Thinking".to_owned(), |step| step.title().to_owned()),
                        current
                            .and_then(|step| step.target())
                            .map(|target| relative(target, chat))
                            .unwrap_or_default(),
                        Some(now().saturating_sub(reply.started())),
                        theme.foreground,
                    )
                }
                Some(End::Done) => (
                    Icon::new(Glyph::Check)
                        .size(px(12.))
                        .text_color(theme.success)
                        .into_any_element(),
                    "Done".to_owned(),
                    reply.summary(),
                    Some(reply.secs()),
                    theme.muted_foreground,
                ),
                Some(End::Stopped) => (
                    Icon::new(Glyph::Square)
                        .size(px(11.))
                        .text_color(theme.muted_foreground)
                        .into_any_element(),
                    "Stopped".to_owned(),
                    String::new(),
                    Some(reply.secs()),
                    theme.muted_foreground,
                ),
                Some(End::Failed(why)) => (
                    Icon::new(Glyph::CircleAlert)
                        .size(px(12.))
                        .text_color(theme.danger)
                        .into_any_element(),
                    "Failed".to_owned(),
                    why.clone(),
                    None,
                    theme.danger,
                ),
            };
        let now_line = h_flex()
            .gap_2()
            .min_w_0()
            .child(lead(lead_el))
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(tint)
                    .child(verb),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family("monospace")
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(detail),
            )
            .child(div().flex_1())
            .children(time.map(|secs| {
                div()
                    .flex_shrink_0()
                    .text_color(theme.muted_foreground)
                    .child(clock(secs))
            }));
        Some(
            v_flex()
                .px_3()
                .gap_1p5()
                .text_xs()
                .child(you)
                .child(now_line)
                .into_any_element(),
        )
    }

    fn agent_button(&self, agents: &Agents, cx: &App) -> Button {
        let theme = cx.theme();
        let agent = agents.agent().to_owned();
        let options = agents.options();
        let model = options
            .and_then(|options| options.model.as_ref())
            .map_or_else(
                || agents.agent_name(&agent),
                |model| model.name().to_owned(),
            );
        let effort = options
            .and_then(|options| options.effort.as_ref())
            .map(|effort| effort.name().to_owned());
        let mode = options
            .and_then(|options| options.mode.as_ref())
            .and_then(Choice::current)
            .cloned();
        let content = h_flex()
            .gap_2()
            .child(AgentLogo::new(agent))
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(model))
            .children(effort.map(|effort| {
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(effort)
            }))
            .children(mode.map(|mode| pill(&mode, cx)))
            .child(
                Icon::new(Glyph::ChevronUp)
                    .size(px(12.))
                    .text_color(theme.muted_foreground),
            );
        Button::new("agent")
            .ghost()
            .small()
            .selected(self.picking)
            .child(content)
    }
}

impl Render for Dock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let agents = self.agents.read(cx);
        let hint: SharedString = format!("Ask {}…", agents.agent_name(agents.agent())).into();
        if hint != self.hint {
            self.hint = hint.clone();
            self.field
                .update(cx, |field, cx| field.set_placeholder(hint, window, cx));
        }
        let agents = self.agents.read(cx);
        let theme = cx.theme();
        let feed = self.feed(agents, cx);
        let working = agents.active().is_some_and(Chat::is_working);
        let waits = agents.someone_waits();

        // It opens from the button, clear of the bar, an arrow pointing back at it.
        let view = cx.entity().downgrade();
        let picker = self.picker.clone();
        let picker = Popover::new("agent-picker")
            .anchor(gpui_kit::Anchor::BottomLeft)
            .offset(px(14.))
            .arrow(true)
            .open(self.picking)
            .on_open_change({
                let view = view.clone();
                move |open, _, cx| {
                    view.update(cx, |this, cx| this.pick(*open, cx)).ok();
                }
            })
            .trigger(self.agent_button(agents, cx))
            .content(move |_, _, _| picker.clone());

        let stop = working.then(|| {
            let agents = self.agents.clone();
            Button::new("stop")
                .ghost()
                .small()
                .icon(Glyph::Square)
                .tooltip("Stop")
                .on_click(move |_, _, cx| {
                    agents.update(cx, |agents, cx| agents.stop(cx));
                })
        });
        let chat = div()
            .relative()
            .child(
                Button::new("chat")
                    .ghost()
                    .small()
                    .icon(Glyph::MessageSquare)
                    .selected(self.chatting)
                    .tooltip(if self.chatting {
                        "Close the chat"
                    } else {
                        "Open the chat"
                    })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |_, cx| cx.emit(Event::Chat)).ok();
                    }),
            )
            .children(waits.then(|| {
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(5.))
                    .size(px(7.))
                    .rounded_full()
                    .bg(theme.warning)
                    .border_1()
                    .border_color(theme.background)
            }));

        // A paste with images keeps them beside the field; one with none goes in as text.
        let field = Input::new(&self.field).appearance(false).on_paste({
            let view = cx.entity().downgrade();
            move |item, window, cx| {
                let Some((images, refused)) = attach::pasted(item) else {
                    return false;
                };
                view.update(cx, |this, cx| this.attach(images, refused, window, cx))
                    .ok();
                true
            }
        });
        let clip = Button::new("attach")
            .ghost()
            .small()
            .icon(Glyph::Paperclip)
            .tooltip("Attach images")
            .on_click(cx.listener(|this, _, window, cx| this.browse(window, cx)));
        let row = h_flex()
            .h(px(48.))
            .px(px(7.))
            .gap_1()
            .child(picker)
            .child(div().w(px(1.)).h(px(20.)).mx_1().bg(theme.border))
            .child(div().flex_1().min_w_0().child(field))
            .child(clip)
            .children(stop)
            .child(chat);
        let attached = self.attached(cx);
        let theme = cx.theme();
        let ring = theme.ring;
        let bar = v_flex()
            .id("agent-bar")
            .rounded(px(12.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            // Image files dropped on the bar go with the message.
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(ring))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.attach_paths(paths.paths(), window, cx);
            }))
            .children(attached)
            .child(row);

        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(20.))
            .flex()
            .justify_center()
            .child(v_flex().w(px(WIDTH)).gap_2().children(feed).child(bar))
    }
}

// What the dev can pick for the agent: which agent, and the model, effort and mode it offers.
pub struct Picker {
    agents: Entity<Agents>,
    project: Project,
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let agents = self.agents.read(cx);
        let current = agents.agent().to_owned();

        // Only the agents the dev has, and the chat's own even when it is gone.
        let rows = agents
            .hub()
            .specs()
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.id() == current || agents.hub().available(spec.id()));
        let rows = rows.map(|(ix, spec)| {
            let id = spec.id().to_owned();
            let on = id == current;
            let missing = !agents.hub().available(&id);
            let (owner, project) = (self.agents.clone(), self.project.clone());
            div()
                .id(("agent", ix))
                .flex()
                .items_center()
                .gap_2()
                .h(px(40.))
                .px_2()
                .rounded(px(8.))
                .cursor_pointer()
                .hover(|row| row.bg(theme.muted))
                .when(on, |row| row.bg(theme.muted))
                .on_click(move |_, _, cx| {
                    let project = project(cx);
                    owner.update(cx, |agents, cx| agents.pick_agent(&id, &project, cx));
                })
                .child(AgentLogo::new(spec.id().to_owned()))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(spec.name().to_owned()),
                        )
                        .children(missing.then(|| {
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Not installed")
                        })),
                )
                .children(on.then(|| {
                    Icon::new(Glyph::Check)
                        .size(px(14.))
                        .text_color(theme.primary)
                }))
        });

        let name = agents.agent_name(&current);
        let mut opts = v_flex().flex_1().min_w_0().p_2().gap_4();
        if !agents.hub().available(&current) {
            let hint = agents
                .hub()
                .spec(&current)
                .map(|spec| spec.install().to_owned())
                .unwrap_or_default();
            opts = opts
                .child(div().text_sm().child(format!("{name} is not installed.")))
                .child(
                    div()
                        .p_2()
                        .rounded(px(6.))
                        .bg(theme.muted)
                        .font_family("monospace")
                        .text_xs()
                        .child(hint),
                );
        } else if let Some(options) = agents.options() {
            if let Some(model) = &options.model {
                opts = opts.child(section("Model", cx).child(models(model, &self.agents, cx)));
            }
            if let Some(effort) = &options.effort {
                opts = opts.child(section("Effort", cx).child(segments(effort, &self.agents, cx)));
            }
            if let Some(mode) = &options.mode {
                opts = opts.child(section("Mode", cx).child(modes(mode, &self.agents, cx)));
            }
            if options.model.is_none() && options.effort.is_none() && options.mode.is_none() {
                opts = opts.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(format!("{name} has nothing to pick.")),
                );
            }
        } else if agents.is_opening() {
            opts = opts.child(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(Spinner::new().xsmall())
                    .child(format!("Starting {name}…")),
            );
        } else {
            opts = opts.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{name} did not start. Write a message to try again."
                    )),
            );
        }
        let joined = agents.active().is_some_and(|chat| chat.turns() > 0);

        v_flex().w(px(572.)).child(
            h_flex()
                .items_start()
                .child(
                    v_flex()
                        .w(px(196.))
                        .p_2()
                        .gap_0p5()
                        .border_r_1()
                        .border_color(theme.border)
                        .child(label("Agent", cx))
                        .children(rows)
                        .children(joined.then(|| {
                            div()
                                .mt_3()
                                .px_2()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Another agent goes on in this chat, with what was said so far.")
                        })),
                )
                .child(opts),
        )
    }
}

fn section(title: &str, cx: &App) -> gpui_kit::Div {
    v_flex().gap_1().child(label(title, cx))
}

fn label(title: &str, cx: &App) -> impl IntoElement + use<> {
    div()
        .px_2()
        .pb_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(title.to_uppercase())
}

fn models(choice: &Choice, agents: &Entity<Agents>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let rows = choice.values.iter().enumerate().map(|(ix, pick)| {
        let on = pick.id == choice.current;
        let (agents, option, value) = (agents.clone(), choice.id.clone(), pick.id.clone());
        div()
            .id(("model", ix))
            .flex()
            .items_center()
            .gap_2()
            .h(px(30.))
            .px_2()
            .rounded(px(6.))
            .cursor_pointer()
            .text_sm()
            .text_color(if on {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .hover(|row| row.bg(theme.muted))
            .on_click(move |_, _, cx| {
                agents.update(cx, |agents, cx| agents.set(&option, &value, cx));
            })
            .child(
                div()
                    .size(px(12.))
                    .flex_shrink_0()
                    .rounded_full()
                    .border_1()
                    .border_color(if on { theme.primary } else { theme.border })
                    .when(on, |dot| dot.bg(theme.primary)),
            )
            .child(div().truncate().child(pick.name.clone()))
    });
    div().grid().grid_cols(2).gap_0p5().children(rows)
}

fn segments(choice: &Choice, agents: &Entity<Agents>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let parts = choice.values.iter().enumerate().map(|(ix, pick)| {
        let on = pick.id == choice.current;
        let (agents, option, value) = (agents.clone(), choice.id.clone(), pick.id.clone());
        div()
            .id(("effort", ix))
            .flex_1()
            .h(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .text_xs()
            .text_color(if on {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .when(on, |part| part.bg(theme.muted))
            .on_click(move |_, _, cx| {
                agents.update(cx, |agents, cx| agents.set(&option, &value, cx));
            })
            .child(pick.name.clone())
    });
    h_flex()
        .mx_2()
        .p_0p5()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .children(parts)
}

fn modes(choice: &Choice, agents: &Entity<Agents>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let rows = choice.values.iter().enumerate().map(|(ix, pick)| {
        let on = pick.id == choice.current;
        let tint = if pick.is_risky() {
            theme.warning
        } else if on {
            theme.primary
        } else {
            theme.muted_foreground
        };
        let (agents, option, value) = (agents.clone(), choice.id.clone(), pick.id.clone());
        div()
            .id(("mode", ix))
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .py(px(7.))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|row| row.bg(theme.muted))
            .when(on, |row| row.bg(theme.primary.opacity(0.12)))
            .on_click(move |_, _, cx| {
                agents.update(cx, |agents, cx| agents.set(&option, &value, cx));
            })
            .child(
                div()
                    .size(px(26.))
                    .flex_shrink_0()
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.muted)
                    .child(Icon::new(mode_glyph(pick)).size(px(14.)).text_color(tint)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().child(pick.name.clone()))
                    .children(pick.description.clone().map(|text| {
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(text)
                    })),
            )
    });
    v_flex().gap_0p5().children(rows)
}

// The mode on the agent button, amber when it goes on without asking.
fn pill(mode: &Pick, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let risky = mode.is_risky();
    h_flex()
        .gap_1()
        .h(px(18.))
        .px(px(6.))
        .rounded(px(4.))
        .text_xs()
        .when(risky, |pill| {
            pill.text_color(theme.warning)
                .bg(theme.warning.opacity(0.12))
        })
        .when(!risky, |pill| {
            pill.text_color(theme.muted_foreground)
                .border_1()
                .border_color(theme.border)
        })
        .child(Icon::new(mode_glyph(mode)).size(px(10.)))
        .child(mode.name.clone())
}

fn mode_glyph(mode: &Pick) -> Glyph {
    let words = format!("{} {}", mode.id, mode.name).to_lowercase();
    if mode.is_risky() {
        Glyph::Zap
    } else if words.contains("plan") {
        Glyph::Map
    } else if words.contains("edit") {
        Glyph::Pencil
    } else if words.contains("auto") {
        Glyph::Sparkles
    } else if words.contains("read") {
        Glyph::Eye
    } else {
        Glyph::Hand
    }
}

fn lead(child: impl IntoElement) -> impl IntoElement {
    div()
        .w(px(14.))
        .flex_shrink_0()
        .flex()
        .justify_center()
        .child(child)
}

fn dot(color: Hsla) -> impl IntoElement {
    div().size(px(6.)).rounded_full().bg(color)
}

// The message in a line: its words, and how many images went with it.
fn said(message: &Message) -> String {
    let images = match message.images.len() {
        0 => return message.text.clone(),
        1 => "1 image".to_owned(),
        n => format!("{n} images"),
    };
    if message.text.is_empty() {
        images
    } else {
        format!("{} · {images}", message.text)
    }
}

pub fn clock(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

// A path the agent touched, from the chat's folder when it is inside it.
pub fn relative(target: &str, chat: &Chat) -> String {
    std::path::Path::new(target)
        .strip_prefix(chat.project())
        .map_or_else(|_| target.to_owned(), |path| path.display().to_string())
}
