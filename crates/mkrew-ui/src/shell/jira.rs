//! The Board's Jira side (SPEC §20I, ADR 0062): the `Tasks | Jira` switch, the workspace's
//! Jira keys field, the issue list, and opening an issue as a task draft.

use crate::*;

impl Shell {
    /// The `[jira]` section, read per call so a hand edit applies at once (like voice).
    fn jira_settings(&self) -> Result<mkrew_core::settings::JiraSettings, String> {
        mkrew_core::settings::load(&self.settings_file)
            .map(|settings| settings.jira)
            .map_err(|error| error.to_string())
    }

    /// The settings and the API token, or why a call cannot be made.
    fn jira_setup(&self) -> Result<(mkrew_core::settings::JiraSettings, String), String> {
        let settings = self.jira_settings()?;
        mkrew_ui::jira::api_token(&settings, |name| std::env::var(name).ok())
            .map(|token| (settings, token))
            .map_err(|error| error.to_string())
    }

    /// The workspace's saved Jira keys (empty until it is linked).
    fn jira_keys_of(&self, project_id: Ulid) -> &[String] {
        self.sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or(&[][..], |p| p.jira_keys.as_slice())
    }

    pub(crate) fn set_board_source(
        &mut self,
        project_id: Ulid,
        source: Source,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let view = self.jira.entry(project_id).or_default();
        view.source = source;
        if source == Source::Jira && view.load == Load::NotLoaded {
            self.load_jira(project_id, window, cx);
        }
        cx.notify();
    }

    /// Searches the workspace's open issues; without keys the keys field shows instead.
    pub(crate) fn load_jira(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let keys = self.jira_keys_of(project_id).to_vec();
        if keys.is_empty() {
            let view = self.jira.entry(project_id).or_default();
            view.keys_field.get_or_insert_with(Input::default);
            window.focus(&self.jira_keys_focus, cx);
            cx.notify();
            return;
        }
        self.search_jira(project_id, keys, window, cx);
    }

