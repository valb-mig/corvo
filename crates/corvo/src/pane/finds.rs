use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use corvo_domain::{Found, Query, Workbench};
use corvo_ui::file_icon::FileIcon;
use corvo_ui::find::{self, toggle};
use corvo_ui::{field, notice};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, Focusable as _, FontWeight,
    HighlightStyle, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledText, Subscription, Task,
    UniformListScrollHandle, Window, div, px, uniform_list,
};

use crate::pane::code::Code;

// How long typing rests before the files are looked through again.
const SETTLE: Duration = Duration::from_millis(200);
// The most hits a search keeps.
const MOST: usize = 5000;
const ROW: f32 = 22.;

pub enum Event {
    // A hit was picked: its file, and where it is in the text that was looked through.
    Open(PathBuf, Range<usize>),
}

// A line of the results: a file with how many hits it holds, or one of its hits.
#[derive(Clone, Copy)]
enum Line {
    File(usize),
    Hit(usize, usize),
}

// The search through the files of the project in front, in the drawer of the code: what is
// looked for and how, what replaces it, and the hits, file by file.
pub struct Finds {
    bench: Entity<Workbench>,
    code: Entity<Code>,
    query: Entity<field::State>,
    with: Entity<field::State>,
    case: bool,
    word: bool,
    regex: bool,
    replacing: bool,
    found: Option<Found>,
    // Why the query finds nothing, when it does not hold.
    fault: Option<String>,
    busy: bool,
    // The files whose hits are folded away.
    folded: HashSet<PathBuf>,
    scroll: UniformListScrollHandle,
    // Each search takes the next turn; one whose turn has passed stops, and is not shown.
    turn: Arc<AtomicUsize>,
    _soon: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for Finds {}

impl Finds {
    pub fn new(
        bench: Entity<Workbench>,
        code: Entity<Code>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| field::state("Search", window, cx));
        let with = cx.new(|cx| field::state("Replace", window, cx));
        let subs = vec![
            cx.subscribe(&query, |this, _, event: &field::Event, cx| match event {
                field::Event::Change => this.soon(cx),
                field::Event::PressEnter { .. } => this.seek(cx),
                _ => {}
            }),
        ];
        Self {
            bench,
            code,
            query,
            with,
            case: false,
            word: false,
            regex: false,
            replacing: false,
            found: None,
            fault: None,
            busy: false,
            folded: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
            turn: Arc::default(),
            _soon: None,
            _subs: subs,
        }
    }

    // The keys go to the query, all of it selected; a text given takes its place first.
    pub fn focus(&mut self, text: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let changed = text.is_some_and(|text| {
            let changed = *self.query.read(cx).value() != *text;
            if changed {
                self.query
                    .update(cx, |query, cx| query.set_value(text, window, cx));
            }
            changed
        });
        self.query.update(cx, |query, cx| {
            query.focus(window, cx);
            query.select_all(window, cx);
        });
        if changed {
            self.seek(cx);
        }
    }

