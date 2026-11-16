use gpui_kit::component::list::ListItem;
use gpui_kit::component::tree::{Tree, tree};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};
use gpui_kit::{
    App, Entity, IntoElement, ParentElement as _, RenderOnce, Styled as _, Window, div, px,
};

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
        R: Fn(&Entry) -> ListItem + 'static,
    {
        Self {
            tree: tree(state, move |_, entry, _, _, _| row(entry)),
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
        let icon = if entry.is_expanded() {
            IconName::FolderOpen
        } else {
            IconName::Folder
        };
        Self::build(entry, icon)
    }

    pub fn file(entry: &Entry) -> ListItem {
        Self::build(entry, IconName::File)
    }

    fn build(entry: &Entry, icon: IconName) -> ListItem {
        let item = entry.item();
        ListItem::new(item.id.clone())
            .pl(px(12.) * entry.depth() + px(8.))
            .child(
                h_flex()
                    .gap_2()
                    .child(Icon::new(icon).small())
                    .child(item.label.clone()),
            )
    }
}
