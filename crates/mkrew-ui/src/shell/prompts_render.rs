//! The body of Settings → Prompts (spec 2026-11-04 section 7.2): the prompt list on the left,
//! the editor on the right. The model is `mkrew_ui::prompts_page`; the actions are in
//! `prompts.rs`.

use crate::*;
use mkrew_ui::prompts_page::{PromptAction, Spot};
use mkrew_ui::settings_dialog::{Effect, Focus};

impl Shell {
    /// The page: a note in a machine window, the page error, then the list and the editor.
    pub(crate) fn render_prompts_page(
        &self,
        dialog: &SettingsDialog,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut page: Vec<gpui::AnyElement> = Vec::new();
        // The picker reads this computer's folder, so a machine window edits it too.
        if self.target.is_machine() {
            page.push(
                meta_line(
                    theme,
                    "These prompts are on this computer; every Mkrew window uses them.".into(),
                )
                .into_any_element(),
            );
        }
        if let Some(error) = &dialog.prompts.error {
            page.push(self.error_line(error.clone(), theme));
        }
        page.push(
            div()
                .flex()
                .items_start()
                .gap(px(space::M))
                .child(self.prompts_list(dialog, theme, cx))
                .child(self.prompt_editor(dialog, typing, theme, cx))
                .into_any_element(),
        );
        page
    }

