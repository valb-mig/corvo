use std::collections::HashSet;
use std::path::{Path, PathBuf};

use corvo_domain::{Profile, Uuid};
use corvo_ui::button::Button;
use corvo_ui::field;
use corvo_ui::language_icon::{LanguageIcon, logo, tint};
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px, relative,
};

use super::facts::Branch;
use super::{Home, count, tilde};

// The widest the grid of projects gets.
const GRID: f32 = 920.;

impl Home {
    pub(super) fn detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let body = match self.picked {
            Some(id) => self.space(id, cx),
            None => self.blank(cx),
        };
        div()
            .id("workspace-detail")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .px(px(48.))
            .py(px(32.))
            .child(body)
            .into_any_element()
    }

    // No workspace yet: what one is.
    fn blank(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(div().text_base().child("Name a workspace to start"))
            .child(
                div()
                    .max_w(px(360.))
                    .text_sm()
                    .text_center()
                    .text_color(muted)
                    .child(
                        "A workspace links any number of project folders. Here you'll see each \
                         one with its stack and branch.",
                    ),
            )
            .into_any_element()
    }

    fn space(&self, id: Uuid, cx: &mut Context<Self>) -> AnyElement {
        let Some(space) = self.spaces.read(cx).get(id).cloned() else {
            return self.blank(cx);
        };
        let theme = cx.theme().clone();
        let projects = space.projects().to_vec();
        let removing = self.removing.filter(|(asked, _)| *asked == id);

        // The title, or the field it turns into.
        let title: AnyElement = match &self.renaming {
            Some((renamed, state, _)) if *renamed == id => {
                let view = cx.entity().downgrade();
                div()
                    .flex_1()
                    .max_w(px(420.))
                    .capture_key_down(move |event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "escape" {
                            cx.stop_propagation();
                            view.update(cx, |home, cx| home.cancel(window, cx)).ok();
                        }
                    })
                    .child(field::view(state))
                    .into_any_element()
            }
            _ => {
                let view = cx.entity().downgrade();
                div()
                    .id("workspace-title")
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(22.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(move |event, window, cx| {
                        if event.click_count() > 1 {
                            view.update(cx, |home, cx| home.rename(id, window, cx)).ok();
                        }
                    })
                    .child(space.name().to_owned())
                    .into_any_element()
            }
        };
        let (renamer, remover, opener) = (
            cx.entity().downgrade(),
            cx.entity().downgrade(),
            cx.entity().downgrade(),
        );
        let renaming = self
            .renaming
            .as_ref()
            .is_some_and(|(renamed, _, _)| *renamed == id);
        let actions = h_flex()
            .gap_1()
            .flex_shrink_0()
            .when(renaming, |actions| actions.invisible())
            .when(removing.is_none(), |actions| {
                actions
                    .child(Button::ghost("rename-workspace").label("Rename").on_click(
                        move |_, window, cx| {
                            renamer
                                .update(cx, |home, cx| home.rename(id, window, cx))
                                .ok();
                        },
                    ))
                    .child(Button::ghost("remove-workspace").label("Remove").on_click(
                        move |_, _, cx| {
                            remover.update(cx, |home, cx| home.ask_remove(id, cx)).ok();
                        },
                    ))
                    .child(Button::primary("open-workspace").label("Open ↵").on_click(
                        move |_, window, cx| {
                            opener
                                .update(cx, |home, cx| home.open(id, None, window, cx))
                                .ok();
                        },
                    ))
            });

        let mut page = v_flex()
            .w_full()
            .max_w(px(GRID))
            .child(h_flex().gap_3().min_h(px(34.)).child(title).child(actions))
            .child(
                div()
                    .pt_1()
                    .pb_5()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(self.summary(&projects, cx)),
            );
        if let Some((_, chats)) = removing {
            page = page.child(self.confirm(id, space.name(), chats, cx));
        }

        let mut grid = div()
            .grid()
            .grid_cols(2)
            .gap_3()
            .when(removing.is_some(), |grid| grid.opacity(0.45));
        for (ix, project) in projects.iter().enumerate() {
            grid = grid.child(self.card(id, ix, project, cx));
        }
        let linker = cx.entity().downgrade();
        grid = grid.child(
            v_flex()
                .id("link-folder")
                .when(projects.is_empty(), |card| card.col_span(2))
                .min_h(px(if projects.is_empty() { 120. } else { 140. }))
                .items_center()
                .justify_center()
                .gap_1()
                .rounded(px(8.))
                .border_1()
                .border_dashed()
                .border_color(theme.border)
                .text_sm()
                .text_color(theme.muted_foreground)
                .cursor_pointer()
                .hover(|card| card.bg(theme.muted).text_color(theme.foreground))
                .on_click(move |_, window, cx| {
                    linker
                        .update(cx, |home, cx| home.browse(id, None, window, cx))
                        .ok();
                })
                .child(
                    h_flex()
                        .gap_2()
                        .child(Icon::new(Glyph::Plus).small())
                        .child("Link folder…"),
                )
                .when(projects.is_empty(), |card| {
                    card.child(
                        div()
                            .text_xs()
                            .child("Pick a project folder. Its stack and branch show up here."),
                    )
                }),
        );
        page.child(grid).into_any_element()
    }

    // `3 projects · PHP, Node · 41 files changed`
    fn summary(&self, projects: &[PathBuf], cx: &Context<Self>) -> String {
        if projects.is_empty() {
            return "No projects linked".to_owned();
        }
        let facts = self.facts.read(cx);
        let mut parts = vec![count(projects.len(), "project", "projects")];
        let mut stacks: Vec<&str> = Vec::new();
        for profile in projects.iter().filter_map(|project| facts.profile(project)) {
            for stamp in profile.stamps() {
                if !stacks.contains(&stamp.stack()) {
                    stacks.push(stamp.stack());
                }
            }
        }
        if !stacks.is_empty() {
            parts.push(stacks.join(", "));
        }
        // Projects that share a repository count its changes once.
        let mut roots: HashSet<&Path> = HashSet::new();
        let changed: usize = projects
            .iter()
            .filter_map(|project| facts.branch(project).flatten())
            .filter(|branch| roots.insert(&branch.root))
            .map(|branch| branch.changed)
            .sum();
        if changed > 0 {
            parts.push(count(changed, "file changed", "files changed"));
        }
        parts.join(" · ")
    }

    fn confirm(&self, id: Uuid, name: &str, chats: usize, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (keep, remove) = (cx.entity().downgrade(), cx.entity().downgrade());
        let what = match chats {
            0 => format!("Remove {name}? The folders stay on disk."),
            n => format!(
                "Remove {name} and its {}? The folders stay on disk.",
                count(n, "chat", "chats")
            ),
        };
        h_flex()
            .gap_3()
            .mb_5()
            .px_4()
            .py_3()
            .rounded(px(6.))
            .border_1()
            .border_color(theme.danger)
            .bg(theme.danger.opacity(0.1))
            .text_sm()
            .child(div().flex_1().child(what))
            .child(Button::ghost("keep-workspace").label("Cancel").on_click(
                move |_, window, cx| {
                    keep.update(cx, |home, cx| home.cancel(window, cx)).ok();
                },
            ))
            .child(Button::danger("confirm-remove").label("Remove ↵").on_click(
                move |_, window, cx| {
                    remove
                        .update(cx, |home, cx| home.remove(id, window, cx))
                        .ok();
                },
            ))
            .into_any_element()
    }

    fn card(&self, id: Uuid, ix: usize, project: &Path, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let facts = self.facts.read(cx);
        let profile = facts.profile(project).cloned();
        let branch = facts.branch(project).map(|branch| branch.cloned());
        let name = project
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| project.display().to_string());
        let group = format!("project-{ix}");

        if profile.as_ref().is_some_and(Profile::is_missing) {
            return self.lost(id, ix, project, &name, cx);
        }

        let top = profile
            .as_ref()
            .and_then(|profile| profile.shares().first())
            .map(|share| share.name());
        let tile = div()
            .size(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(match top {
                Some(language) => LanguageIcon::new(language).size(px(18.)).into_any_element(),
                None => Icon::new(Glyph::Folder)
                    .small()
                    .text_color(theme.muted_foreground)
                    .into_any_element(),
            });
        let stamps = profile.iter().flat_map(Profile::stamps).map(|stamp| {
            let label = match stamp.dir() {
                Some(dir) => format!("{dir}/ {}", stamp.stack()),
                None => stamp.stack().to_owned(),
            };
            Tag::secondary().xsmall().child(label)
        });
        let head = h_flex()
            .gap_3()
            .child(tile)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .font_family(theme.mono_font_family.clone())
                            .text_color(theme.muted_foreground)
                            .child(tilde(project)),
                    ),
            )
            .child(h_flex().gap_1().flex_shrink_0().children(stamps));

        let stack: AnyElement = match &profile {
            // Still being counted.
            None => v_flex()
                .gap_2()
                .child(div().h(px(4.)).w_full().rounded_full().bg(theme.muted))
                .child(
                    div()
                        .h(px(12.))
                        .w(relative(0.6))
                        .rounded(px(4.))
                        .bg(theme.muted),
                )
                .into_any_element(),
            Some(profile) if profile.shares().is_empty() => div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("No code yet. Only settings and docs in this folder.")
                .into_any_element(),
            Some(profile) => {
                let total = profile.files().max(1) as f32;
                let shown: Vec<(&'static str, f32)> = profile
                    .shares()
                    .iter()
                    .take(4)
                    .map(|share| (share.name(), share.files() as f32 / total))
                    .collect();
                let bar = h_flex()
                    .h(px(4.))
                    .w_full()
                    .gap(px(2.))
                    .rounded_full()
                    .overflow_hidden()
                    .children(shown.iter().map(|(language, part)| {
                        div()
                            .h_full()
                            .w(relative(*part))
                            .bg(color(language, &theme.muted_foreground))
                    }));
                let legend = h_flex().flex_wrap().gap_x_4().gap_y_1().text_xs().children(
                    shown.iter().take(3).map(|(language, part)| {
                        h_flex()
                            .gap_1p5()
                            .child(LanguageIcon::new(*language).size(px(13.)))
                            .child(*language)
                            .child(
                                div()
                                    .font_family(theme.mono_font_family.clone())
                                    .text_color(theme.muted_foreground)
                                    .child(percent(*part)),
                            )
                    }),
                );
                v_flex().gap_3().child(bar).child(legend).into_any_element()
            }
        };

        let git: AnyElement = match branch {
            None => div().into_any_element(),
            Some(None) => div()
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground)
                .child("no git")
                .into_any_element(),
            Some(Some(Branch { name, changed, .. })) => h_flex()
                .gap_1p5()
                .min_w_0()
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground)
                .child(Icon::new(Glyph::GitBranch).xsmall())
                .child(div().truncate().child(name))
                .when(changed > 0, |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_color(theme.yellow)
                            .child(format!("● {changed}")),
                    )
                })
                .into_any_element(),
        };
        let files = profile
            .as_ref()
            .filter(|profile| profile.files() > 0)
            .map(|profile| {
                let plus = if profile.is_whole() { "" } else { "+" };
                format!("{}{plus} files", profile.files())
            });
        // The count of files, or what a click does while the mouse is on the card. Only the
        // visibility changes on hover: a display changed by a hover style breaks the paint.
        let corner = div()
            .flex_shrink_0()
            .text_xs()
            .text_color(theme.muted_foreground)
            .group_hover(&group, |files| files.invisible())
            .children(files);
        let alone = div()
            .absolute()
            .right_0()
            .bottom_0()
            .pl_4()
            .text_xs()
            .bg(theme.secondary)
            .text_color(theme.blue)
            .invisible()
            .group_hover(&group, |alone| alone.visible())
            .child(format!("Open with only {name} →"));
        let foot = h_flex()
            .relative()
            .justify_between()
            .gap_3()
            .min_h(px(28.))
            .pt_3()
            .border_t_1()
            .border_color(theme.border)
            .child(div().flex_1().min_w_0().child(git))
            .child(corner)
            .child(alone);

        let path = project.to_path_buf();
        let view = cx.entity().downgrade();
        v_flex()
            .id(("project", ix))
            .group(group)
            .min_w_0()
            .p_4()
            .gap_3()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary.opacity(0.5))
            .cursor_pointer()
            .hover(|card| card.bg(theme.secondary))
            .on_click(move |_, window, cx| {
                view.update(cx, |home, cx| home.open(id, Some(&path), window, cx))
                    .ok();
            })
            .child(head)
            .child(stack)
            .child(foot)
            .into_any_element()
    }

    // A project whose folder is gone: found again elsewhere, or unlinked.
    fn lost(
        &self,
        id: Uuid,
        ix: usize,
        project: &Path,
        name: &str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let warn = theme.warning;
        let (locate, unlink) = (project.to_path_buf(), project.to_path_buf());
        let (locator, unlinker) = (cx.entity().downgrade(), cx.entity().downgrade());
        v_flex()
            .id(("lost-project", ix))
            .min_w_0()
            .p_4()
            .gap_3()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary.opacity(0.5))
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        div()
                            .size(px(32.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(warn.opacity(0.12))
                            .child(Icon::new(Glyph::TriangleAlert).small().text_color(warn)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .line_through()
                                    .text_color(theme.muted_foreground)
                                    .child(name.to_owned()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .font_family(theme.mono_font_family.clone())
                                    .text_color(theme.muted_foreground)
                                    .child(tilde(project)),
                            ),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(warn)
                    .child("Folder not found. It was moved or deleted."),
            )
            .child(
                h_flex()
                    .justify_between()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(Button::ghost(("locate", ix)).label("Locate…").on_click(
                        move |_, window, cx| {
                            let moved = Some(locate.clone());
                            locator
                                .update(cx, |home, cx| home.browse(id, moved, window, cx))
                                .ok();
                        },
                    ))
                    .child(Button::ghost(("unlink", ix)).label("Unlink").on_click(
                        move |_, window, cx| {
                            unlinker
                                .update(cx, |home, cx| home.unlink(id, &unlink, window, cx))
                                .ok();
                        },
                    )),
            )
            .into_any_element()
    }
}

// The brand color of a language, as its logo has it.
fn color(language: &str, fallback: &Hsla) -> Hsla {
    logo(language)
        .map(|(_, hex)| tint(hex))
        .unwrap_or(*fallback)
}

fn percent(part: f32) -> String {
    match (part * 100.).round() as u32 {
        0 => "<1%".to_owned(),
        n => format!("{n}%"),
    }
}
