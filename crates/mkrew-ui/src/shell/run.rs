//! Run button, run menu and run setup.

use crate::*;

impl Shell {
    /// The Run state of `project_id` (empty when the workspace is unknown).
    pub(crate) fn run_summary(&self, project_id: Ulid) -> Summary {
        self.sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| run::summary(project, self.sidebar.sessions(), &self.ports))
            .unwrap_or_default()
    }

    /// The button's main part: Run setup when nothing is configured, the run tab while
    /// something runs, else run everything.
    pub(crate) fn run_main_click(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let summary = self.run_summary(project_id);
        if summary.rows.is_empty() {
            self.open_run_setup(project_id, true, window, cx);
        } else if summary.any_live() {
            self.show_run_tab(project_id, window, cx);
        } else {
            self.run_action(project_id, RunAction::Start, None, window, cx);
        }
    }

    /// `project.run_start` / `run_restart` / `run_stop` (`name` = one command, `None` = all).
    /// Running from all-stopped shows the run tab; restarts keep the view (SPEC §10). A
    /// failure shows in the sidebar notice.
    pub(crate) fn run_action(
        &mut self,
        project_id: Ulid,
        action: RunAction,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.run_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if action == RunAction::Stop {
            let stopped =
                connection.call::<()>(Request::ProjectRunStop(RunStopParams { project_id, name }));
            cx.spawn_in(window, async move |this, cx| {
                let stopped = stopped.await;
                let _ = this.update(cx, |shell, cx| {
                    shell.sidebar_error = stopped.err().map(|error| {
                        tracing::warn!(%project_id, "project.run_stop failed: {error:#}");
                        format!("Cannot stop: {error:#}")
                    });
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        let reveal = action == RunAction::Start
            && name.is_none()
            && !self.run_summary(project_id).any_live();
        let params = RunStartParams {
            project_id,
            name,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        };
        let request = match action {
            RunAction::Restart => Request::ProjectRunRestart(params),
            RunAction::Start | RunAction::Stop => Request::ProjectRunStart(params),
        };
        let started = connection.call::<RunStarted>(request);
        cx.spawn_in(window, async move |this, cx| {
            let started = started.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match started {
                    Ok(run) => {
                        shell.sidebar_error = None;
                        if reveal {
                            shell.views.insert(project_id, View::Terminals);
                            shell.run_reveal = run.tab_id;
                            shell.reveal_run_tab(project_id, window, cx);
                            shell.activate_workspace(project_id, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "run failed: {error:#}");
                        shell.sidebar_error = Some(format!("Cannot run: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Switches to the workspace's run tab.
    pub(crate) fn show_run_tab(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.run_menu = None;
        self.run_reveal = self.tabs.get(&project_id).and_then(|tabs| {
            tabs.tabs()
                .iter()
                .find(|tab| tab.title == mkrew_core::run::TAB_TITLE)
                .map(|tab| tab.id)
        });
        self.views.insert(project_id, View::Terminals);
        self.reveal_run_tab(project_id, window, cx);
        self.activate_workspace(project_id, window, cx);
    }

    /// Activates the tab in `run_reveal` once `project_id`'s tabs hold it.
    pub(crate) fn reveal_run_tab(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tab_id) = self.run_reveal else {
            return;
        };
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.activate(tab_id) {
            self.run_reveal = None;
            self.sync_panes(window, cx);
            cx.notify();
        }
    }

    pub(crate) fn open_run_setup(
        &mut self,
        project_id: Ulid,
        run_after: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.run_menu = None;
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        self.run_setup = Some(RunSetup::new(project, run_after));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_run_setup(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.run_setup = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn run_setup_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(setup) = self.run_setup.as_mut() else {
            return;
        };
        if matches!(key, DialogKey::Copy | DialogKey::Cut)
            && setup.picker.is_none()
            && let Some(text) = setup.selected_text()
        {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
        }
        if let Some(text) = paste
            && setup.picker.is_none()
        {
            setup.insert(&text);
        }
        match setup.key(key) {
            run_setup::Outcome::Cancel => return self.close_run_setup(window, cx),
            run_setup::Outcome::Save => return self.save_run_setup(window, cx),
            run_setup::Outcome::Edited => {}
        }
        cx.notify();
    }

    /// `fs.list` of folder `dir` for the Run setup's picker (ignored files too: `.env` usually
    /// is).
    pub(crate) fn list_run_picker(
        &mut self,
        dir: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.notify();
        let (Some(dir), Some(connection), Some(setup)) =
            (dir, self.connection.clone(), self.run_setup.as_ref())
        else {
            return;
        };
        let listed = connection.call::<Vec<FsEntry>>(Request::FsList(FsListParams {
            project_id: setup.project_id,
            worktree_task_id: None,
            dir: dir.clone(),
            show_ignored: true,
        }));
        let serial = self.dialog_serial;
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if shell.dialog_serial != serial {
                    return;
                }
                if let Some(setup) = shell.run_setup.as_mut() {
                    setup.listed(&dir, listed);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// `project.update` with the dialog's commands; on success it closes and, when opened from
    /// ▶ Run, runs everything.
    pub(crate) fn save_run_setup(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(setup) = self.run_setup.as_mut() else {
            return;
        };
        let run_after = setup.run_after;
        let Some(params) = setup.begin() else {
            cx.notify();
            return;
        };
        let project_id = params.project_id;
        let updated = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let updated = updated.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if shell.dialog_serial != serial || shell.run_setup.is_none() {
                    return;
                }
                match updated {
                    Ok(project) => {
                        // Seen before `project.updated` arrives, so the run below finds the commands.
                        shell.sidebar.apply(&Event::ProjectUpdated(project.clone()));
                        shell.close_run_setup(window, cx);
                        if run_after && !project.run_commands.is_empty() {
                            shell.run_action(project_id, RunAction::Start, None, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "project.update failed: {error:#}");
                        if let Some(setup) = shell.run_setup.as_mut() {
                            setup.failed(format!("{error:#}"));
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// The Run button (SPEC §10): a split accent button, `▶ Run` (or the running count and
    /// ports, with ↻ and ■ beside it while live) and `▾` for the per-command menu below it.
    pub(crate) fn render_run_button(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let summary = self.run_summary(project_id);
        let live = summary.any_live();
        let tip = if summary.rows.is_empty() {
            "Set up the commands to run".to_owned()
        } else {
            summary.tooltip()
        };
        // Idle: the accent (the logo's blue) fill; live: quiet, ↻ ■ carry the colors.
        let main = button("run-main", summary.label(), theme, !live)
            .flex_none()
            .when(!live, |el| el.rounded_r_none())
            .when(summary.warning(), |el| {
                el.text_color(rgb(theme.status_needs_input))
            })
            .tooltip(tooltip(theme, &tip, ""))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.run_main_click(project_id, window, cx);
            }));
        let open = self.run_menu == Some(project_id) && !self.modal_open();
        let mut bar = div()
            .flex_none()
            .flex()
            .items_center()
            .when(live, |el| el.gap(px(space::XS)))
            .child(main);
        if live {
            for (id, label, tip, action) in [
                (
                    "run-restart",
                    "↻",
                    "Restart all run commands",
                    RunAction::Restart,
                ),
                ("run-stop", "■", "Stop all run commands", RunAction::Stop),
            ] {
                bar = bar.child(
                    run_icon_button(id, label, action, theme)
                        .flex_none()
                        .tooltip(tooltip(theme, tip, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.run_action(project_id, action, None, window, cx);
                        })),
                );
            }
        }
        // The ▾ half of the split button: same fill, a faint divider on its left.
        let menu = button("run-menu", "▾", theme, !live).when(!live, |el| {
            el.rounded_l_none()
                .border_l(px(theme::BORDER_WIDTH))
                .border_color(gpui::rgba(theme.on_accent_divider))
        });
        bar.child(
            div()
                .child(
                    menu.tooltip(tooltip(theme, "Run commands", ""))
                        // An open menu already closed on this mouse down (outside it): the
                        // click reopens it.
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.run_menu = Some(project_id);
                            cx.notify();
                        })),
                )
                .when(open, |el| {
                    el.child(self.render_run_menu(project_id, &summary, theme, cx))
                }),
        )
        .into_any_element()
    }

    /// One row per command (state, ports, ↻ ■ or ▶), then "Show Run tab" and "Run setup…".
    pub(crate) fn render_run_menu(
        &self,
        project_id: Ulid,
        summary: &Summary,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        for row in &summary.rows {
            let running = matches!(row.state, RowState::Running(_));
            let color = match row.state {
                RowState::Running(_) => theme.status_done,
                RowState::Exited(_) => theme.status_exited,
                RowState::Stopped => theme.text_muted,
            };
            let ports: Vec<String> = row.ports.iter().map(|p| format!(":{p}")).collect();
            let controls: &[(&'static str, &'static str, RunAction)] = if running {
                &[
                    ("↻", "Restart this command", RunAction::Restart),
                    ("■", "Stop this command", RunAction::Stop),
                ]
            } else {
                &[("▶", "Run this command", RunAction::Start)]
            };
            let mut line = div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(font_size::BODY)
                .child(
                    div()
                        .flex_1()
                        .text_color(rgb(theme.text))
                        .child(row.name.clone()),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(color))
                        .child(run::state_text(&row.state)),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(ports.join(" ")),
                );
            for &(label, tip, action) in controls {
                let name = row.name.clone();
                let colors = run::action_colors(action, theme);
                line = line.child(
                    div()
                        .id(SharedString::from(format!("run-{label}-{}", row.name)))
                        .px(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .text_color(rgb(colors.fg))
                        .hover(move |style| style.bg(rgb(colors.tint)))
                        .tooltip(tooltip(theme, tip, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            // Not a click on what lies under the menu (a card, a pane).
                            cx.stop_propagation();
                            shell.run_action(project_id, action, Some(name.clone()), window, cx);
                        }))
                        .child(label),
                );
            }
            rows.push(line.into_any_element());
        }
        let panel = menu_panel("run-menu-panel", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.run_menu = None;
                cx.notify();
            }))
            .when(!rows.is_empty(), |el| el.children(rows).child(separator()))
            .child(
                menu_item("run-show-tab".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Show Run tab"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.show_run_tab(project_id, window, cx);
                    })),
            )
            .child(
                menu_item("run-setup".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Run setup…"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.open_run_setup(project_id, false, window, cx);
                    })),
            );
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }

    /// "Run setup": one card per command, "+ Add command", "Start with the daemon", errors,
    /// Cancel and Save (Ctrl+Enter).
    pub(crate) fn render_run_setup(
        &self,
        setup: &RunSetup,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        use run_setup::{Field, Focus};
        if let Some(picker) = &setup.picker {
            return self.render_run_picker(picker, theme, cx);
        }
        let label = |text: &'static str| {
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let field = |cx: &mut Context<'_, Self>,
                     i: usize,
                     which: Field,
                     id: &'static str,
                     placeholder: &'static str| {
            let input = setup.cards[i].input(which);
            caret_field(
                id,
                input.as_str(),
                input.cursor(),
                input.selection(),
                placeholder,
                setup.focus == Focus::Field(i, which),
                theme,
            )
            .on_click(cx.listener(move |shell, _, _, cx| {
                if let Some(setup) = shell.run_setup.as_mut() {
                    setup.focus = Focus::Field(i, which);
                }
                cx.notify();
            }))
        };
        // A field with "Browse…" beside it, which picks its value from the workspace.
        let browse = |cx: &mut Context<'_, Self>,
                      field: gpui::Stateful<gpui::Div>,
                      i: usize,
                      which: Field,
                      id: &'static str,
                      hint: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .child(div().flex_1().min_w_0().child(field))
                .child(
                    quiet_button(id, "Browse…", theme, theme.surface_raised)
                        .tooltip(tooltip(theme, hint, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            let dir = shell
                                .run_setup
                                .as_mut()
                                .and_then(|setup| setup.open_picker(i, which));
                            shell.list_run_picker(dir, window, cx);
                        })),
                )
        };
        let mut cards = div()
            .id("run-cards")
            .flex()
            .flex_col()
            .gap(px(space::S))
            .max_h(px(theme::RUN_SETUP_MAX_HEIGHT))
            .overflow_y_scroll();
        for i in 0..setup.cards.len() {
            let name = field(cx, i, Field::Name, "run-name", "e.g. BE");
            let dir = field(cx, i, Field::Dir, "run-dir", ". or EA-BE");
            let dir = browse(
                cx,
                dir,
                i,
                Field::Dir,
                "run-dir-browse",
                "Choose a folder in the workspace",
            );
            let command = field(cx, i, Field::Command, "run-command", "e.g. npm run dev");
            let env_file = field(cx, i, Field::EnvFile, "run-env-file", "e.g. EA-BE/.env");
            let env_file = browse(
                cx,
                env_file,
                i,
                Field::EnvFile,
                "run-env-file-browse",
                "Choose a file in the workspace",
            );
            let env = field(cx, i, Field::Env, "run-env", "e.g. ENVIRONMENT=dev");
            cards = cards.child(
                // The id scopes the card's field and button ids (the same in every card).
                div()
                    .id(("run-card", i))
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .p(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .flex()
                            .gap(px(space::S))
                            .child(div().flex_1().child(label("Name")).child(name))
                            .child(div().flex_1().child(label("Folder")).child(dir)),
                    )
                    .child(label("Command"))
                    .child(command)
                    .child(label("Env file (optional, relative to the workspace)"))
                    .child(env_file)
                    .child(label("Env (KEY=VALUE per line, wins over the env file)"))
                    .child(env)
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(space::XS))
                            .child(
                                quiet_button(
                                    "run-duplicate",
                                    "Duplicate",
                                    theme,
                                    theme.surface_raised,
                                )
                                .on_click(cx.listener(
                                    move |shell, _, _, cx| {
                                        if let Some(setup) = shell.run_setup.as_mut() {
                                            setup.duplicate_card(i);
                                        }
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                quiet_button("run-remove", "Remove", theme, theme.surface_raised)
                                    .on_click(cx.listener(move |shell, _, _, cx| {
                                        if let Some(setup) = shell.run_setup.as_mut() {
                                            setup.remove_card(i);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let hover = theme.surface_raised;
        let autostart = div()
            .id("run-autostart")
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::XS))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(gpui::transparent_black())
            .when(setup.focus == Focus::Autostart, |el| {
                el.border_color(rgb(theme.pane_focus_border))
            })
            .cursor_pointer()
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
            .tooltip(tooltip(
                theme,
                "Start the run commands when the daemon starts",
                "Space",
            ))
            .on_click(cx.listener(|shell, _, _, cx| {
                if let Some(setup) = shell.run_setup.as_mut() {
                    setup.autostart = !setup.autostart;
                }
                cx.notify();
            }))
            .child(
                div()
                    .size(px(space::M))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.accent))
                    .child(if setup.autostart { "✓" } else { "" }),
            )
            .child(div().flex_1().child("Start with the daemon"))
            .child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("Space"),
            );
        let panel = div()
            .id("run-setup")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::run_setup_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::run_setup_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Run setup"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(
                        "Each command runs in its own pane of the “run” tab, in a shell started \
                         in its folder.",
                    ),
            )
            .child(cards)
            .child(
                div()
                    .flex()
                    .child(
                        button("run-add", "+ Add command", theme, false).on_click(cx.listener(
                            |shell, _, _, cx| {
                                if let Some(setup) = shell.run_setup.as_mut() {
                                    setup.add_card();
                                }
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(autostart)
            .children(setup.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("run-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Keep the current commands", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_run_setup(window, cx);
                            })),
                    )
                    .child(if setup.busy {
                        disabled_button("run-save", "Saving…", theme, true)
                    } else {
                        button("run-save", "Save", theme, true)
                            .tooltip(tooltip(theme, "Save the run commands", "Ctrl+Enter"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.save_run_setup(window, cx);
                            }))
                    }),
            );
        Self::modal_shade("run-setup-shade", panel, cx)
    }

    /// The Run setup's folder / env file picker: the listed folder's entries (a folder opens,
    /// a file is picked), ↑ Up, Cancel (Esc) and, for a folder, "Use this folder" (Enter).
    pub(crate) fn render_run_picker(
        &self,
        picker: &run_setup::Picker,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let folder = picker.field == run_setup::Field::Dir;
        let note = |text: String| {
            div()
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let hover = theme.surface_raised;
        let mut list = div()
            .id("run-picker-list")
            .flex()
            .flex_col()
            .max_h(px(theme::RUN_SETUP_MAX_HEIGHT))
            .overflow_y_scroll();
        list = match &picker.entries {
            None => list.child(note("Loading…".into())),
            Some(Err(error)) => list.child(note(format!("✕ Cannot list the folder: {error}"))),
            Some(Ok(entries)) if entries.is_empty() => list.child(note(
                if folder {
                    "No folders here."
                } else {
                    "Nothing here."
                }
                .into(),
            )),
            Some(Ok(entries)) => list.children(entries.iter().enumerate().map(|(index, entry)| {
                let (path, is_dir) = (entry.path.clone(), entry.is_dir);
                div()
                    .id(("run-picker-entry", index))
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .cursor_pointer()
                    .text_size(font_size::BODY)
                    .hover(move |style| style.bg(rgb(hover)))
                    .active(|style| style.opacity(opacity::ACTIVE))
                    .child(
                        div()
                            .flex_none()
                            .w(px(space::M))
                            .text_color(rgb(theme.text_muted))
                            .child(if is_dir { "▸" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(if entry.ignored {
                                theme.text_muted
                            } else {
                                theme.text
                            }))
                            .child(entry.name.clone()),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        let Some(setup) = shell.run_setup.as_mut() else {
                            return;
                        };
                        if is_dir {
                            let dir = setup.browse(path.clone());
                            shell.list_run_picker(dir, window, cx);
                        } else {
                            setup.pick(Some(&path));
                            cx.notify();
                        }
                    }))
            })),
        };
        let up = if picker.at_root() {
            disabled_button("run-picker-up", "↑ Up", theme, false)
        } else {
            quiet_button("run-picker-up", "↑ Up", theme, theme.surface_raised)
                .tooltip(tooltip(theme, "The folder above", ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    let dir = shell.run_setup.as_mut().and_then(RunSetup::browse_up);
                    shell.list_run_picker(dir, window, cx);
                }))
        };
        let location = if picker.dir.is_empty() {
            "Workspace root".to_owned()
        } else {
            picker.dir.clone()
        };
        let panel = div()
            .id("run-picker")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::run_setup_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(if folder {
                        "Choose the folder"
                    } else {
                        "Choose the env file"
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(font_size::BODY)
                            .text_color(rgb(theme.text_muted))
                            .child(location),
                    )
                    .child(up),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("run-picker-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Back to Run setup", "Esc"))
                            .on_click(cx.listener(|shell, _, _, cx| {
                                if let Some(setup) = shell.run_setup.as_mut() {
                                    setup.picker = None;
                                }
                                cx.notify();
                            })),
                    )
                    .when(folder, |row| {
                        row.child(
                            button("run-picker-use", "Use this folder", theme, true)
                                .tooltip(tooltip(theme, "Run the command in this folder", "Enter"))
                                .on_click(cx.listener(|shell, _, _, cx| {
                                    if let Some(setup) = shell.run_setup.as_mut() {
                                        setup.pick(None);
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            );
        Self::modal_shade("run-setup-shade", panel, cx)
    }
}
