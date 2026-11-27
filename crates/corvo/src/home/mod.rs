mod detail;
mod facts;
mod list;

use std::path::{Path, PathBuf};

use corvo_domain::{Uuid, Workspaces};
use corvo_fs::JsonChats;
use corvo_ui::keys::{
    Dismiss, NewWorkspace, OpenPicked, PickNext, PickPrevious, RemovePicked, RenamePicked,
    WORKSPACES,
};
use corvo_ui::{field, notice};
use gpui_kit::component::h_flex;
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, PathPromptOptions, Render, Styled as _, Subscription, Window,
};

pub use facts::Facts;

pub enum Event {
    Enter(Uuid),
}

// The list of workspaces the window opens on: the workspaces down the left, and the one picked
// on the right, with its projects, their stacks and their branches. Picking one only shows it;
// Enter, its Open or a double click opens it.
pub struct Home {
    spaces: Entity<Workspaces>,
    facts: Entity<Facts>,
    picked: Option<Uuid>,
    name: Entity<field::State>,
    // The name of the picked workspace, turned into a field.
    renaming: Option<(Uuid, Entity<field::State>, Subscription)>,
    // The workspace asked about being removed, and how many chats would go with it.
    removing: Option<(Uuid, usize)>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for Home {}

impl Home {
    pub fn new(
        spaces: Entity<Workspaces>,
        facts: Entity<Facts>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| field::state("New workspace", window, cx));
        let subs = vec![
            cx.subscribe_in(&name, window, Self::on_name),
            cx.observe(&facts, |_, _, cx| cx.notify()),
        ];
        let (picked, projects, empty) = {
            let spaces = spaces.read(cx);
            let picked = spaces
                .last()
                .or_else(|| spaces.items().first().map(|space| space.id()));
            let projects: Vec<PathBuf> = spaces
                .items()
                .iter()
                .flat_map(|space| space.projects().iter().cloned())
                .collect();
            (picked, projects, spaces.items().is_empty())
        };
        facts.update(cx, |facts, cx| facts.weigh(&projects, cx));
        let focus = cx.focus_handle();
        // With no workspace yet, the cursor waits in the field that adds one.
        if empty {
            name.update(cx, |state, cx| state.focus(window, cx));
        } else {
            focus.focus(window, cx);
        }
        let home = Self {
            spaces,
            facts,
            picked,
            name,
            renaming: None,
            removing: None,
            focus,
            _subs: subs,
        };
        home.look(cx);
        home
    }