    /// Sends the search for `keys` on the background executor.
    fn search_jira(
        &mut self,
        project_id: Ulid,
        keys: Vec<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let setup = self.jira_setup();
        let view = self.jira.entry(project_id).or_default();
        let serial = view.start_load();
        let (settings, token) = match setup {
            Ok(setup) => setup,
            Err(reason) => {
                view.finish_load(serial, Err(reason));
                cx.notify();
                return;
            }
        };
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { mkrew_ui::jira::search(&settings, &token, &keys) })
                .await
                .map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Some(view) = shell.jira.get_mut(&project_id)
                    && view.finish_load(serial, result)
                {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A click on an issue row: the linked task's edit dialog, else a new task from the ticket.
    pub(crate) fn open_issue(
        &mut self,
        project_id: Ulid,
        key: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(task) =
            mkrew_ui::jira_board::linked_task(self.board.tasks(), project_id, &key).cloned()
        {
            self.jira.entry(project_id).or_default().source = Source::Tasks;
            self.open_task_dialog(TaskDialog::edit(task), window, cx);
            return;
        }
        let setup = self.jira_setup();
        let view = self.jira.entry(project_id).or_default();
        view.row_error = None;
        let (settings, token) = match setup {
            Ok(setup) => setup,
            Err(reason) => {
                view.row_error = Some((key, reason));
                cx.notify();
                return;
            }
        };
        view.opening = Some(key.clone());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let fetched_key = key.clone();
            let draft = cx
                .background_executor()
                .spawn(async move { mkrew_ui::jira::issue_draft(&settings, &token, &fetched_key) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                // A slow reply never replaces a dialog opened meanwhile, nor opens in another
                // workspace: the draft is dropped (the human clicks the row again).
                let free = !shell.modal_open() && shell.workspace == Some(project_id);
                let Some(view) = shell.jira.get_mut(&project_id) else {
                    return;
                };
                if view.opening.as_deref() != Some(key.as_str()) {
                    return; // another row was clicked since
                }
                view.opening = None;
                match draft {
                    Ok(draft) if free => shell.open_task_dialog(
                        TaskDialog::from_draft(project_id, draft),
                        window,
                        cx,
                    ),
                    Ok(_) => {}
                    Err(error) => view.row_error = Some((key, format!("{error:#}"))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Saves the keys field (`project.update`); errors show under the field.
    pub(crate) fn save_jira_keys(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.jira.entry(project_id).or_default();
        // Without the field `typed_keys` is empty and would clear the saved keys.
        if view.keys_field.is_none() || view.saving_keys {
            return;
        }
        let params = workspace_menu::set_jira_keys(project_id, view.typed_keys());
        view.saving_keys = true;
        view.keys_error = None;
        let saved = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let saved = saved.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(view) = shell.jira.get_mut(&project_id) else {
                    return;
                };
                view.saving_keys = false;
                match saved {
                    Ok(project) => {
                        view.load = Load::NotLoaded;
                        // The sidebar may not have `project.updated` yet: search the saved keys.
                        if !project.jira_keys.is_empty() {
                            view.keys_field = None;
                            shell.search_jira(project_id, project.jira_keys, window, cx);
                        }
                    }
                    Err(error) => view.keys_error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The keys field: typing edits, Enter saves, Esc clears (then leaves the field when the
    /// workspace already has keys).
    pub(crate) fn jira_keys_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(project_id), Some(key)) = (self.workspace, quick_open::key(&event.keystroke))
        else {
            return;
        };
        let has_keys = !self.jira_keys_of(project_id).is_empty();
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let view = self.jira.entry(project_id).or_default();
        // The field only has the keyboard while shown: saved keys absent or Edit.
        let field = view.keys_field.get_or_insert_with(Input::default);
        match key {
            quick_open::Key::Close if !field.is_empty() => field.clear(),
            quick_open::Key::Close => {
                if has_keys {
                    view.keys_field = None;
                    self.focus_pane(window, cx);
                }
            }
            quick_open::Key::Open => return self.save_jira_keys(project_id, window, cx),
            quick_open::Key::Move(_) => {}
            quick_open::Key::Backspace => {
                field.backspace();
            }
            quick_open::Key::DeleteWord => {
                field.delete_word();
            }
            quick_open::Key::SelectAll => field.select_all(),
            quick_open::Key::Paste => {
                field.insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                field.insert(&text);
            }
        }
        cx.notify();
    }

    /// The `Tasks | Jira` switch (only when `[jira] site` is set); in Jira mode with keys, the
    /// keys, Edit and Refresh.
    pub(crate) fn render_board_header(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        if !self.jira_on {
            return None;
        }
        let source = self
            .jira
            .get(&project_id)
            .map_or(Source::Tasks, |v| v.source);
        let keys = self.jira_keys_of(project_id);
        let toggle = self.mode_toggle(
            "board-source",
            source,
            [(Source::Tasks, "Tasks"), (Source::Jira, "Jira")],
            "Tasks / Jira",
            SharedString::default(),
            move |shell, source, window, cx| shell.set_board_source(project_id, source, window, cx),
            theme,
            cx,
        );
        let header = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .pt(px(space::S))
            .child(toggle);
        if source != Source::Jira || keys.is_empty() {
            return Some(header);
        }
        Some(
            header
                .child(notice(theme.text_muted, keys.join(", ")))
                .child(
                    button("jira-edit", "Edit", theme, false).on_click(cx.listener(
                        move |shell, _, window, cx| {
                            let keys = shell.jira_keys_of(project_id).to_vec();
                            shell.jira.entry(project_id).or_default().edit_keys(&keys);
                            window.focus(&shell.jira_keys_focus, cx);
                            cx.notify();
                        },
                    )),
                )
                .child(
                    button(
                        "jira-refresh",
                        icon_text(Icon::RotateCw, "Refresh", button_fg(theme, false)),
                        theme,
                        false,
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.load_jira(project_id, window, cx);
                    })),
                ),
        )
    }

    /// The Jira side of the Board: the keys field, the loading / failed / empty states, or the
    /// issue rows. Render never starts a load.
    pub(crate) fn render_jira(
        &self,
        project_id: Ulid,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let keys = self.jira_keys_of(project_id);
        let view = self.jira.get(&project_id);
        let field = view.and_then(|v| v.keys_field.as_ref());
        if field.is_some() || keys.is_empty() {
            let saving = view.is_some_and(|v| v.saving_keys);
            let error = view.and_then(|v| v.keys_error.clone());
            return div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .px(px(space::L))
                .pt(px(space::M))
                .child(
                    div()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text_muted))
                        .child("Link this workspace to Jira projects"),
                )
                .child(
                    text_field(
                        "jira-keys",
                        field.map_or("", Input::as_str),
                        "PROJ, BILL",
                        self.jira_keys_focus.is_focused(window),
                        field.is_some_and(Input::all_selected),
                        theme,
                    )
                    .track_focus(&self.jira_keys_focus)
                    .on_key_down(cx.listener(Self::jira_keys_key_down))
                    .relative()
                    .child(self.typing(&self.jira_keys_focus, Self::jira_keys_key_down, cx))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        window.focus(&shell.jira_keys_focus, cx);
                        cx.notify();
                    })),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(
                            button("jira-save", "Save", theme, true).on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    shell.save_jira_keys(project_id, window, cx);
                                },
                            )),
                        )
                        .when(saving, |el| {
                            el.child(working_dots(now, rgb(theme.text_muted)))
                        }),
                )
                .children(error.map(|error| notice(theme.text, format!("✕ {error}"))))
                .into_any_element();
        }
        match view.map_or(&Load::NotLoaded, |v| &v.load) {
            Load::Loading => message(theme, None, "Loading Jira issues…".into(), None)
                .flex_1()
                .child(working_dots(now, rgb(theme.text_muted)))
                .into_any_element(),
            Load::Failed(reason) => message(
                theme,
                None,
                "Cannot load Jira issues.".into(),
                Some(reason.clone().into()),
            )
            .flex_1()
            .child(button("jira-retry", "Retry", theme, false).on_click(
                cx.listener(move |shell, _, window, cx| shell.load_jira(project_id, window, cx)),
            ))
            .into_any_element(),
            Load::Loaded(issues) if issues.is_empty() => message(
                theme,
                None,
                format!("No open issues assigned to you in {}", keys.join(", ")).into(),
                None,
            )
            .flex_1()
            .into_any_element(),
            Load::Loaded(issues) => {
                let opening = view.and_then(|v| v.opening.as_deref());
                let row_error = view.and_then(|v| v.row_error.as_ref());
                div()
                    .id("jira-issues")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(space::L))
                    .pb(px(space::L))
                    .children(issues.iter().enumerate().map(|(index, issue)| {
                        let linked = mkrew_ui::jira_board::linked_task(
                            self.board.tasks(),
                            project_id,
                            &issue.key,
                        );
                        let key = issue.key.clone();
                        div()
                            .child(
                                div()
                                    .id(("jira-issue", index))
                                    .flex()
                                    .items_center()
                                    .gap(px(space::S))
                                    .px(px(space::S))
                                    .py(px(space::XS))
                                    .rounded(px(radius::SMALL))
                                    .cursor_pointer()
                                    .text_color(rgb(theme.text))
                                    .hover(|style| style.bg(rgb(theme.hover)))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.open_issue(project_id, key.clone(), window, cx);
                                    }))
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(font_size::META)
                                            .text_color(rgb(theme.text_muted))
                                            .child(issue.kind.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .font_weight(FontWeight(weight::MEDIUM))
                                            .child(issue.key.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .child(issue.summary.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(font_size::META)
                                            .text_color(rgb(theme.text_muted))
                                            .child(issue.status.clone()),
                                    )
                                    .children(linked.map(|task| {
                                        div()
                                            .flex_none()
                                            .px(px(space::S))
                                            .rounded(px(radius::SMALL))
                                            .bg(rgb(theme.surface_raised))
                                            .text_size(font_size::META)
                                            .child(board::column_title(task.status))
                                    }))
                                    .when(opening == Some(issue.key.as_str()), |el| {
                                        el.child(working_dots(now, rgb(theme.text_muted)))
                                    }),
                            )
                            .children(row_error.filter(|(failed, _)| *failed == issue.key).map(
                                |(_, reason)| {
                                    notice(theme.text, format!("✕ {reason}")).px(px(space::S))
                                },
                            ))
                    }))
                    .into_any_element()
            }
            Load::NotLoaded => message(
                theme,
                None,
                "Press Refresh to load Jira issues.".into(),
                None,
            )
            .flex_1()
            .into_any_element(),
        }
    }
}
