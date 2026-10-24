//! The Settings dialog (ADR 0073): every `settings.toml` key and every shortcut, applied at
//! once. The model is `mkrew_ui::settings_dialog`; this file writes the files and draws it.

use crate::*;
use mkrew_core::settings_fields::{Kind, Setting};
use mkrew_ui::settings_dialog::{Effect, Focus, Row, Tab};

impl Shell {
    /// Ctrl+, and the palette's "Global: Open settings". Needs no daemon.
    pub(crate) fn open_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.close_menus();
        let settings =
            mkrew_core::settings::load(&self.settings_file).map_err(|error| error.to_string());
        let mut fonts: Vec<String> = cx.text_system().all_font_names();
        fonts.sort_unstable_by_key(|font| font.to_lowercase());
        fonts.dedup();
        self.settings_dialog = Some(SettingsDialog::new(settings, fonts));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_settings_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.settings_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn settings_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let paste = (task_dialog::dialog_key(&event.keystroke) == Some(DialogKey::Paste))
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let keymap = self.keymap.clone();
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        let effect = dialog.key(&event.keystroke, paste.as_deref(), &keymap);
        self.apply_settings_effect(effect, window, cx);
    }

    /// Writes what the dialog asked for and feeds the result back to it.
    pub(crate) fn apply_settings_effect(
        &mut self,
        effect: Effect,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match effect {
            Effect::None => {}
            Effect::Close => return self.close_settings_dialog(window, cx),
            Effect::Write { setting, text } => {
                let result = setting
                    .write(&self.settings_file, &text)
                    .map_err(|error| error.to_string());
                let ok = result.is_ok();
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.written(setting, result);
                }
                if ok {
                    // Now, not at the next mtime poll (or window focus).
                    self.settings_modified = std::fs::metadata(&self.settings_file)
                        .and_then(|m| m.modified())
                        .ok();
                    self.apply_settings_file(window, cx);
                }
            }
            Effect::Bind { command, keys } => {
                let result = keymap::write_binding(&self.keymap_file, command, keys.as_deref());
                let reply = match result {
                    Ok((keymap, errors)) => {
                        self.set_keymap(keymap, errors, cx);
                        Ok(())
                    }
                    Err(error) => Err(settings_dialog::binding_error(&error)),
                };
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.bound(command, reply);
                }
            }
        }
        cx.notify();
    }

    fn settings_do(
        &mut self,
        act: impl FnOnce(&mut SettingsDialog, &Keymap) -> Effect,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.dialog_focus, cx);
        let keymap = self.keymap.clone();
        if let Some(effect) = self.settings_dialog.as_mut().map(|d| act(d, &keymap)) {
            self.apply_settings_effect(effect, window, cx);
        }
    }
}

