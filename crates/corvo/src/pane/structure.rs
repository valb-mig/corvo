use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use corvo_domain::port::Depth;
use corvo_domain::{Symbol, SymbolKind, Workbench};
use corvo_ui::notice;
use corvo_ui::tree::{Event as TreeEvent, Item, Panel, Row, State};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, div,
};

pub enum Event {
    Jump(PathBuf, u32, u32),
}

#[derive(Clone, Copy)]
struct Spot {
    kind: SymbolKind,
    line: u32,
    column: u32,
}

// The outline of the active file, at the foot of the editor's sidebar: types, functions,
// methods, fields…, nested as in the code. A click on a symbol puts the cursor on it.
pub struct Structure {
    bench: Entity<Workbench>,
    state: Entity<State>,
    file: Option<PathBuf>,
    symbols: Vec<Symbol>,
    folded: HashSet<SharedString>,
    spots: Rc<HashMap<SharedString, Spot>>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<Event> for Structure {}

impl Structure {
    pub fn new(bench: Entity<Workbench>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| State::new(cx));
        let subs = vec![cx.subscribe_in(&state, window, Self::on_tree)];
        Self {
            bench,
            state,
            file: None,
            symbols: Vec::new(),
            folded: HashSet::new(),
            spots: Rc::default(),
            _subs: subs,
        }
    }

    // Follows the active file, or none; the same file again keeps what was folded.
    pub fn follow(&mut self, file: Option<&Path>, window: &mut Window, cx: &mut Context<Self>) {
        if self.file.as_deref() == file {
            return;
        }
        self.folded.clear();
        self.file = file.map(Path::to_path_buf);
        self.trace(window, cx);
    }

    // Shown only for a file whose language has an outline.
    pub fn is_shown(&self, cx: &App) -> bool {
        self.file
            .as_deref()
            .is_some_and(|file| self.bench.read(cx).outlines(file))
    }

    // The file was written: its outline is read again.
    pub fn saved(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self.file.as_deref() == Some(path) {
            self.trace(window, cx);
        }
    }

    fn trace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.symbols = match &self.file {
            Some(file) => match self.bench.read(cx).outline(file, Depth::Items) {
                Ok(symbols) => symbols,
                Err(err) => {
                    notice::error(format!("{}: {err}", file.display()), window, cx);
                    Vec::new()
                }
            },
            None => Vec::new(),
        };
        self.refresh(cx);
    }

    fn on_tree(
        &mut self,
        _: &Entity<State>,
        event: &TreeEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TreeEvent::Expanded(id) => self.folded.remove(id),
            TreeEvent::Collapsed(id) => self.folded.insert(id.clone()),
        };
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let mut spots = HashMap::new();
        let items = items(&self.symbols, &self.folded, &mut spots);
        self.spots = Rc::new(spots);
        self.state.update(cx, |state, cx| {
            let selected = state.selected_item().map(|item| item.id.clone());
            state.set_items(items, cx);
            let ix = selected.and_then(|id| state.index_of(&id));
            state.set_selected_index(ix, cx);
        });
        cx.notify();
    }
}

impl Render for Structure {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let body = match self.symbols.is_empty().then_some("No structure") {
            Some(text) => div()
                .px_3()
                .py_2()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(text)
                .into_any_element(),
            None => {
                let spots = self.spots.clone();
                let file = self.file.clone().unwrap_or_default();
                let view = cx.entity().downgrade();
                Panel::new(&self.state, move |entry| {
                    let id = &entry.item().id;
                    let Some(spot) = spots.get(id).copied() else {
                        return Row::file(entry);
                    };
                    let (view, file) = (view.clone(), file.clone());
                    Row::symbol(entry, spot.kind).on_click(move |_, _, cx| {
                        let file = file.clone();
                        view.update(cx, |_, cx| {
                            cx.emit(Event::Jump(file, spot.line, spot.column));
                        })
                        .ok();
                    })
                })
                .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .overflow_hidden()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(div().flex_1().min_h_0().child(body))
    }
}

fn items(
    symbols: &[Symbol],
    folded: &HashSet<SharedString>,
    spots: &mut HashMap<SharedString, Spot>,
) -> Vec<Item> {
    symbols
        .iter()
        .map(|symbol| {
            let (line, column) = (symbol.line(), symbol.column());
            let id = SharedString::from(format!("{line}:{column}"));
            let kind = symbol.kind();
            spots.insert(id.clone(), Spot { kind, line, column });
            Item::new(id.clone(), symbol.name().to_owned())
                .expanded(!folded.contains(&id))
                .children(items(symbol.nested(), folded, spots))
        })
        .collect()
}