    // The keys are in the query or in what replaces it.
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        [&self.query, &self.with]
            .iter()
            .any(|field| field.read(cx).focus_handle(cx).is_focused(window))
    }

    // The files changed: the hits are looked for again, should there be a query.
    pub fn rerun(&mut self, cx: &mut Context<Self>) {
        if !self.query.read(cx).value().is_empty() {
            self.soon(cx);
        }
    }

    fn query(&self, cx: &App) -> Query {
        Query {
            text: self.query.read(cx).value().to_string(),
            case: self.case,
            word: self.word,
            regex: self.regex,
        }
    }

    fn soon(&mut self, cx: &mut Context<Self>) {
        self._soon = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |this, cx| this.seek(cx)).ok();
        }));
    }

    // Looks through the files off the main thread; the search before it, if still going, stops.
    fn seek(&mut self, cx: &mut Context<Self>) {
        self._soon = None;
        let turn = self.turn.fetch_add(1, Ordering::SeqCst) + 1;
        self.fault = None;
        let finder = match self.query(cx).finder() {
            Ok(Some(finder)) => finder,
            Ok(None) => {
                self.found = None;
                self.busy = false;
                cx.notify();
                return;
            }
            Err(fault) => {
                self.found = None;
                self.fault = Some(fault);
                self.busy = false;
                cx.notify();
                return;
            }
        };
        let Some(root) = self.bench.read(cx).project().map(Path::to_path_buf) else {
            return;
        };
        let unsaved = self.code.read(cx).unsaved(cx);
        let now = self.turn.clone();
        self.busy = true;
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let stale = || now.load(Ordering::SeqCst) != turn;
            corvo_fs::search(&root, &finder, &unsaved, MOST, &stale)
        });
        cx.spawn(async move |this, cx| {
            let found = task.await;
            this.update(cx, |this, cx| {
                if this.turn.load(Ordering::SeqCst) != turn {
                    return;
                }
                this.busy = false;
                if let Some(found) = found {
                    this.found = Some(found);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn flip(&mut self, f: fn(&mut Self) -> &mut bool, cx: &mut Context<Self>) {
        let on = f(self);
        *on = !*on;
        self.seek(cx);
    }

    fn toggle_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.replacing = !self.replacing;
        let field = if self.replacing {
            &self.with
        } else {
            &self.query
        };
        field.update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }

    fn fold(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !self.folded.remove(path) {
            self.folded.insert(path.to_path_buf());
        }
        cx.notify();
    }

    // Asks before replacing every hit, telling how many and in how many files.
    fn ask_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(found) = self.found.as_ref().filter(|found| found.hits > 0) else {
            return;
        };
        let with = self.with.read(cx).value().to_string();
        let (hits, files) = (found.hits, found.files.len());
        let more = if found.capped {
            " The search stopped at its most: what it did not show stays as it is."
        } else {
            ""
        };
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            alert
                .title(format!(
                    "Replace {hits} {} in {files} {}?",
                    plural(hits, "match", "matches"),
                    plural(files, "file", "files"),
                ))
                .description(format!(
                    "With “{with}”. Files open in the code change there, to be saved; the \
                     others are written now.{more}"
                ))
                .ok_text("Replace All")
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    view.update(cx, |this, cx| this.replace_all(window, cx))
                        .ok();
                    true
                })
        });
    }

    // Replaces the hits shown: in the editor of a file open in the code, on disk for the rest.
    fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(found), Ok(Some(finder))) = (self.found.take(), self.query(cx).finder()) else {
            return;
        };
        let with = self.with.read(cx).value().to_string();
        let (mut count, mut files, mut failed) = (0, 0, Vec::new());
        for (path, _) in &found.files {
            let replaced = self
                .code
                .update(cx, |code, cx| {
                    code.replace_in(path, &finder, &with, window, cx)
                })
                .map(Ok)
                .unwrap_or_else(|| {
                    self.bench
                        .update(cx, |bench, _| bench.replace(path, &finder, &with))
                });
            match replaced {
                Ok(0) => {}
                Ok(n) => {
                    count += n;
                    files += 1;
                }
                Err(err) => failed.push(format!("{}: {err}", self.shown(path, cx).0)),
            }
        }
        notice::info(
            format!(
                "Replaced {count} {} in {files} {}",
                plural(count, "match", "matches"),
                plural(files, "file", "files")
            ),
            window,
            cx,
        );
        if !failed.is_empty() {
            notice::error(failed.join("\n"), window, cx);
        }
        self.seek(cx);
    }

    fn lines(&self) -> Vec<Line> {
        let Some(found) = &self.found else {
            return Vec::new();
        };
        let mut lines = Vec::with_capacity(found.files.len() + found.hits);
        for (file, (path, hits)) in found.files.iter().enumerate() {
            lines.push(Line::File(file));
            if !self.folded.contains(path) {
                lines.extend((0..hits.len()).map(|hit| Line::Hit(file, hit)));
            }
        }
        lines
    }

    // A file's name, and the folder it is in from the project down.
    fn shown(&self, path: &Path, cx: &App) -> (SharedString, SharedString) {
        let rest = self
            .bench
            .read(cx)
            .project()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path);
        let name = rest.file_name().unwrap_or_default().to_string_lossy();
        let dir = rest.parent().unwrap_or(Path::new("")).display().to_string();
        (name.into_owned().into(), dir.into())
    }

    fn summary(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let (text, color) = if let Some(fault) = &self.fault {
            (fault.clone(), theme.danger)
        } else if self.busy && self.found.is_none() {
            ("Searching…".to_owned(), theme.muted_foreground)
        } else if let Some(found) = &self.found {
            let text = match found.hits {
                0 => "No results".to_owned(),
                hits => {
                    let files = found.files.len();
                    let mut text = format!(
                        "{hits} {} in {files} {}",
                        plural(hits, "result", "results"),
                        plural(files, "file", "files")
                    );
                    if found.capped {
                        text.push_str(", the first ones");
                    }
                    text
                }
            };
            (text, theme.muted_foreground)
        } else {
            (
                "Searches the project's files, as git keeps them".to_owned(),
                theme.muted_foreground,
            )
        };
        div()
            .px_3()
            .pb_1()
            .text_xs()
            .text_color(color)
            .child(text)
            .into_any_element()
    }

    fn line(&self, line: Line, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let Some(found) = &self.found else {
            return div().into_any_element();
        };
        match line {
            Line::File(file) => {
                let (path, hits) = &found.files[file];
                let (name, dir) = self.shown(path, cx);
                let chevron = if self.folded.contains(path) {
                    Glyph::ChevronRight
                } else {
                    Glyph::ChevronDown
                };
                let path = path.clone();
                h_flex()
                    .id(("finds-line", ix))
                    .w_full()
                    .h(px(ROW))
                    .px_2()
                    .gap_1p5()
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.list_hover))
                    .child(
                        Icon::new(chevron)
                            .size(px(12.))
                            .text_color(theme.muted_foreground),
                    )
                    .child(FileIcon::new(name.clone()).size(px(14.)))
                    .child(div().flex_shrink_0().child(name))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(dir),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_1p5()
                            .rounded_full()
                            .bg(theme.secondary)
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(hits.iter().map(|hit| hit.count).sum::<usize>().to_string()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.fold(&path, cx)))
                    .into_any_element()
            }
            Line::Hit(file, hit) => {
                let (path, hits) = &found.files[file];
                let hit = &hits[hit];
                let mark = HighlightStyle {
                    background_color: Some(theme.blue.opacity(0.3)),
                    color: Some(theme.foreground),
                    font_weight: Some(FontWeight::MEDIUM),
                    ..Default::default()
                };
                let (path, range) = (path.clone(), hit.range.clone());
                let marks = hit.marks.iter().map(|range| (range.clone(), mark));
                h_flex()
                    .id(("finds-line", ix))
                    .w_full()
                    .h(px(ROW))
                    .pl(px(36.))
                    .pr_2()
                    .gap_2()
                    .cursor_pointer()
                    .text_color(theme.muted_foreground)
                    .hover(|row| row.bg(theme.list_hover))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(StyledText::new(hit.preview.clone()).with_highlights(marks)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .child((hit.line + 1).to_string()),
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(Event::Open(path.clone(), range.clone()));
                    }))
                    .into_any_element()
            }
        }
    }
}

