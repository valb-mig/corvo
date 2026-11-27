use corvo_domain::{Profile, mix};
use corvo_ui::button::Button;
use corvo_ui::field;
use corvo_ui::language_icon::LanguageIcon;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::{Home, count};

// The column of workspaces.
pub(super) const SIDE: f32 = 300.;

impl Home {
    pub(super) fn side(&self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let spaces = self.spaces.read(cx);
        let facts = self.facts.read(cx);
        let accent = theme.blue;

        let mut items = v_flex()
            .id("workspace-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(px(2.));
        if spaces.items().is_empty() {
            items = items.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(muted)
                    .child("No workspaces yet."),
            );
        }
        for space in spaces.items() {
            let id = space.id();
            let on = self.picked == Some(id);
            let profiles: Vec<&Profile> = space
                .projects()
                .iter()
                .filter_map(|project| facts.profile(project))
                .collect();
            let logos = mix(profiles)
                .into_iter()
                .take(3)
                .map(|share| LanguageIcon::new(share.name()).size(px(13.)));
            let projects = match space.projects().len() {
                0 => "no projects".to_owned(),
                n => count(n, "project", "projects"),
            };
            let view = cx.entity().downgrade();
            items = items.child(
                h_flex()
                    .id(("workspace", id.as_u128() as u64))
                    .gap_2()
                    .px_2()
                    .py(px(7.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(gpui_kit::transparent_black())
                    .cursor_pointer()
                    .map(|row| {
                        if on {
                            row.bg(accent.opacity(0.12))
                                .border_color(accent.opacity(0.45))
                        } else {
                            row.hover(|row| row.bg(theme.muted))
                        }
                    })
                    .on_click(move |event, window, cx| {
                        view.update(cx, |home, cx| {
                            home.focus.focus(window, cx);
                            if event.click_count() > 1 {
                                home.open(id, None, window, cx);
                            } else {
                                home.pick(id, window, cx);
                            }
                        })
                        .ok();
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().text_sm().child(space.name().to_owned()))
                            .child(div().text_xs().text_color(muted).child(projects)),
                    )
                    .child(h_flex().gap_1p5().children(logos)),
            );
        }

        let add = cx.entity().downgrade();
        v_flex()
            .w(px(SIDE))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(theme.border)
            .px_3()
            .pt_5()
            .pb_3()
            .child(div().px_2().pb_4().text_size(px(22.)).child("corvo"))
            .child(
                div()
                    .px_2()
                    .pb_2()
                    .text_xs()
                    .text_color(muted)
                    .child("WORKSPACES"),
            )
            .child(items)
            .child(self.hints(cx))
            .child(
                h_flex()
                    .gap_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(v_flex().flex_1().child(field::view(&self.name)))
                    .child(
                        Button::primary("add")
                            .label("Add")
                            .on_click(move |_, window, cx| {
                                add.update(cx, |home, cx| home.add(window, cx)).ok();
                            }),
                    ),
            )
            .into_any_element()
    }

    fn hints(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let muted = cx.theme().muted_foreground;
        let hint = |keys: &[&str], what: &'static str, cx: &Context<Self>| {
            h_flex()
                .gap_1()
                .children(keys.iter().map(|key| kbd(key, cx)))
                .child(div().text_color(muted).child(what))
        };
        h_flex()
            .flex_wrap()
            .gap_3()
            .px_2()
            .pt_2()
            .pb_3()
            .text_xs()
            .child(hint(&["↑", "↓"], "select", cx))
            .child(hint(&["↵"], "open", cx))
            .child(hint(&["F2"], "rename", cx))
            .child(hint(&["Ctrl", "N"], "new", cx))
    }
}

// A key, as drawn in a hint.
pub(super) fn kbd(key: &str, cx: &Context<Home>) -> impl IntoElement + use<> {
    let theme = cx.theme();
    div()
        .min_w(px(18.))
        .h(px(18.))
        .px_1()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .text_color(theme.muted_foreground)
        .font_family(theme.mono_font_family.clone())
        .text_size(px(10.5))
        .child(key.to_owned())
}
