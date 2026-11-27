// What the screens of the git window share: rows of files, names of refs, how far apart two
// branches are, and a patch read off the main thread.

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use corvo_domain::{Change, FileStat, RefName, ago};
use corvo_ui::file_icon::FileIcon;
use corvo_ui::git::tint;
use gpui_kit::component::diff::{DiffFile, DiffState};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, ElementId, Entity, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Stateful, Styled as _,
    Task, div, prelude::FluentBuilder as _, px,
};

// A line in the middle of an empty part of the window.
pub fn note(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .size_full()
        .p_4()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}

// The heading of a group in a list: its name in capitals, a count, and what acts on all of it.
pub fn group(label: &str, count: usize, cx: &App) -> Div {
    let theme = cx.theme();
    h_flex()
        .h(px(30.))
        .pl(px(14.))
        .pr_2()
        .gap_1p5()
        .flex_shrink_0()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.muted_foreground)
        .child(label.to_uppercase())
        .child(
            div()
                .text_color(theme.muted_foreground.opacity(0.6))
                .child(count.to_string()),
        )
        .child(div().flex_1())
}

// A row of a list, picked or not.
pub fn row(id: impl Into<ElementId>, on: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    h_flex()
        .id(id)
        .h(px(28.))
        .mx_1p5()
        .px_2()
        .gap_2()
        .flex_shrink_0()
        .rounded(px(6.))
        .cursor_pointer()
        .text_sm()
        .hover(|row| row.bg(theme.muted))
        .when(on, |row| row.bg(theme.secondary))
}

// The parts of a path inside a repository: the name, and the folder it is in.
pub fn split(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (name.to_owned(), dir.to_owned()),
        None => (path.to_owned(), String::new()),
    }
}

// A file's icon, its name in the color of its change, and its folder.
pub fn file_label(path: &str, change: Change, cx: &App) -> Vec<AnyElement> {
    let theme = cx.theme();
    let (name, dir) = split(path);
    vec![
        FileIcon::new(name.clone()).size(px(14.)).into_any_element(),
        div()
            .flex_shrink_0()
            .text_color(tint(change, cx))
            .child(name)
            .into_any_element(),
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(dir)
            .into_any_element(),
    ]
}

// The lines a file gained and lost, small, in green and red.
pub fn lines(added: usize, removed: usize, cx: &App) -> Div {
    let theme = cx.theme();
    h_flex()
        .flex_shrink_0()
        .gap_1p5()
        .text_size(px(11.))
        .font_family("monospace")
        .child(div().text_color(theme.green).child(format!("+{added}")))
        .child(div().text_color(theme.red).child(format!("−{removed}")))
}

// A row for a file a commit, comparison or stash touched.
pub fn stat_row(id: impl Into<ElementId>, file: &FileStat, on: bool, cx: &App) -> Stateful<Div> {
    let tail = match file.lines {
        Some((added, removed)) => lines(added, removed, cx).into_any_element(),
        None => div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("bin")
            .into_any_element(),
    };
    row(id, on, cx)
        .children(file_label(&file.path, file.change, cx))
        .child(tail)
}

// A name pointing at a commit, framed in the color of its kind.
pub fn ref_pill(name: &RefName, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (color, filled) = match name {
        RefName::Head(_) => (theme.blue, true),
        RefName::Local(_) => (theme.blue, false),
        RefName::Remote(_) => (theme.muted_foreground, false),
        RefName::Tag(_) => (theme.warning, false),
    };
    pill(name.name(), color, filled, cx)
}

// A small framed name.
pub fn pill(text: &str, color: Hsla, filled: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .flex_shrink_0()
        .h(px(18.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .border_1()
        .text_size(px(11.))
        .font_family("monospace")
        .when(filled, |p| {
            p.bg(color)
                .border_color(color)
                .text_color(theme.background)
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!filled, |p| {
            p.border_color(color.opacity(0.45))
                .bg(color.opacity(0.06))
                .text_color(color)
        })
        .child(text.to_owned())
        .into_any_element()
}

// A soft rounded label: a state, as `merged` or `Conflicts with main`.
pub fn badge(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .h(px(18.))
        .px(px(7.))
        .flex()
        .items_center()
        .gap_1()
        .rounded_full()
        .text_size(px(11.))
        .bg(color.opacity(0.12))
        .text_color(color)
        .child(text.into())
}

// How far a branch went from another: ↑ what only it has, ↓ what it lacks.
pub fn apart(ahead: usize, behind: usize, cx: &App) -> Div {
    let theme = cx.theme();
    h_flex()
        .flex_shrink_0()
        .gap_1p5()
        .text_size(px(11.5))
        .font_family("monospace")
        .when(ahead > 0, |a| {
            a.child(div().text_color(theme.green).child(format!("↑{ahead}")))
        })
        .when(behind > 0, |a| {
            a.child(div().text_color(theme.warning).child(format!("↓{behind}")))
        })
}

// How long ago, from seconds since the epoch: `now`, `5m`, `3h`, `2d`, `4w`.
pub fn since(when: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    ago(when.max(0) as u64, now)
}

// A patch shown in a diff, read off the main thread. Reading again what is already shown leaves
// it as it is, scrolled where it was.
pub struct PatchView {
    state: Entity<DiffState>,
    text: Option<String>,
    trouble: Option<String>,
    _reading: Option<Task<()>>,
}

impl PatchView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            state: cx.new(|cx| DiffState::new(Vec::new(), cx)),
            text: None,
            trouble: None,
            _reading: None,
        }
    }

    pub fn state(&self) -> &Entity<DiffState> {
        &self.state
    }

    // The patch shown, as git wrote it.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn trouble(&self) -> Option<&str> {
        self.trouble.as_deref()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self._reading = None;
        self.trouble = None;
        if self.text.take().is_some() {
            self.state
                .update(cx, |diff, cx| diff.set_files(Vec::new(), cx));
        }
        cx.notify();
    }

    pub fn load(
        &mut self,
        read: impl FnOnce() -> io::Result<String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let task = cx.background_executor().spawn(async move { read() });
        self._reading = Some(cx.spawn(async move |this, cx| {
            let patch = task.await;
            this.update(cx, |this, cx| this.show(patch, cx)).ok();
        }));
    }

    fn show(&mut self, patch: io::Result<String>, cx: &mut Context<Self>) {
        if let Ok(patch) = &patch
            && self.text.as_ref() == Some(patch)
            && self.trouble.is_none()
        {
            return;
        }
        let read = patch.map_err(|err| err.to_string()).and_then(|patch| {
            let files = DiffFile::parse(&patch).map_err(|err| err.message().to_string());
            files.map(|files| (patch, files))
        });
        let files = match read {
            Ok((patch, files)) => {
                self.trouble = None;
                self.text = Some(patch);
                files
            }
            Err(why) => {
                self.trouble = Some(why);
                self.text = None;
                Vec::new()
            }
        };
        self.state.update(cx, |diff, cx| diff.set_files(files, cx));
        cx.notify();
    }
}
