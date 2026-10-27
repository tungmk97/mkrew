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

    /// `project.run_start` / `run_restart` / `run_stop` (`name` = one command, `None` = all).
    /// Running from all-stopped shows the run tab; restarts keep the view (SPEC §10). A
    /// failure shows in the sidebar notice. An open Run menu stays open and shows the new
    /// states; a click outside it closes it.
    pub(crate) fn run_action(
        &mut self,
        project_id: Ulid,
        action: RunAction,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if action == RunAction::Stop {
            let stopped =
                connection.call::<()>(Request::ProjectRunStop(RunStopParams { project_id, name }));
            cx.spawn_in(window, async move |this, cx| {
                let stopped = stopped.await;
                let _ = this.update(cx, |shell, cx| {
                    if let Err(error) = stopped {
                        tracing::warn!(%project_id, "project.run_stop failed: {error:#}");
                        shell.toast_error(format!("Cannot stop: {error:#}"), cx);
                    }
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
                        if reveal {
                            shell.views.insert(project_id, View::Terminals);
                            shell.run_reveal = run.tab_id;
                            shell.reveal_run_tab(project_id, window, cx);
                            shell.activate_workspace(project_id, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "run failed: {error:#}");
                        shell.toast_error(format!("Cannot run: {error:#}"), cx);
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

    /// Scrolls the Run setup's focused card into view (a new card lands below the fold).
    pub(crate) fn reveal_run_card(&self) {
        if let Some(i) = self.run_setup.as_ref().and_then(RunSetup::focused_card) {
            self.run_setup_scroll.scroll_to_item(i);
        }
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
        let outcome = setup.key(key);
        let card = setup.focused_card();
        match outcome {
            run_setup::Outcome::Cancel => return self.close_run_setup(window, cx),
            run_setup::Outcome::Save => return self.save_run_setup(window, cx),
            run_setup::Outcome::Edited => {}
        }
        if let Some(i) = card {
            self.run_setup_scroll.scroll_to_item(i);
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
                    // Closed while saving: a failure has no dialog left to show in.
                    if let Err(error) = &updated {
                        shell.toast_error(format!("Cannot save the run commands: {error:#}"), cx);
                    }
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

    /// The Run pill (SPEC §10): while something runs, a green pill `● 2 running :8082 :5180 ▾`
    /// (amber dot and `1/2 running` when one is down); else a muted `Run ▾`. A click opens the
    /// Run menu below it, which holds every Run action. The hover lists each command's state.
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
        let hover = theme.hover;
        let fg = if live {
            theme.run_pill_text
        } else {
            theme.text_dim
        };
        let dot = if summary.warning() {
            theme.status_needs_input
        } else {
            theme.status_done
        };
        let pill = div()
            .id("run-pill")
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::M))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .cursor_pointer()
            .whitespace_nowrap()
            .text_size(font_size::BODY)
            .text_color(rgb(fg))
            .when(live, |el| {
                el.bg(rgb(theme.run_pill_bg))
                    .border_color(rgb(theme.run_pill_border))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .hover(|style| style.opacity(opacity::HOVER))
            })
            .when(!live, |el| {
                el.border_color(rgb(theme.control_border))
                    .hover(move |style| style.bg(rgb(hover)))
            })
            .active(|style| style.opacity(opacity::ACTIVE))
            .tooltip(tooltip(theme, &tip, ""))
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_click(cx.listener(move |shell, _, _, cx| {
                shell.run_menu = Some(project_id);
                cx.notify();
            }))
            // ponytail: a steady dot, not a pulse: a pulse re-renders the window for as long as
            // a dev server runs (hours), against the idle CPU budget (SPEC §15).
            .when(live, |el| {
                el.child(status_dot("run-pill-dot", dot, false, false))
            })
            .when(!live, |el| {
                el.child(icon_svg(
                    Icon::Play,
                    theme::ICON_SIZE_SMALL,
                    theme.status_done,
                ))
            })
            .child(summary.label())
            .children(summary.ports_text().map(|ports| {
                div()
                    .font_family(theme.terminal_font_family.clone())
                    .child(ports)
            }))
            .child(icon_svg(Icon::ChevronDown, theme::ICON_SIZE_SMALL, fg));
        let open = self.run_menu == Some(project_id) && !self.modal_open();
        div()
            .child(pill)
            .when(open, |el| {
                el.child(self.render_run_menu(project_id, &summary, theme, cx))
            })
            .into_any_element()
    }

    /// `RUNNING IN <WORKSPACE>`: one row per command (state dot, name, ports, then ↻ and a red
    /// ■ while it runs, ▶ otherwise); then Restart all / Stop all while anything runs, Start all
    /// while nothing does, Show Run tab, and Run setup… (which runs the commands after saving
    /// when none were set up yet).
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
        let live = summary.any_live();
        let workspace = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map_or(String::new(), |project| project.name.to_uppercase());
        let heading = div()
            .px(px(space::S))
            .py(px(space::XS))
            .text_size(font_size::META)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text_muted))
            .child(if live {
                format!("RUNNING IN {workspace}")
            } else {
                format!("RUN IN {workspace}")
            });
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        for row in &summary.rows {
            let running = matches!(row.state, RowState::Running(_));
            let (color, hollow) = match row.state {
                RowState::Running(_) => (theme.status_done, false),
                RowState::Exited => (theme.status_exited, false),
                RowState::Stopped => (theme.status_idle, true),
            };
            let controls: &[(Icon, &'static str, RunAction)] = if running {
                &[
                    (Icon::RotateCw, "Restart this command", RunAction::Restart),
                    (Icon::CircleStop, "Stop this command", RunAction::Stop),
                ]
            } else {
                &[(Icon::Play, "Run this command", RunAction::Start)]
            };
            let ports: Vec<String> = row.ports.iter().map(|p| format!(":{p}")).collect();
            let mut line = div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(font_size::BODY)
                .child(status_dot(
                    SharedString::from(format!("run-dot-{}", row.name)),
                    color,
                    hollow,
                    false,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text))
                        .child(row.name.clone()),
                )
                // Running: its ports; otherwise its state in words (never color alone).
                .child(
                    div()
                        .flex_none()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .when(running, |el| {
                            el.font_family(theme.terminal_font_family.clone())
                                .child(ports.join(" "))
                        })
                        .when(!running, |el| {
                            el.child(
                                run::state_text(&row.state)
                                    .trim_start_matches(['○', '✕', ' '])
                                    .to_owned(),
                            )
                        }),
                );
            for &(glyph, tip, action) in controls {
                let name = row.name.clone();
                let colors = run::action_colors(action, theme);
                line = line.child(
                    div()
                        .id(SharedString::from(format!("run-{action:?}-{}", row.name)))
                        .flex_none()
                        .flex()
                        .items_center()
                        .p(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(colors.tint)))
                        .tooltip(tooltip(theme, tip, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            // Not a click on what lies under the menu (a card, a pane).
                            cx.stop_propagation();
                            shell.run_action(project_id, action, Some(name.clone()), window, cx);
                        }))
                        .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, colors.fg)),
                );
            }
            rows.push(line.into_any_element());
        }
        let item = |id: &'static str, glyph: Icon, label: &'static str, color: u32| {
            menu_item(id.into(), theme)
                .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, color))
                .child(div().flex_1().text_color(rgb(color)).child(label))
        };
        let keys = |command: Command| kbd(self.keymap.label(command), theme, false);
        let mut actions: Vec<gpui::AnyElement> = Vec::new();
        if live {
            actions.push(
                item("run-restart", Icon::RotateCw, "Restart all", theme.text)
                    .child(keys(Command::RunRestart))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.run_action(project_id, RunAction::Restart, None, window, cx);
                    }))
                    .into_any_element(),
            );
            actions.push(
                item(
                    "run-stop",
                    Icon::CircleStop,
                    "Stop all",
                    theme.status_exited,
                )
                .child(keys(Command::RunStop))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.run_action(project_id, RunAction::Stop, None, window, cx);
                }))
                .into_any_element(),
            );
        } else if !summary.rows.is_empty() {
            actions.push(
                item("run-start", Icon::Play, "Start all", theme.text)
                    .child(keys(Command::RunStart))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.run_action(project_id, RunAction::Start, None, window, cx);
                    }))
                    .into_any_element(),
            );
        }
        // Nothing set up yet: Save in the setup also runs them (the old Run button's click).
        let run_after = summary.rows.is_empty();
        let panel = menu_panel("run-menu-panel", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.run_menu = None;
                cx.notify();
            }))
            .child(heading)
            .when(!rows.is_empty(), |el| el.children(rows).child(separator()))
            .when(!actions.is_empty(), |el| {
                el.children(actions).child(separator())
            })
            .child(
                item(
                    "run-show-tab",
                    Icon::SquareTerminal,
                    "Show Run tab",
                    theme.text,
                )
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.show_run_tab(project_id, window, cx);
                })),
            )
            .child(
                item("run-setup", Icon::Settings, "Run setup…", theme.text).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.run_menu = None;
                        shell.open_run_setup(project_id, run_after, window, cx);
                    },
                )),
            );
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }

    /// "Run setup": a header naming the workspace, one card per command (number, name,
    /// Duplicate / Remove; Name + Folder, Command, Env file, Env), "+ Add command" under the
    /// cards, errors, then a footer with "Start with the daemon", Cancel and Save (Ctrl+Enter).
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
        // A field's caption, with a muted hint after it.
        let label = |text: &'static str, hint: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(
                    div()
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text_dim))
                        .child(text),
                )
                .when(!hint.is_empty(), |el| {
                    el.child(div().text_color(rgb(theme.text_muted)).child(hint))
                })
        };
        let group_field = |caption: gpui::Div, field: gpui::AnyElement| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .child(caption)
                .child(field)
        };
        let mono = theme.terminal_font_family.clone();
        let field = |cx: &mut Context<'_, Self>,
                     i: usize,
                     which: Field,
                     id: &'static str,
                     placeholder: &'static str| {
            caret_field(
                id,
                setup.cards[i].input(which),
                placeholder,
                setup.focus == Focus::Field(i, which),
                theme,
                cx,
                move |shell, caret| {
                    if let Some(setup) = shell.run_setup.as_mut() {
                        setup.click(i, which, caret.at, caret.extend, caret.clicks);
                    }
                },
            )
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
        // A checkbox row ("Start with the daemon", a card's "Close the pane when it ends");
        // Space flips the focused one.
        let check = |id: &'static str,
                     label: &'static str,
                     checked: bool,
                     focused: bool,
                     tip: &'static str| {
            let hover = theme.surface_raised;
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::XS))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(focused, |el| el.border_color(rgb(theme.pane_focus_border)))
                .cursor_pointer()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .hover(move |style| style.bg(rgb(hover)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .tooltip(tooltip(theme, tip, "Space"))
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
                        .child(if checked { "✓" } else { "" }),
                )
                .child(label)
                .when(focused, |el| {
                    el.child(
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child("Space"),
                    )
                })
        };
        let mut cards = div()
            .id("run-cards")
            .flex()
            .flex_col()
            .gap(px(space::S))
            // Room for the scroll bar over the right edge.
            .pr(px(theme::SCROLLBAR_WIDTH))
            .max_h(px(theme::RUN_SETUP_MAX_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.run_setup_scroll);
        // Only the card holding the keyboard shows its fields; the others are one line each.
        let open = setup.focused_card();
        for i in 0..setup.cards.len() {
            let card = &setup.cards[i];
            let named = card.name.as_str().trim();
            let title = div()
                .flex_none()
                .max_w(px(theme::RUN_NAME_MAX_WIDTH))
                .truncate()
                .text_size(font_size::BODY)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(if named.is_empty() {
                    theme.text_muted
                } else {
                    theme.text
                }))
                .child(if named.is_empty() {
                    "New command".to_owned()
                } else {
                    named.to_owned()
                });
            let number = div()
                .flex_none()
                .px(px(space::S))
                .rounded_full()
                .bg(rgb(theme.accent_bg))
                .text_color(rgb(theme.accent_text))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .child((i + 1).to_string());
            let group = SharedString::from(format!("run-card-{i}"));
            let card_action = |id: &'static str, glyph: Icon, tip: &'static str| {
                quiet_button(
                    id,
                    icon_svg(glyph, theme::ICON_SIZE_SMALL, theme.text_muted),
                    theme,
                    theme.surface_raised,
                )
                .tooltip(tooltip(theme, tip, ""))
            };
            let actions = div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .when(open != Some(i), |el| {
                    el.opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                })
                .child(
                    card_action("run-duplicate", Icon::Copy, "Duplicate this command").on_click(
                        cx.listener(move |shell, _, _, cx| {
                            // Not a click on the folded card (which would open it).
                            cx.stop_propagation();
                            if let Some(setup) = shell.run_setup.as_mut() {
                                setup.duplicate_card(i);
                                shell.reveal_run_card();
                            }
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    card_action("run-remove", Icon::Trash2, "Remove this command").on_click(
                        cx.listener(move |shell, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(setup) = shell.run_setup.as_mut() {
                                setup.remove_card(i);
                            }
                            cx.notify();
                        }),
                    ),
                );
            // The id scopes the card's field and button ids (the same in every card).
            let frame = div()
                .id(("run-card", i))
                .group(group.clone())
                .flex_none()
                .flex()
                .flex_col()
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border));
            if open != Some(i) {
                // Folded: number, name, the command and its folder; a click opens it.
                let command = card.command.as_str().trim();
                let dir = card.dir.as_str().trim();
                let hover = theme.hover;
                cards = cards.child(
                    frame
                        .px(px(space::M))
                        .py(px(space::S))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            if let Some(setup) = shell.run_setup.as_mut() {
                                setup.focus = Focus::Field(i, Field::Command);
                                shell.reveal_run_card();
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(space::S))
                                .child(number)
                                .child(title)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(mono.clone())
                                        .text_size(font_size::META)
                                        .text_color(rgb(theme.text_muted))
                                        .child(if command.is_empty() {
                                            "no command yet".to_owned()
                                        } else {
                                            command.to_owned()
                                        }),
                                )
                                .when(!dir.is_empty() && dir != ".", |el| {
                                    el.child(
                                        div()
                                            .flex_none()
                                            .max_w(px(theme::RUN_NAME_MAX_WIDTH))
                                            .truncate()
                                            .text_size(font_size::META)
                                            .text_color(rgb(theme.text_muted))
                                            .child(dir.to_owned()),
                                    )
                                })
                                .child(actions),
                        ),
                );
                continue;
            }
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
            let command = field(cx, i, Field::Command, "run-command", "e.g. npm run dev")
                .font_family(mono.clone());
            let env_file = field(cx, i, Field::EnvFile, "run-env-file", "e.g. EA-BE/.env");
            let env_file = browse(
                cx,
                env_file,
                i,
                Field::EnvFile,
                "run-env-file-browse",
                "Choose a file in the workspace",
            );
            let env = field(cx, i, Field::Env, "run-env", "e.g. ENVIRONMENT=dev")
                .font_family(mono.clone());
            let exit = check(
                "run-exit",
                "Close the pane when it ends",
                card.exit_after,
                setup.focus == Focus::Exit(i),
                "Off: the shell stays open after the command, showing its output",
            )
            .on_click(cx.listener(move |shell, _, _, cx| {
                if let Some(setup) = shell.run_setup.as_mut()
                    && let Some(card) = setup.cards.get_mut(i)
                {
                    card.exit_after = !card.exit_after;
                    setup.focus = Focus::Exit(i);
                }
                cx.notify();
            }));
            cards = cards.child(
                frame
                    .gap(px(space::M))
                    .p(px(space::M))
                    .bg(rgb(theme.card))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(number)
                            .child(title)
                            .child(div().flex_1())
                            .child(actions),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(space::S))
                            .child(group_field(label("Name", ""), name.into_any_element()))
                            .child(group_field(label("Folder", ""), dir.into_any_element())),
                    )
                    .child(group_field(
                        label("Command", ""),
                        command.into_any_element(),
                    ))
                    .child(group_field(
                        label("Env file", "optional, relative to the workspace"),
                        env_file.into_any_element(),
                    ))
                    .child(group_field(
                        label("Env", "KEY=VALUE per line, wins over the env file"),
                        env.into_any_element(),
                    ))
                    .child(exit),
            );
        }
        let add_hover = theme.hover;
        cards = cards.child(
            div()
                .id("run-add")
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .py(px(space::S))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border))
                .cursor_pointer()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .hover(move |style| style.bg(rgb(add_hover)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .child(icon_text(Icon::Plus, "Add command", theme.text_muted))
                .on_click(cx.listener(|shell, _, _, cx| {
                    if let Some(setup) = shell.run_setup.as_mut() {
                        setup.add_card();
                        shell.reveal_run_card();
                    }
                    cx.notify();
                })),
        );
        let workspace = (self.sidebar.projects().iter())
            .find(|project| project.id == setup.project_id)
            .map(|project| project.name.clone());
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let autostart = check(
            "run-autostart",
            "Start with the daemon",
            setup.autostart,
            setup.focus == Focus::Autostart,
            "Start the run commands when the daemon starts",
        )
        .on_click(cx.listener(|shell, _, _, cx| {
            if let Some(setup) = shell.run_setup.as_mut() {
                setup.autostart = !setup.autostart;
            }
            cx.notify();
        }));
        let panel = div()
            .id("run-setup")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::L))
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
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(icon_svg(Icon::Play, theme::ICON_SIZE, theme.accent))
                            .child(
                                div()
                                    .text_size(font_size::CARD_TITLE)
                                    .font_weight(FontWeight(weight::SEMIBOLD))
                                    .text_color(rgb(theme.text))
                                    .child("Run setup"),
                            )
                            .children(workspace.map(|name| {
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(font_size::BODY)
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("· {name}"))
                            })),
                    )
                    .child(
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(
                                "Each command runs in its own pane of the “run” tab, in a shell \
                                 started in its folder.",
                            ),
                    ),
            )
            .child(div().relative().child(cards).child(scrollbar::bar(
                scrollbar::Source::Handle(self.run_setup_scroll.clone()),
                Rc::clone(&self.run_setup_drag),
                theme,
            )))
            .children(setup.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pt(px(space::M))
                    .border_t(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.divider))
                    .child(autostart)
                    .child(div().flex_1())
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
                            .w(theme::rems_px(theme::ICON_SIZE_SMALL))
                            .when(is_dir, |el| {
                                el.child(icon_svg(
                                    Icon::ChevronRight,
                                    theme::ICON_SIZE_SMALL,
                                    theme.text_muted,
                                ))
                            }),
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
            disabled_button(
                "run-picker-up",
                icon_text(Icon::ArrowUp, "Up", theme.text_muted),
                theme,
                false,
            )
        } else {
            quiet_button(
                "run-picker-up",
                icon_text(Icon::ArrowUp, "Up", theme.text_muted),
                theme,
                theme.surface_raised,
            )
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
