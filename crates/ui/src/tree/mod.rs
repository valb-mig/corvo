use corvo_domain::SymbolKind;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{Tree, tree};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex};
use gpui_kit::{
    AnyElement, App, Entity, Hsla, IntoElement, ParentElement as _, Pixels, RenderOnce,
    SharedString, Styled as _, Window, div, px,
};

use crate::file_icon::{FileIcon, FolderIcon};
use crate::symbol_icon::SymbolIcon;

const ICON: Pixels = px(14.);
const CHEVRON: Pixels = px(12.);
const HEIGHT: Pixels = px(22.);
const INDENT: Pixels = px(12.);
const TEXT: Pixels = px(13.);

pub use gpui_kit::component::tree::{
    TreeEntry as Entry, TreeEvent as Event, TreeItem as Item, TreeState as State,
};

#[derive(IntoElement)]
pub struct Panel {
    tree: Tree,
}

impl Panel {
    pub fn new<R>(state: &Entity<State>, row: R) -> Self
    where
        R: Fn(&Entry, &App) -> ListItem + 'static,
    {
        Self {
            tree: tree(state, move |_, entry, _, _, cx| row(entry, cx)),
        }
    }
}

impl RenderOnce for Panel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .size_full()
            .py_1()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().foreground)
            .child(self.tree)
    }
}

// The rows of a tree: low, with a chevron before what opens, so many fit in a narrow panel.
pub struct Row;

impl Row {
    pub fn dir(entry: &Entry, cx: &App) -> ListItem {
        let icon = FolderIcon::new(entry.item().label.clone())
            .open(entry.is_expanded())
            .size(ICON);
        Self::build(entry, icon, cx)
    }

    pub fn file(entry: &Entry, cx: &App) -> ListItem {
        Self::build(
            entry,
            FileIcon::new(entry.item().label.clone()).size(ICON),
            cx,
        )
    }

    pub fn symbol(entry: &Entry, kind: SymbolKind, cx: &App) -> ListItem {
        Self::build(entry, SymbolIcon::new(kind).size(ICON), cx)
    }

    // The fields or variants of a type, gathered in one row under it, with how many there are.
    pub fn group(entry: &Entry, kind: SymbolKind, count: usize, cx: &App) -> ListItem {
        let muted = cx.theme().muted_foreground;
        Self::build(entry, SymbolIcon::new(kind).size(ICON), cx)
            .text_color(muted)
            .suffix(move |_, _| div().text_xs().text_color(muted).child(count.to_string()))
    }

    // How many fields and methods a folded type holds, at the end of its row.
    pub fn counts(item: ListItem, counts: Vec<(SymbolKind, usize)>, cx: &App) -> ListItem {
        if counts.is_empty() {
            return item;
        }
        let muted = cx.theme().muted_foreground;
        item.suffix(move |_, _| {
            h_flex()
                .gap_1()
                .text_xs()
                .text_color(muted)
                .children(counts.iter().map(|&(kind, count)| {
                    h_flex()
                        .gap(px(2.))
                        .child(SymbolIcon::new(kind).size(px(11.)))
                        .child(count.to_string())
                }))
        })
    }

    // The symbol the cursor is in: a bar along its left and a light fill.
    pub fn here(item: ListItem, cx: &App) -> ListItem {
        let blue = cx.theme().blue;
        item.bg(blue.opacity(0.08)).child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(2.))
                .bg(blue),
        )
    }

    // A row in a color of its own, as a changed file or folder; a file with a light fill behind.
    pub fn tint(item: ListItem, color: Hsla, fill: bool) -> ListItem {
        let item = item.text_color(color);
        if fill {
            item.bg(color.opacity(0.1))
        } else {
            item
        }
    }

    fn build(entry: &Entry, icon: impl IntoElement, cx: &App) -> ListItem {
        let item = entry.item();
        ListItem::new(item.id.clone())
            .h(HEIGHT)
            .py_0()
            .text_size(TEXT)
            .pl(INDENT * entry.depth() + px(6.))
            .pr_2()
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(chevron(entry, cx))
                    .child(icon)
                    .child(label(item.label.clone())),
            )
    }
}

// What opens shows a chevron, down while open; the rest keeps its room, so icons line up.
fn chevron(entry: &Entry, cx: &App) -> AnyElement {
    let slot = div().flex_shrink_0().size(CHEVRON);
    if !entry.is_folder() {
        return slot.into_any_element();
    }
    let glyph = if entry.is_expanded() {
        Glyph::ChevronDown
    } else {
        Glyph::ChevronRight
    };
    slot.child(
        Icon::new(glyph)
            .size(CHEVRON)
            .text_color(cx.theme().muted_foreground),
    )
    .into_any_element()
}

fn label(text: SharedString) -> impl IntoElement {
    div().min_w_0().truncate().child(text)
}