    /// "New prompt" and one row per prompt: its title (the `# Title` line, else the name) and,
    /// when different, its name; a file that cannot be read shows its error in its own row.
    fn prompts_list(
        &self,
        dialog: &SettingsDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let library = &dialog.prompts.library;
        let selected = dialog
            .prompts
            .draft
            .as_ref()
            .and_then(|draft| draft.origin.as_deref());
        let on_list = dialog.focus == Focus::Prompts(Spot::List);
        let empty = library.prompts.is_empty() && library.errors.is_empty();

        // The filled button while the library is empty, like Machines' first "Add machine…".
        let new_button = if empty {
            button("prompt-new", "New prompt", theme, true).focus_stop(theme)
        } else {
            secondary_button("prompt-new", "New prompt", theme).focusable_bordered(theme)
        }
        .tooltip(tooltip(theme, "Start a new prompt", "Ctrl+N"))
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.settings_do(|_, _| Effect::Prompts(PromptAction::New), window, cx);
        }));

        let rows = library.prompts.iter().map(|prompt| {
            let active = selected == Some(prompt.name.as_str());
            let focused = active && on_list;
            let name = prompt.name.clone();
            div()
                .id(SharedString::from(format!("prompt-row-{}", prompt.name)))
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(active, |el| el.bg(rgb(theme.surface_raised)))
                .when(focused, |el| el.border_color(rgb(theme.selected_ring)))
                .hover(|style| style.bg(rgb(theme.surface_raised)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    let name = name.clone();
                    shell.settings_do(
                        move |dialog, _| {
                            dialog.focus = Focus::Prompts(Spot::List);
                            Effect::Prompts(PromptAction::Open(name))
                        },
                        window,
                        cx,
                    );
                }))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .child(prompt.title.clone()),
                )
                .when(prompt.title != prompt.name, |el| {
                    el.child(meta_line(theme, prompt.name.clone().into()))
                })
        });

        let broken = library.errors.iter().map(|error| {
            let file = error.file.file_name().map_or_else(
                || error.file.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .px(px(space::S))
                .py(px(space::XS))
                .child(
                    div()
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                        .child(div().min_w_0().text_color(rgb(theme.text)).child(file)),
                )
                .child(meta_line(theme, error.message.clone().into()))
        });

        div()
            .flex_none()
            .w(px(theme::SETTINGS_PROMPTS_LIST_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .when(empty, |el| {
                el.child(meta_line(theme, "No prompts yet.".into()))
            })
            .child(div().flex().child(new_button))
            .when(!empty, |el| {
                el.child(
                    div()
                        .id("prompts-list")
                        .max_h(px(theme::SETTINGS_PROMPTS_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.prompts_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows)
                        .children(broken),
                )
            })
            .into_any_element()
    }

    /// The Name field, the prompt text, the variables hint and Save, Delete and Open folder; a
    /// line to pick or start a prompt while none is open.
    fn prompt_editor(
        &self,
        dialog: &SettingsDialog,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(draft) = &dialog.prompts.draft else {
            return div()
                .flex_1()
                .min_w_0()
                .child(meta_line(
                    theme,
                    "Pick a prompt to edit it, or start a new one.".into(),
                ))
                .into_any_element();
        };
        let name_focused = dialog.focus == Focus::Prompts(Spot::Name) && typing;
        let body_focused = dialog.focus == Focus::Prompts(Spot::Body) && typing;

        let name_field = caret_field(
            "prompt-name",
            &draft.name,
            "review",
            name_focused,
            theme,
            cx,
            |shell, caret| {
                let draft = (shell.settings_dialog.as_mut()).and_then(|d| d.prompts.draft.as_mut());
                if let Some(draft) = draft {
                    draft.name.click(caret.at, caret.extend, caret.clicks);
                }
            },
        )
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.settings_do(
                |dialog, _| {
                    dialog.focus = Focus::Prompts(Spot::Name);
                    Effect::None
                },
                window,
                cx,
            );
        }));

        // A click puts the caret under the pointer, a drag selects (SPEC §13).
        let body_field = caret_field(
            "prompt-body",
            draft.body.input(),
            "Write the prompt…",
            body_focused,
            theme,
            cx,
            |shell, caret| {
                let draft = (shell.settings_dialog.as_mut()).and_then(|d| d.prompts.draft.as_mut());
                if let Some(draft) = draft {
                    draft.body.click(caret.at, caret.extend, caret.clicks);
                }
            },
        )
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.settings_do(
                |dialog, _| {
                    dialog.focus = Focus::Prompts(Spot::Body);
                    Effect::None
                },
                window,
                cx,
            );
        }))
        .h(px(theme::SETTINGS_PROMPT_BODY_HEIGHT))
        .overflow_y_scroll()
        .track_scroll(&self.prompts_body_scroll);

        let save = if draft.is_dirty() {
            button("prompt-save", "Save", theme, true)
                .focus_stop(theme)
                .tooltip(tooltip(theme, "Save the prompt", "Ctrl+S"))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.settings_do(|_, _| Effect::Prompts(PromptAction::Save), window, cx);
                }))
        } else {
            disabled_button("prompt-save", "Save", theme, true)
        };
        let delete = if draft.origin.is_some() {
            danger_button("prompt-delete", "Delete", theme)
                .focus_stop(theme)
                .tooltip(tooltip(theme, "Delete this prompt (asks first)", ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.settings_do(|_, _| Effect::Prompts(PromptAction::Delete), window, cx);
                }))
        } else {
            disabled_button("prompt-delete", "Delete", theme, false)
        };
        let folder = secondary_button("prompt-folder", "Open folder", theme)
            .focusable_bordered(theme)
            .tooltip(tooltip(theme, "Show the prompts folder in Zed", ""))
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.settings_do(|_, _| Effect::Prompts(PromptAction::OpenFolder), window, cx);
            }));

        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .child(meta_line(theme, "Name".into()))
            .child(name_field)
            .children(draft.name_error.as_ref().map(|message| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_exited))
                    .child(message.clone())
            }))
            .child(meta_line(theme, "Prompt".into()))
            .child(body_field)
            .child(meta_line(
                theme,
                "Variables: {file}, {selection}, {task}, {project}. A first line “# Title” names the prompt in lists."
                    .into(),
            ))
            .child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(save)
                    .child(delete)
                    .child(folder),
            )
            .into_any_element()
    }
}