    fn on_name(
        &mut self,
        _: &Entity<field::State>,
        event: &field::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, field::Event::PressEnter { .. }) {
            self.add(window, cx);
        }
    }

    fn projects(&self, id: Uuid, cx: &Context<Self>) -> Vec<PathBuf> {
        self.spaces
            .read(cx)
            .get(id)
            .map(|space| space.projects().to_vec())
            .unwrap_or_default()
    }

    // Reads again where the projects of the picked workspace stand in git.
    fn look(&self, cx: &mut Context<Self>) {
        let Some(id) = self.picked else {
            return;
        };
        let projects = self.projects(id, cx);
        self.facts.update(cx, |facts, cx| facts.look(&projects, cx));
    }

    fn pick(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.picked == Some(id) {
            return;
        }
        self.picked = Some(id);
        self.renaming = None;
        self.removing = None;
        if let Err(err) = self.spaces.update(cx, |spaces, _| spaces.pick(id)) {
            notice::error(err.to_string(), window, cx);
        }
        self.look(cx);
        cx.notify();
    }

    // Picks the workspace `by` places above or below the picked one, stopping at the ends.
    fn step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<Uuid> = self
            .spaces
            .read(cx)
            .items()
            .iter()
            .map(|space| space.id())
            .collect();
        let Some(last) = ids.len().checked_sub(1) else {
            return;
        };
        let at = self
            .picked
            .and_then(|id| ids.iter().position(|known| *known == id));
        let next = match at {
            Some(at) => at.saturating_add_signed(by).min(last),
            None => 0,
        };
        self.pick(ids[next], window, cx);
    }

    // Opens the workspace, with only this project on its home when one is given.
    fn open(
        &mut self,
        id: Uuid,
        alone: Option<&Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused = match alone {
            Some(project) => self
                .spaces
                .update(cx, |spaces, _| spaces.focus(id, Some(project))),
            None => Ok(()),
        };
        if let Err(err) = focused {
            notice::error(err.to_string(), window, cx);
        }
        if let Err(err) = self.spaces.update(cx, |spaces, _| spaces.pick(id)) {
            notice::error(err.to_string(), window, cx);
        }
        cx.emit(Event::Enter(id));
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value();
        match self.spaces.update(cx, |spaces, _| spaces.add(&name)) {
            Ok(id) => {
                self.name
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.pick(id, window, cx);
                self.focus.focus(window, cx);
            }
            Err(err) => notice::error(err.to_string(), window, cx),
        }
        cx.notify();
    }

    // Asks first, telling how many chats would go with it.
    fn ask_remove(&mut self, id: Uuid, cx: &mut Context<Self>) {
        let chats = JsonChats::config()
            .and_then(|chats| chats.count(id))
            .unwrap_or(0);
        self.renaming = None;
        self.removing = Some((id, chats));
        cx.notify();
    }

    // The workspace goes with its chats; the one after it, or else before it, is picked.
    fn remove(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        self.removing = None;
        let next = {
            let items = self.spaces.read(cx).items();
            let at = items.iter().position(|space| space.id() == id);
            at.and_then(|at| {
                items
                    .get(at + 1)
                    .or_else(|| at.checked_sub(1).map(|i| &items[i]))
            })
            .map(|space| space.id())
        };
        if let Err(err) = self.spaces.update(cx, |spaces, _| spaces.remove(id)) {
            notice::error(err.to_string(), window, cx);
            return;
        }
        if let Err(err) = JsonChats::config().and_then(|chats| chats.forget(id)) {
            notice::error(format!("chats: {err}"), window, cx);
        }
        self.picked = None;
        match next {
            Some(next) => self.pick(next, window, cx),
            None => self.name.update(cx, |state, cx| state.focus(window, cx)),
        }
        cx.notify();
    }

    fn rename(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self
            .spaces
            .read(cx)
            .get(id)
            .map(|space| space.name().to_owned())
        else {
            return;
        };
        self.removing = None;
        let state = cx.new(|cx| field::state("Workspace name", window, cx));
        state.update(cx, |state, cx| {
            state.set_value(name, window, cx);
            state.select_all(window, cx);
            state.focus(window, cx);
        });
        let sub = cx.subscribe_in(&state, window, |this, _, event, window, cx| {
            if matches!(event, field::Event::PressEnter { .. } | field::Event::Blur) {
                this.renamed(window, cx);
            }
        });
        self.renaming = Some((id, state, sub));
        cx.notify();
    }

    // The name typed in goes to the workspace; a blank one leaves it as it was.
    fn renamed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, state, _)) = self.renaming.take() else {
            return;
        };
        let name = state.read(cx).value().to_string();
        if !name.trim().is_empty()
            && let Err(err) = self.spaces.update(cx, |spaces, _| spaces.rename(id, &name))
        {
            notice::error(err.to_string(), window, cx);
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    // Puts away a question or a name being typed, leaving things as they were.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.take().is_some() {
            self.focus.focus(window, cx);
        }
        self.removing = None;
        cx.notify();
    }

    // Asks for a folder, then links it to the workspace, or puts it in place of a project that
    // was moved.
    fn browse(
        &mut self,
        id: Uuid,
        moved: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(if moved.is_some() { "Locate" } else { "Link" }.into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = picked.await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.place(id, moved.as_deref(), path, window, cx);
                    }
                }
                Ok(Ok(None)) | Err(_) => {}
                Ok(Err(err)) => notice::error(err.to_string(), window, cx),
            })
            .ok();
        })
        .detach();
    }

    fn place(
        &mut self,
        id: Uuid,
        moved: Option<&Path>,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placed = self.spaces.update(cx, |spaces, _| match moved {
            Some(from) => spaces.relocate(id, from, &path),
            None => spaces.link(id, &path),
        });
        if let Err(err) = placed {
            notice::error(err.to_string(), window, cx);
            return;
        }
        let projects = [path];
        self.facts.update(cx, |facts, cx| {
            facts.forget(&projects[0]);
            facts.weigh(&projects, cx);
            facts.look(&projects, cx);
        });
        cx.notify();
    }

    fn unlink(&mut self, id: Uuid, project: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(err) = self
            .spaces
            .update(cx, |spaces, _| spaces.unlink(id, project))
        {
            notice::error(err.to_string(), window, cx);
        }
        cx.notify();
    }

    // The keys of the list act only while it has them, not while a field is being typed in.
    fn listening(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
}

impl Render for Home {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = self.side(window, cx);
        let detail = self.detail(cx);
        h_flex()
            .id("workspaces")
            .key_context(WORKSPACES)
            .track_focus(&self.focus)
            .size_full()
            .items_stretch()
            .on_action(cx.listener(|this, _: &PickPrevious, window, cx| {
                if this.listening(window) {
                    this.step(-1, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PickNext, window, cx| {
                if this.listening(window) {
                    this.step(1, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenPicked, window, cx| {
                if this.listening(window)
                    && let Some(id) = this.picked
                {
                    match this.removing {
                        Some((asked, _)) if asked == id => this.remove(id, window, cx),
                        _ => this.open(id, None, window, cx),
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &RemovePicked, window, cx| {
                if this.listening(window)
                    && let Some(id) = this.picked
                {
                    this.ask_remove(id, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &RenamePicked, window, cx| {
                if this.listening(window)
                    && let Some(id) = this.picked
                {
                    this.rename(id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewWorkspace, window, cx| {
                this.cancel(window, cx);
                this.name.update(cx, |state, cx| state.focus(window, cx));
            }))
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                this.cancel(window, cx);
            }))
            .child(side)
            .child(detail)
    }
}

// A path with the home folder as `~`.
fn tilde(path: &Path) -> String {
    if let Some(home) = std::env::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return Path::new("~").join(rest).display().to_string();
    }
    path.display().to_string()
}

fn count(n: usize, one: &str, many: &str) -> String {
    match n {
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}
