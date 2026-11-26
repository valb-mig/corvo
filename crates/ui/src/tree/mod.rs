use corvo_domain::SymbolKind;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{Tree, tree};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    App, Entity, Hsla, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div, px,
};

use crate::file_icon::{FileIcon, FolderIcon};
use crate::symbol_icon::SymbolIcon;

const ICON: gpui_kit::Pixels = px(16.);

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
            .text_sm()
            .child(self.tree)
    }
}

pub struct Row;

impl Row {
    pub fn dir(entry: &Entry) -> ListItem {
        let icon = FolderIcon::new(entry.item().label.clone())
            .open(entry.is_expanded())
            .size(ICON);
        Self::build(entry, icon)
    }

    pub fn file(entry: &Entry) -> ListItem {
        Self::build(entry, FileIcon::new(entry.item().label.clone()).size(ICON))
    }

    pub fn symbol(entry: &Entry, kind: SymbolKind) -> ListItem {
        Self::build(entry, SymbolIcon::new(kind).size(ICON))
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

    fn build(entry: &Entry, icon: impl IntoElement) -> ListItem {
        let item = entry.item();
        ListItem::new(item.id.clone())
            .pl(px(12.) * entry.depth() + px(8.))
            .child(h_flex().gap_2().child(icon).child(item.label.clone()))
    }
}