impl Shell {
    pub(crate) fn render_settings_dialog(
        &self,
        dialog: &SettingsDialog,
        viewport_height: Pixels,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let tabs = Tab::all().into_iter().map(|tab| {
            let active = tab == dialog.tab;
            let focused = active && dialog.focus == Focus::Tabs;
            div()
                .id(SharedString::from(format!("settings-tab-{}", tab.title())))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .text_size(font_size::BODY)
                .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                .when(active, |el| el.bg(rgb(theme.surface_raised)))
                .when(focused, |el| {
                    el.border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(theme.selected_ring))
                })
                .hover(|style| style.bg(rgb(theme.surface_raised)))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.settings_do(|d, _| d.set_tab(tab), window, cx);
                }))
                .child(tab.title())
        });
        let nav = div()
            .w(px(theme::SETTINGS_NAV_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::M))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .children(tabs);

        let banner = match (&dialog.settings, dialog.tab) {
            (Err(error), Tab::Settings(_)) => {
                Some(self.settings_banner(error.clone(), true, theme, cx))
            }
            (_, Tab::Keyboard) => keymap::notice(&self.keymap_errors)
                .map(|notice| self.settings_banner(notice, false, theme, cx)),
            _ => None,
        };
        let search = (dialog.tab == Tab::Keyboard).then(|| {
            text_field(
                "settings-search",
                dialog.search.as_str(),
                "Search shortcuts",
                dialog.focus == Focus::Search,
                false,
                theme,
            )
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.settings_do(|d, keymap| d.click(Focus::Search, keymap), window, cx);
            }))
        });
        let rows = dialog.rows(&self.keymap);
        let body = div()
            .id("settings-rows")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.settings_scroll)
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::M))
            .children(banner)
            .children(search)
            .children(
                rows.iter()
                    .enumerate()
                    .map(|(i, row)| self.settings_row(dialog, i, *row, theme, cx)),
            );

        let footer = div()
            .flex()
            .items_center()
            .gap(px(space::L))
            .px(px(space::L))
            .py(px(space::M))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .text_size(font_size::META)
            .child(self.settings_link(
                "settings-edit-toml",
                "Edit settings.toml in Zed",
                theme,
                cx,
                |shell, cx| {
                    shell.edit_settings_file(cx);
                },
            ))
            .child(self.settings_link(
                "settings-edit-keymap",
                "Edit keymap.toml in Zed",
                theme,
                cx,
                |shell, cx| {
                    shell.edit_keymap_file(cx);
                },
            ))
            .child(div().flex_1())
            .child(
                div()
                    .text_color(rgb(theme.text_muted))
                    .child("Changes apply at once · Esc closes"),
            );

        let max_height = (viewport_height - px(space::XXL * 2.0)).max(px(0.0));
        let panel = div()
            .id("settings-dialog")
            .w(px(theme::SETTINGS_DIALOG_WIDTH))
            .h(px(theme::SETTINGS_DIALOG_HEIGHT))
            .max_h(max_height)
            .flex()
            .flex_col()
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::settings_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::settings_key_down, cx))
            .child(
                div()
                    .px(px(space::L))
                    .py(px(space::M))
                    .border_b(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.divider))
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Settings"),
            )
            .child(div().flex().flex_1().min_h_0().child(nav).child(body))
            .child(footer);
        Self::modal_shade("settings-shade", panel, cx)
    }

    /// The broken-file / keymap-problem line above the rows, with "Fix in Zed".
    fn settings_banner(
        &self,
        text: String,
        settings_file: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            .text_size(font_size::META)
            .child(div().text_color(rgb(theme.status_exited)).child("✕"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(rgb(theme.text))
                    .child(text),
            )
            .child(
                self.settings_link("settings-fix", "Fix in Zed", theme, cx, move |shell, cx| {
                    if settings_file {
                        shell.edit_settings_file(cx);
                    } else {
                        shell.edit_keymap_file(cx);
                    }
                }),
            )
    }

    fn settings_link(
        &self,
        id: &'static str,
        label: &'static str,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
        on_click: impl Fn(&mut Self, &mut Context<'_, Self>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .cursor_pointer()
            .text_color(rgb(theme.accent_text))
            .hover(|style| style.underline())
            .on_click(cx.listener(move |shell, _, _, cx| on_click(shell, cx)))
            .child(label)
    }

    fn settings_row(
        &self,
        dialog: &SettingsDialog,
        index: usize,
        row: Row,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = dialog.focus == Focus::Row(index);
        let error = dialog.errors.get(&row.id()).cloned();
        let (title, help, control): (SharedString, Option<String>, gpui::AnyElement) = match row {
            Row::Setting(setting) => (
                setting.label.into(),
                (!setting.help.is_empty()).then(|| setting.help.to_owned()),
                self.settings_control(dialog, setting, focused, theme, cx),
            ),
            Row::Binding(command) => (
                palette::title(command).into(),
                command
                    .context()
                    .map(|context| format!("{} only", context_name(context))),
                self.binding_control(dialog, command, theme, cx),
            ),
        };
        let is_default = match row {
            Row::Setting(setting) => dialog.settings.as_ref().is_ok_and(|settings| {
                setting.current(settings) == setting.current(&Default::default())
            }),
            Row::Binding(_) => false,
        };
        div()
            .id(SharedString::from(format!("settings-row-{}", row.id())))
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .when(focused, |el| el.bg(rgb(theme.surface_raised)))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.settings_do(|d, keymap| d.click(Focus::Row(index), keymap), window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .gap(px(space::S))
                                    .text_size(font_size::BODY)
                                    .text_color(rgb(theme.text))
                                    .child(title)
                                    .when(is_default, |el| {
                                        el.child(
                                            div()
                                                .text_size(font_size::META)
                                                .text_color(rgb(theme.text_muted))
                                                .child("default"),
                                        )
                                    }),
                            )
                            .children(help.map(|help| meta_line(theme, help.into()))),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(theme::SETTINGS_CONTROL_WIDTH))
                            .flex()
                            .justify_end()
                            .child(control),
                    ),
            )
            .children(error.map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_exited))
                    .child(error)
            }))
            .children(self.font_list(dialog, row, theme, cx))
            .into_any_element()
    }

    fn settings_control(
        &self,
        dialog: &SettingsDialog,
        setting: &'static Setting,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let shown = dialog.shown(setting);
        let read_only = dialog.read_only();
        let ring = if focused {
            theme.selected_ring
        } else {
            theme.border
        };
        match setting.kind {
            Kind::Bool => {
                let on = shown == "true";
                let knob = px(theme::SWITCH_HEIGHT - 2.0 * theme::SWITCH_INSET);
                div()
                    .id(SharedString::from(format!("switch-{}", setting.id())))
                    .flex_none()
                    .w(px(theme::SWITCH_WIDTH))
                    .h(px(theme::SWITCH_HEIGHT))
                    .p(px(theme::SWITCH_INSET))
                    .rounded_full()
                    .flex()
                    .when(on, |el| el.justify_end())
                    .bg(rgb(if on { theme.accent } else { theme.border }))
                    .when(focused, |el| {
                        el.border(px(theme::BORDER_WIDTH)).border_color(rgb(ring))
                    })
                    .when(read_only, |el| el.opacity(opacity::DISABLED))
                    .when(!read_only, |el| el.cursor_pointer())
                    .child(div().size(knob).rounded_full().bg(rgb(theme.surface)))
                    .into_any_element()
            }
            Kind::Choice(choices) => div()
                .flex()
                .gap(px(space::XS))
                .p(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(ring))
                .when(read_only, |el| el.opacity(opacity::DISABLED))
                .children(choices.iter().map(|&choice| {
                    let active = shown == choice;
                    div()
                        .id(SharedString::from(format!(
                            "choice-{}-{choice}",
                            setting.id()
                        )))
                        .px(px(space::S))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::META)
                        .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                        .when(active, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!active && !read_only, |el| {
                            el.cursor_pointer().on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    cx.stop_propagation();
                                    shell.settings_do(|d, _| d.choose(setting, choice), window, cx);
                                },
                            ))
                        })
                        .child(choice)
                }))
                .into_any_element(),
            Kind::Text | Kind::Number => {
                let editing = dialog
                    .draft
                    .as_ref()
                    .filter(|(s, _)| s.id() == setting.id());
                let field = match editing {
                    Some((_, input)) => {
                        caret_field("settings-value", input, "", true, theme, cx, |_, _| {})
                    }
                    None => text_field("settings-value", &shown, "", false, false, theme),
                };
                field
                    .w_full()
                    .when(read_only, |el| el.opacity(opacity::DISABLED))
                    .into_any_element()
            }
            Kind::Font => div()
                .w_full()
                .flex()
                .justify_between()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(ring))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .when(read_only, |el| el.opacity(opacity::DISABLED))
                .child(if shown.is_empty() {
                    "Default".to_owned()
                } else {
                    shown
                })
                .child(div().text_color(rgb(theme.text_muted)).child("▾"))
                .into_any_element(),
        }
    }

    /// The shortcut chip (or "Press keys…"), the changed dot and Reset.
    fn binding_control(
        &self,
        dialog: &SettingsDialog,
        command: Command,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let capturing = dialog.capture == Some(command);
        let changed = self.keymap.changed(command);
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .when(changed, |el| {
                el.child(
                    div()
                        .size(px(space::S))
                        .rounded_full()
                        .bg(rgb(theme.accent)),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("reset-{}", command.name())))
                        .px(px(space::S))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .hover(|style| style.bg(rgb(theme.surface_raised)))
                        .tooltip(tooltip(
                            theme,
                            &format!("Back to {}", keymap::keys_label(&command.default_keys())),
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.settings_do(|d, _| d.reset(command), window, cx);
                        }))
                        .child("Reset"),
                )
            })
            .child(if capturing {
                kbd("Press keys…".into(), theme, false)
                    .border_color(rgb(theme.accent))
                    .text_color(rgb(theme.accent_text))
            } else {
                kbd(self.keymap.label(command), theme, false)
            })
            .into_any_element()
    }

    /// The font pick under its row: the query, then the matching fonts (Default first).
    fn font_list(
        &self,
        dialog: &SettingsDialog,
        row: Row,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let pick = dialog.font_pick.as_ref()?;
        if row != Row::Setting(pick.setting) {
            return None;
        }
        let setting = pick.setting;
        let selected = pick.selected;
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .child(caret_field(
                    "settings-font-query",
                    &pick.query,
                    "Search fonts",
                    true,
                    theme,
                    cx,
                    |_, _| {},
                ))
                .child(
                    div()
                        .id("settings-font-list")
                        .max_h(px(theme::SETTINGS_FONT_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .children(dialog.font_choices().into_iter().enumerate().map(
                            |(i, font)| {
                                let label = if font.is_empty() {
                                    "Default".to_owned()
                                } else {
                                    font.clone()
                                };
                                menu_item(SharedString::from(format!("font-choice-{i}")), theme)
                                    .when(i == selected, |el| el.bg(rgb(theme.surface_raised)))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        cx.stop_propagation();
                                        let font = font.clone();
                                        shell.settings_do(
                                            move |d, _| d.choose(setting, &font),
                                            window,
                                            cx,
                                        );
                                    }))
                                    .child(label)
                            },
                        )),
                )
                .into_any_element(),
        )
    }
}

/// `"Terminal"` from a key context like `Terminal` or `Git && !Editing`, for "Terminal only".
fn context_name(context: &str) -> String {
    let word = context
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())
        .unwrap_or(context);
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
