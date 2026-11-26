//! The Settings dialog (ADR 0073): every `settings.toml` key and every shortcut, applied at
//! once. The model is `mkrew_ui::settings_dialog`; this file writes the files and draws it.

use crate::*;
use mkrew_core::settings_fields::{Kind, Setting};
use mkrew_ui::prompts_page::Spot;
use mkrew_ui::settings_dialog::{Effect, Focus, Hit, Row, Tab};

impl Shell {
    /// Ctrl+, (⌘, on macOS) and the palette's "Global: Open settings". Needs no daemon.
    pub(crate) fn open_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.close_menus();
        let settings =
            mkrew_core::settings::load(&self.settings_file).map_err(|error| error.to_string());
        let mut fonts: Vec<String> = cx.text_system().all_font_names();
        fonts.sort_unstable_by_key(|font| font.to_lowercase());
        fonts.dedup();
        let mut dialog = SettingsDialog::new(settings, fonts);
        dialog.set_catalog(cx.global::<ThemeCatalog>().clone());
        // The dialog this one replaces may have been previewing a card: the kept theme comes
        // back (the new dialog starts with no preview).
        if let Some((mode, _)) = self.gallery_preview.take() {
            theme::refresh(mode, window, cx);
            cx.refresh_windows();
        }
        self.settings_dialog = Some(dialog);
        self.settings_from_composer =
            self.composer_session(window).is_some() && self.composer_focus.is_focused(window);
        // The task dialog under it stays the open dialog: its late replies (the image save,
        // the agent list) still land.
        if self.task_dialog.is_none() {
            self.dialog_serial += 1;
        }
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_settings_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.restore_theme_pick(window, cx);
        self.settings_dialog = None;
        if std::mem::take(&mut self.settings_from_composer)
            && self.composer_session(window).is_some()
        {
            window.focus(&self.composer_focus, cx);
        } else {
            self.focus_pane(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn settings_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let back = event.keystroke.modifiers.shift;
        let edge = (self.settings_dialog.as_ref())
            .is_some_and(|dialog| dialog.chain_edge(back, &self.keymap));
        if edge && event.keystroke.key == "tab" && self.dialog_focus.is_focused(window) {
            // Leaving the rows writes the draft of the row it leaves, like Tab to the next row.
            let effect = self.settings_dialog.as_mut().map(SettingsDialog::leave);
            if let Some(effect) = effect {
                self.apply_settings_effect(effect, window, cx);
            }
        }
        let keymap = self.keymap.clone();
        if self.dialog_nav(event, edge, window, cx, move |shell, forward| {
            if let Some(dialog) = shell.settings_dialog.as_mut() {
                dialog.enter(forward, &keymap);
            }
        }) {
            return;
        }
        cx.stop_propagation();
        let key = task_dialog::dialog_key(&event.keystroke);
        // Ctrl+N and Ctrl+S of the Prompts page (Cmd on macOS) work from a button too, and so does
        // Ctrl+F (Cmd+F).
        let find_key = settings_dialog::find_shortcut(&event.keystroke);
        let page_key = settings_dialog::prompts_shortcut(&event.keystroke).is_some();
        let control = self.dialog_control_focused(window);
        // A button or link has the keyboard: the rows take nothing but Esc / Ctrl+Enter.
        if control
            && !page_key
            && !find_key
            && !matches!(key, Some(DialogKey::Cancel | DialogKey::Save))
        {
            return;
        }
        // These keys move the dialog's own focus (the search box, a prompt's Name): the keyboard
        // follows, as a click does, or the next key would hit the early return above.
        let on_prompts = (self.settings_dialog.as_ref())
            .is_some_and(|d| d.tab == Tab::Prompts && !d.searching());
        if control && (find_key || (page_key && on_prompts)) {
            window.focus(&self.dialog_focus, cx);
        }
        let paste = (key == Some(DialogKey::Paste))
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let keymap = self.keymap.clone();
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        if let (Some(key), Some(input)) = (&key, dialog.typing()) {
            copy_selection(key, input, cx);
        }
        let effect = dialog.key(&event.keystroke, paste.as_deref(), &keymap);
        self.apply_settings_effect(effect, window, cx);
        // Typing at the end of a long prompt keeps its last line in view, like the task dialog.
        if let Some(dialog) = &self.settings_dialog
            && dialog.focus == Focus::Prompts(Spot::Body)
            && let Some(draft) = &dialog.prompts.draft
            && draft.body.input().cursor() == draft.body.input().len()
        {
            self.prompts_body_scroll.scroll_to_bottom();
        }
        // Keep the focused row on screen: the banner, the machine's daemon note, the Keyboard
        // search box and the theme gallery come before the cards, and a card holds several
        // rows. A gallery card is one of many in a child taller than the dialog, so it
        // scrolls itself into view.
        if let Some(dialog) = &self.settings_dialog {
            match dialog.focus {
                Focus::Row(index) => {
                    let banner = usize::from(self.settings_notice(dialog).is_some());
                    let note = usize::from(
                        (dialog.searching() || matches!(dialog.tab, Tab::Settings(_)))
                            && self.machine_name().is_some(),
                    );
                    let search = usize::from(keyboard_search_shown(dialog));
                    let gallery = usize::from(dialog.gallery_shown());
                    let card = dialog.group_of(index, &self.keymap);
                    // The page head is the first scroll child.
                    self.settings_scroll
                        .scroll_to_item(1 + card + banner + note + search + gallery);
                }
                Focus::Gallery(_) if dialog.gallery_shown() => self.gallery_reveal.set(true),
                _ => {}
            }
        }
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
            // Esc: unsaved prompt text asks first (Save / Don't save / Cancel).
            Effect::Close => {
                let then: AfterSave =
                    Box::new(|shell, window, cx| shell.close_settings_dialog(window, cx));
                return self.leave_prompt_draft(then, window, cx);
            }
            // ADR 0115: the window mode changes through its flows, which save it: the split
            // picker (nothing is saved if it is cancelled) or the combine.
            Effect::Write { setting, text } if setting.id() == "ui.window_mode" => {
                let wanted = if text == WindowMode::PerWorkspace.name() {
                    WindowMode::PerWorkspace
                } else {
                    WindowMode::Single
                };
                if wanted == self.window_mode {
                    return;
                }
                self.close_settings_dialog(window, cx);
                match wanted {
                    WindowMode::PerWorkspace => self.open_split_picker(window, cx),
                    WindowMode::Single => self.combine_windows(window, cx),
                }
            }
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
                // GPUI folds Shift+digit keys; a clash the file check cannot see is refused here.
                let clash = keys.as_deref().and_then(|keys| {
                    keymap::folded_clash(&self.keymap, command, keys, cx.keyboard_mapper().as_ref())
                        .map(|other| keymap::KeymapError::Duplicate {
                            command: command.name(),
                            keys: keys.to_owned(),
                            other: other.name(),
                        })
                });
                let result = match clash {
                    Some(error) => Err(error),
                    None => keymap::write_binding(&self.keymap_file, command, keys.as_deref()),
                };
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
            Effect::KeepTheme { name, dark } => self.keep_theme(&name, dark, window, cx),
            Effect::AddMachine(machine) => self.add_machine(machine, cx),
            Effect::Prompts(action) => self.prompts_action(action, window, cx),
        }
        // A hover, a keyboard move or Esc changed what the gallery previews.
        self.sync_theme_preview(window, cx);
        // The Prompts page reads its folder when it opens and after every write.
        self.sync_prompts();
        cx.notify();
    }

