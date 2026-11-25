use std::path::Path;

use corvo_domain::{Place, Workbench};
use corvo_ui::button::Button;
use corvo_ui::file_icon::{FileIcon, FolderIcon};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use crate::pane::overview::Overview;

// The cards and groups hidden from the home, each with a way back, as the status bar opens it.
pub struct Hidden {
    bench: Entity<Workbench>,
    home: Entity<Overview>,
}

impl Hidden {
    pub fn new(bench: Entity<Workbench>, home: Entity<Overview>) -> Self {
        Self { bench, home }
    }

    fn show(&mut self, place: Option<Place>, window: &mut Window, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.show_hidden(place, window, cx));
        cx.notify();
    }

    fn show_group(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.show_group(id, window, cx));
        cx.notify();
    }
}

impl Render for Hidden {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let atlas = self.bench.read(cx).atlas();
        let root = atlas.root().map(Path::to_path_buf);
        let rows: Vec<(Place, String)> = atlas
            .hidden()
            .map(|(place, name)| (place.clone(), name.to_owned()))
            .collect();
        let groups: Vec<(u64, String, usize)> = atlas
            .groups()
            .iter()
            .filter(|group| group.is_hidden())
            .map(|group| (group.id(), group.name().to_owned(), group.members().len()))
            .collect();
        let muted = theme.muted_foreground;

        let mut list = v_flex()
            .id("hidden")
            .w(px(320.))
            .max_h(px(360.))
            .overflow_y_scroll()
            .gap_0p5()
            .text_sm();
        if rows.is_empty() && groups.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(muted)
                    .child("Nothing hidden. Right click a card on the home to hide it."),
            );
        }
        let count = rows.len() + groups.len();
        for (ix, (id, name, members)) in groups.into_iter().enumerate() {
            let view = cx.entity().downgrade();
            let held = match members {
                1 => "Group · 1 card".to_owned(),
                n => format!("Group · {n} cards"),
            };
            list = list.child(
                h_flex()
                    .id(("hidden-group", ix))
                    .px_2()
                    .py_1()
                    .gap_2()
                    .rounded_md()
                    .hover(|style| style.bg(theme.list_hover))
                    .child(Icon::new(Glyph::Group).small().text_color(muted))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(name))
                            .child(div().truncate().text_xs().text_color(muted).child(held)),
                    )
                    .child(Button::ghost(("show-group", ix)).label("Show").on_click(
                        move |_, window, cx| {
                            view.update(cx, |this, cx| this.show_group(id, window, cx))
                                .ok();
                        },
                    )),
            );
        }
        for (ix, (place, name)) in rows.into_iter().enumerate() {
            let (icon, at) = look(&place, &name, root.as_deref(), cx);
            let view = cx.entity().downgrade();
            list = list.child(
                h_flex()
                    .id(("hidden-row", ix))
                    .px_2()
                    .py_1()
                    .gap_2()
                    .rounded_md()
                    .hover(|style| style.bg(theme.list_hover))
                    .child(icon)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(name))
                            .child(div().truncate().text_xs().text_color(muted).child(at)),
                    )
                    .child(Button::ghost(("show-hidden", ix)).label("Show").on_click(
                        move |_, window, cx| {
                            let place = place.clone();
                            view.update(cx, |this, cx| this.show(Some(place), window, cx))
                                .ok();
                        },
                    )),
            );
        }
        if count > 1 {
            let view = cx.entity().downgrade();
            list = list.child(
                h_flex()
                    .px_1()
                    .pt_1()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(Button::ghost("show-all-hidden").label("Show all").on_click(
                        move |_, window, cx| {
                            view.update(cx, |this, cx| this.show(None, window, cx)).ok();
                        },
                    )),
            );
        }
        list
    }
}

// The icon of a hidden card, and where it is.
fn look(
    place: &Place,
    name: &str,
    root: Option<&Path>,
    cx: &Context<Hidden>,
) -> (AnyElement, String) {
    let size = px(16.);
    let near = |path: &Path| {
        let shown = root
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path);
        shown.display().to_string()
    };
    match place {
        Place::Node(path) => {
            let icon = if path.is_dir() {
                FolderIcon::new(name.to_owned())
                    .size(size)
                    .into_any_element()
            } else {
                FileIcon::new(name.to_owned()).size(size).into_any_element()
            };
            (icon, near(path.parent().unwrap_or(path)))
        }
        Place::Symbol(file, _) => {
            let icon = Icon::new(Glyph::Braces)
                .small()
                .text_color(cx.theme().muted_foreground)
                .into_any_element();
            (icon, near(file))
        }
    }
}