impl Render for Finds {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let view = cx.entity().downgrade();
        let on = |f: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            let view = view.clone();
            move |window: &mut Window, cx: &mut App| {
                view.update(cx, |this, cx| f(this, window, cx)).ok();
            }
        };
        let toggles = h_flex()
            .gap_0p5()
            .child(toggle(
                "finds-case",
                Glyph::CaseSensitive,
                self.case,
                "Match case",
                on(|this, _, cx| this.flip(|this| &mut this.case, cx)),
            ))
            .child(toggle(
                "finds-word",
                Glyph::WholeWord,
                self.word,
                "Whole word",
                on(|this, _, cx| this.flip(|this| &mut this.word, cx)),
            ))
            .child(toggle(
                "finds-regex",
                Glyph::Regex,
                self.regex,
                "Regular expression",
                on(|this, _, cx| this.flip(|this| &mut this.regex, cx)),
            ));
        let found = self.found.as_ref().is_some_and(|found| found.hits > 0);
        let replace = self.replacing.then(|| {
            h_flex()
                .gap_1()
                .child(div().flex_1().min_w_0().child(find::field(&self.with)))
                .child(find::icon(
                    "finds-replace-all",
                    Glyph::ReplaceAll,
                    "Replace all",
                    found,
                    on(Self::ask_replace),
                ))
        });
        let chevron = if self.replacing {
            Glyph::ChevronDown
        } else {
            Glyph::ChevronRight
        };
        let fields = h_flex()
            .items_start()
            .gap_1()
            .px_2()
            .pt_1()
            .pb_1p5()
            .child(find::icon(
                "finds-toggle",
                chevron,
                "Replace",
                true,
                on(Self::toggle_replace),
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(find::field(&self.query).suffix(toggles))
                    .children(replace),
            );

        let lines = self.lines();
        let list = uniform_list(
            "finds",
            lines.len(),
            cx.processor(move |this, range: Range<usize>, _, cx| {
                range
                    .filter_map(|ix| lines.get(ix).map(|line| this.line(*line, ix, cx)))
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();

        v_flex()
            .size_full()
            .child(fields)
            .child(self.summary(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_sm()
                    .child(list)
                    .vertical_scrollbar(&self.scroll),
            )
    }
}

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}