    pub(crate) fn settings_do(
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

/// Whether the Keyboard page's own search box is drawn (a global query hides it).
fn keyboard_search_shown(dialog: &SettingsDialog) -> bool {
    dialog.tab == Tab::Keyboard && !dialog.searching()
}

impl Shell {
    pub(crate) fn render_settings_dialog(
        &self,
        dialog: &SettingsDialog,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let viewport_height = window.viewport_size().height;
        // The model's field paints a caret only while the dialog's own text has the keyboard,
        // not while Tab put it on a button or link.
        let typing = self.dialog_focus.is_focused(window);
        let dimmed = dialog.dimmed_tabs();
        let tab_item = |tab: Tab| {
            let active = tab == dialog.tab && !dialog.searching();
            let focused = tab == dialog.tab && dialog.focus == Focus::Tabs;
            with_pick(
                div()
                    .id(SharedString::from(format!("settings-tab-{}", tab.title())))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .h(theme::rems_px(theme::SETTINGS_NAV_ROW_HEIGHT))
                    .px(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(if focused {
                        rgb(theme.selected_ring).into()
                    } else {
                        gpui::transparent_black()
                    })
                    .text_size(font_size::BODY)
                    .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                    // The active page keeps its tint and the pointer; the others hover.
                    .pressable_unless(active, Look::Row, theme)
                    // Dimmed, still readable and still clickable.
                    .when(dimmed.contains(&tab), |el| el.opacity(opacity::DIMMED))
                    .on_press(
                        true,
                        cx.listener(move |shell, _, window, cx| {
                            shell.settings_do(|d, _| d.set_tab(tab), window, cx);
                        }),
                    )
                    .child(icon_svg(
                        tab.icon(),
                        theme::ICON_SIZE_SMALL,
                        if active {
                            theme.accent_text
                        } else {
                            theme.text_subtle
                        },
                    ))
                    .child(tab.title()),
                active,
                theme,
            )
            .into_any_element()
        };
        let nav_groups =
            div()
                .flex()
                .flex_col()
                .gap(px(space::L))
                .children(settings_dialog::NAV.iter().map(|(title, tabs)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .child(section_title(title, theme).px(px(space::S)))
                        .children(tabs.iter().map(|&tab| tab_item(tab)))
                }));
        let find = search_box(
            caret_field(
                "settings-find",
                &dialog.find,
                "Search",
                dialog.focus == Focus::Find && typing,
                theme,
                cx,
                |shell, caret| {
                    if let Some(d) = shell.settings_dialog.as_mut() {
                        d.find.click(caret.at, caret.extend, caret.clicks);
                    }
                },
            )
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.settings_do(|d, keymap| d.click(Focus::Find, keymap), window, cx);
            })),
            Some(
                kbd(mkrew_ui::os_text!("Ctrl+F", "⌘F").to_owned(), theme, false).into_any_element(),
            ),
            dialog.focus == Focus::Find,
            theme,
        );
        let nav = div()
            .w(px(theme::SETTINGS_NAV_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::L))
            .px(px(space::M))
            .py(px(space::L))
            .bg(rgb(theme.well))
            // gpui does not clip a fill to the panel's rounded corners.
            .rounded_tl(px(radius::PANEL))
            .rounded_bl(px(radius::PANEL))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.layer[1]))
            .child(find)
            .child(nav_groups);

        let banner = self
            .settings_notice(dialog)
            .map(|(text, settings_file)| self.settings_banner(text, settings_file, theme, cx));
        let search = keyboard_search_shown(dialog).then(|| {
            caret_field(
                "settings-search",
                &dialog.search,
                "Search shortcuts",
                dialog.focus == Focus::Search && typing,
                theme,
                cx,
                |shell, caret| {
                    if let Some(d) = shell.settings_dialog.as_mut() {
                        d.search.click(caret.at, caret.extend, caret.clicks);
                    }
                },
            )
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.settings_do(|d, keymap| d.click(Focus::Search, keymap), window, cx);
            }))
        });
        let rows = dialog.rows(&self.keymap);
        // The pages edit this computer's `settings.toml`; the daemon behind a machine window
        // reads its own.
        let results_or_settings = dialog.searching() || matches!(dialog.tab, Tab::Settings(_));
        let daemon_note = results_or_settings
            .then(|| self.machine_name())
            .flatten()
            .map(|name| meta_line(theme, mkrew_ui::machines::daemon_settings_note(name).into()));
        // The Keyboard page is one card of pieces: no gap between them, so the lines above it
        // carry their own.
        let one_card = dialog.tab == Tab::Keyboard && !dialog.searching();
        let below = if one_card { space::M } else { 0.0 };
        // Each direct child of the scroll container is a 740 px column, so
        // `settings_scroll.scroll_to_item(n)` indexes one child per item, the page head first.
        let column = |el: gpui::AnyElement| {
            div()
                .w_full()
                .max_w(px(theme::SETTINGS_CONTENT_MAX_WIDTH))
                .child(el)
                .into_any_element()
        };
        let page_head = div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .pb(px(space::L))
            .child(
                div()
                    .text_size(font_size::VIEW_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(dialog.heading()),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text_subtle))
                    .child(dialog.lead()),
            );
        let body = div()
            .id("settings-rows")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.settings_scroll)
            .flex()
            .flex_col()
            .items_center()
            .gap(px(if one_card { 0.0 } else { space::M }))
            .px(px(space::XXL))
            .py(px(space::XXL))
            .child(column(page_head.into_any_element()))
            .children(banner.map(|banner| column(banner.mb(px(below)).into_any_element())))
            .children(daemon_note.map(|note| column(note.mb(px(below)).into_any_element())))
            .children(
                (dialog.tab == Tab::Machines && !dialog.searching())
                    .then(|| self.render_machines_page(dialog, typing, theme, cx))
                    .into_iter()
                    .flatten()
                    .map(column),
            )
            .children(
                (dialog.tab == Tab::Prompts && !dialog.searching())
                    .then(|| self.render_prompts_page(dialog, typing, theme, cx))
                    .into_iter()
                    .flatten()
                    .map(column),
            )
            .children(search.map(|search| column(search.mb(px(below)).into_any_element())))
            .children(
                dialog
                    .gallery_shown()
                    .then(|| column(self.render_theme_gallery(dialog, theme, cx))),
            )
            .children(
                self.settings_cards(dialog, &rows, typing, theme, cx)
                    .into_iter()
                    .map(column),
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
            .child(
                div()
                    .text_color(rgb(theme.text_subtle))
                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
            )
            .child(self.settings_link(
                "settings-edit-toml",
                "settings.toml",
                theme,
                cx,
                |shell, cx| {
                    shell.edit_settings_file(cx);
                },
            ))
            .child(self.settings_link(
                "settings-edit-keymap",
                "keymap.toml",
                theme,
                cx,
                |shell, cx| {
                    shell.edit_keymap_file(cx);
                },
            ))
            .child(div().flex_1())
            .child(div().text_color(rgb(theme.text_subtle)).child(
                // The Prompts page saves by itself, so its own keys stay in the hint.
                if dialog.tab == Tab::Prompts && !dialog.searching() {
                    mkrew_ui::os_text!(
                        "Ctrl+N new · Ctrl+S saves · Esc closes",
                        "⌘N new · ⌘S saves · Esc closes"
                    )
                } else {
                    "Changes apply at once."
                },
            ));

        let max_height = (viewport_height - px(space::XXL * 2.0)).max(px(0.0));
        let max_width = (window.viewport_size().width - px(space::XXL * 2.0)).max(px(0.0));
        let right = div()
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(body)
            .child(footer)
            .child(
                div()
                    .absolute()
                    .top(px(space::M))
                    .right(px(space::M))
                    .child(dialog_close("settings-close", theme).on_click(
                        cx.listener(|shell, _, window, cx| shell.close_settings_dialog(window, cx)),
                    )),
            );
        let panel = floating_panel("settings-dialog", theme)
            .w(px(theme::SETTINGS_DIALOG_WIDTH))
            .h(px(theme::SETTINGS_DIALOG_HEIGHT))
            .max_w(max_width)
            .max_h(max_height)
            .flex()
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::settings_key_down),
            )
            .relative()
            .child(self.typing(&self.dialog_focus, Self::settings_key_down, cx))
            .child(nav)
            .child(right);
        Self::modal_shade("settings-shade", panel, cx)
    }

    /// The line above the rows: a broken `settings.toml` on a settings page or above search
    /// results (they are greyed then), or a keymap problem on the Keyboard page.
    fn settings_notice(&self, dialog: &SettingsDialog) -> Option<(String, bool)> {
        match &dialog.settings {
            Err(error) if dialog.searching() || matches!(dialog.tab, Tab::Settings(_)) => {
                Some((error.clone(), true))
            }
            _ if dialog.tab == Tab::Keyboard && !dialog.searching() => {
                keymap::notice(&self.keymap_errors).map(|notice| (notice, false))
            }
            _ => None,
        }
    }

    /// The right side's rows: the empty state for a query without a match, else the cards.
    fn settings_cards(
        &self,
        dialog: &SettingsDialog,
        rows: &[Row],
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let searching = dialog.searching();
        if searching && rows.is_empty() {
            return vec![
                empty_state(
                    theme,
                    None,
                    None,
                    format!("No setting matches “{}”.", dialog.find.trim()).into(),
                    None,
                )
                .child(
                    secondary_button("settings-clear-search", "Clear search (Esc)", theme)
                        .mt(px(space::XS))
                        .on_press(
                            true,
                            cx.listener(|shell, _, window, cx| {
                                shell.settings_do(|d, _| d.clear_find(), window, cx);
                            }),
                        ),
                )
                .into_any_element(),
            ];
        }
        let hits = dialog.hits();
        let groups = settings_dialog::grouped(rows, searching);
        let total = groups.len();
        groups
            .into_iter()
            .enumerate()
            .map(|(at_group, group)| {
                let flat = group.title.is_empty();
                // A shortcut: one group per row, but the Keyboard page is one card.
                let shortcut = flat && !searching;
                let title = group.title;
                let count = group.rows.len();
                let items = group
                    .rows
                    .into_iter()
                    .enumerate()
                    .map(|(at, (index, row))| {
                        let (first, last) = if shortcut {
                            (at_group == 0, at_group + 1 == total)
                        } else {
                            (at == 0, at + 1 == count)
                        };
                        self.settings_row(
                            dialog,
                            index,
                            row,
                            first,
                            last,
                            hits.get(index),
                            typing,
                            theme,
                            cx,
                        )
                    })
                    .collect::<Vec<_>>();
                let card = div()
                    .flex()
                    .flex_col()
                    .bg(rgb(theme.surface))
                    .border_color(gpui::rgba(theme.layer[1]));
                if shortcut {
                    // The Keyboard page stays a flat list inside one card. Each row is a
                    // piece of it (the page has no gap), so the scroll can still follow a row.
                    card.border_x(px(theme::BORDER_WIDTH))
                        .when(at_group == 0, |el| {
                            el.border_t(px(theme::BORDER_WIDTH))
                                .rounded_t(px(radius::LARGE))
                        })
                        .when(at_group + 1 == total, |el| {
                            el.border_b(px(theme::BORDER_WIDTH))
                                .rounded_b(px(radius::LARGE))
                        })
                        .children(items)
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space::S))
                        .children((!flat).then(|| {
                            section_title(title, theme)
                                .px(px(space::S))
                                .pt(px(space::M))
                        }))
                        .child(
                            card.rounded(px(radius::LARGE))
                                .border(px(theme::BORDER_WIDTH))
                                .children(items),
                        )
                        .into_any_element()
                }
            })
            .collect()
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
            .child(error_note(text, theme).flex_1().min_w_0())
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
            .text_color(rgb(theme.accent_text))
            .pressable(Look::Text, true, theme)
            .on_press(
                true,
                cx.listener(move |shell, _, _, cx| on_click(shell, cx)),
            )
            .focus_stop(theme)
            .child(label)
    }

    /// A click on a row, or on its switch or dropdown trigger: the keyboard moves to the row
    /// and the click acts like Enter (`SettingsDialog::click`).
    fn press_row(index: usize, cx: &Context<'_, Self>) -> PressHandler {
        Box::new(cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            shell.settings_do(|d, keymap| d.click(Focus::Row(index), keymap), window, cx);
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn settings_row(
        &self,
        dialog: &SettingsDialog,
        index: usize,
        row: Row,
        first: bool,
        last: bool,
        hit: Option<&Hit>,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = dialog.focus == Focus::Row(index);
        let error = dialog.errors.get(&row.id()).cloned();
        let (title, help, control): (SharedString, Option<String>, gpui::AnyElement) = match row {
            Row::Setting(setting) => (
                setting.label.into(),
                (!setting.help.is_empty()).then(|| setting.help.to_owned()),
                self.settings_control(dialog, index, setting, focused, typing, theme, cx),
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
        // A search result names its place: "Appearance › Fonts".
        let crumb = hit.map(|hit| format!("{} › {}", hit.setting.page.title(), hit.setting.group));
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let label: gpui::AnyElement = match hit {
            Some(hit) => gpui::StyledText::new(title.clone())
                .with_highlights(hit.label_ranges.iter().cloned().map(|range| {
                    (
                        range,
                        gpui::HighlightStyle {
                            color: Some(accent),
                            font_weight: Some(FontWeight(weight::SEMIBOLD)),
                            ..gpui::HighlightStyle::default()
                        },
                    )
                }))
                .into_any_element(),
            None => title.into_any_element(),
        };
        with_pick(
            div()
                .id(SharedString::from(format!("settings-row-{}", row.id())))
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .min_h(theme::rems_px(theme::SETTINGS_ROW_MIN_HEIGHT))
                .px(px(space::L))
                .py(px(space::M))
                // The first and last row follow the card's rounded corners: gpui does not clip a
                // fill to them.
                .when(first, |el| el.rounded_t(px(radius::LARGE)))
                .when(last, |el| el.rounded_b(px(radius::LARGE)))
                .when(!first, |el| {
                    el.border_t(px(theme::BORDER_WIDTH))
                        .border_color(rgb(theme.divider))
                })
                .pressable_unless(focused, Look::Row, theme),
            focused,
            theme,
        )
        .on_press(true, Self::press_row(index, cx))
        .children(crumb.map(|crumb| meta_line(theme, crumb.into())))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::XL))
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
                                .font_weight(FontWeight(weight::MEDIUM))
                                .text_color(rgb(theme.text))
                                .child(label)
                                .when(is_default, |el| {
                                    el.child(
                                        div()
                                            .text_size(font_size::META)
                                            .text_color(rgb(theme.text_subtle))
                                            .child("default"),
                                    )
                                }),
                        )
                        .children(help.map(|help| {
                            meta_line(theme, help.into()).text_color(rgb(theme.text_subtle))
                        })),
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
        .children(self.list_pick_view(dialog, row, typing, theme, cx))
        .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn settings_control(
        &self,
        dialog: &SettingsDialog,
        index: usize,
        setting: &'static Setting,
        focused: bool,
        typing: bool,
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
            Kind::Bool => toggle(
                SharedString::from(format!("switch-{}", setting.id())),
                shown == "true",
                !read_only,
                theme,
            )
            // Today's focus ring: a 1 px ring while the row has the keyboard.
            .when(focused, |el| {
                el.border(px(theme::BORDER_WIDTH)).border_color(rgb(ring))
            })
            .on_press(!read_only, Self::press_row(index, cx))
            .into_any_element(),
            Kind::Choice(choices) => segmented(theme)
                .when(focused, |el| el.border_color(rgb(ring)))
                .children(choices.iter().map(|&choice| {
                    let active = shown == choice;
                    // Read-only: each segment is faded, with no pointer and no hover.
                    segment_in(
                        SharedString::from(format!("choice-{}-{choice}", setting.id())),
                        active,
                        !read_only,
                        theme,
                    )
                    .text_size(font_size::META)
                    .on_press(
                        !active && !read_only,
                        cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.settings_do(|d, _| d.choose(setting, choice), window, cx);
                        }),
                    )
                    .child(choice)
                }))
                .into_any_element(),
            Kind::Text | Kind::Number => {
                let editing = dialog
                    .draft
                    .as_ref()
                    .filter(|(s, _)| s.id() == setting.id());
                let field = match editing {
                    Some((_, input)) => caret_field(
                        "settings-value",
                        input,
                        "",
                        typing,
                        theme,
                        cx,
                        |shell, caret| {
                            let draft = shell
                                .settings_dialog
                                .as_mut()
                                .and_then(|d| d.draft.as_mut());
                            if let Some((_, input)) = draft {
                                input.click(caret.at, caret.extend, caret.clicks);
                            }
                        },
                    ),
                    None => text_field("settings-value", &shown, "", theme),
                };
                field
                    .w_full()
                    .when(read_only, |el| el.opacity(opacity::DISABLED))
                    .into_any_element()
            }
            Kind::Font | Kind::List(_) => {
                let open = dialog
                    .list_pick
                    .as_ref()
                    .is_some_and(|pick| pick.setting.id() == setting.id());
                dropdown_trigger(
                    SharedString::from(format!("dropdown-{}", setting.id())),
                    if shown.is_empty() {
                        "Default".to_owned()
                    } else {
                        shown
                    },
                    open,
                    !read_only,
                    theme,
                )
                // Today's focus ring; an open pick keeps the accent frame.
                .when(focused && !open, |el| el.border_color(rgb(ring)))
                .on_press(!read_only, Self::press_row(index, cx))
                .into_any_element()
            }
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
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .pressable(Look::Icon, true, theme)
                        .tooltip(tooltip(
                            theme,
                            &format!("Back to {}", keymap::keys_label(&command.default_keys())),
                            "",
                        ))
                        .on_press(
                            true,
                            cx.listener(move |shell, _, window, cx| {
                                cx.stop_propagation();
                                shell.settings_do(|d, _| d.reset(command), window, cx);
                            }),
                        )
                        .focus_stop(theme)
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

    /// The pick under its row: the query, then the matching fonts (Default first) or names.
    fn list_pick_view(
        &self,
        dialog: &SettingsDialog,
        row: Row,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let pick = dialog.list_pick.as_ref()?;
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
                    "settings-list-query",
                    &pick.query,
                    if matches!(setting.kind, Kind::List(_)) {
                        "Search themes"
                    } else {
                        "Search fonts"
                    },
                    typing,
                    theme,
                    cx,
                    |shell, caret| {
                        let pick = shell
                            .settings_dialog
                            .as_mut()
                            .and_then(|d| d.list_pick.as_mut());
                        if let Some(pick) = pick {
                            pick.query.click(caret.at, caret.extend, caret.clicks);
                        }
                    },
                ))
                .child(
                    div()
                        .id("settings-list")
                        .max_h(px(theme::SETTINGS_FONT_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .children(dialog.list_choices().into_iter().enumerate().map(
                            |(i, font)| {
                                let label = if font.is_empty() {
                                    "Default".to_owned()
                                } else {
                                    font.clone()
                                };
                                menu_item(
                                    SharedString::from(format!("list-choice-{i}")),
                                    i == selected,
                                    theme,
                                )
                                .on_press(
                                    true,
                                    cx.listener(move |shell, _, window, cx| {
                                        cx.stop_propagation();
                                        let font = font.clone();
                                        shell.settings_do(
                                            move |d, _| d.choose(setting, &font),
                                            window,
                                            cx,
                                        );
                                    }),
                                )
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

/// A boxed click handler (`Pressable::on_press` takes any `Fn`).
type PressHandler = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
