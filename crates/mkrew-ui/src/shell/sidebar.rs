//! Sidebar (full and compact) and its footer.

use crate::*;

/// Payload of a drag of the sidebar's right edge (a resize, nothing is dropped).
pub(crate) struct SidebarGrip;

/// A workspace's groups of sidebar rows; a row is dragged to a new place only inside its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RowGroup {
    Agents,
    Terminal,
    Run,
}

impl RowGroup {
    fn label(self) -> &'static str {
        match self {
            RowGroup::Agents => "AGENTS",
            RowGroup::Terminal => "TERMINAL",
            RowGroup::Run => "RUN",
        }
    }
}

impl Shell {
    /// The sidebar's edge was released: saves the dragged width as `[ui] sidebar_width`.
    pub(crate) fn end_sidebar_drag(&mut self) {
        if std::mem::take(&mut self.sidebar_dragging) {
            self.save_width("sidebar_width", self.sidebar_width);
        }
    }

    /// The RUN group under an open workspace: a small `RUN` label, then one row per run
    /// command (green dot while running, hollow gray when stopped, red after an exit), its
    /// name and its ports in green monospace; then live runs whose command left the config. A
    /// running row opens its terminal like a session row (right-click: the pane menu; × on
    /// hover removes it); a stopped one shows ▶ on hover. Empty without run commands.
    pub(crate) fn render_run_group(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let summary = self.run_summary(project_id);
        let stray = self.sidebar.sessions().iter().filter_map(|session| {
            let name = mkrew_core::run::command_name(session)?;
            (session.project_id == project_id
                && session.state != SessionState::Exited
                && !summary.rows.iter().any(|row| row.name == name))
            .then(|| run::Row {
                name: name.to_owned(),
                state: RowState::Running(session.state),
                session_id: Some(session.id),
                ports: self.ports.get(session.id).to_vec(),
            })
        });
        let rows: Vec<run::Row> = summary.rows.iter().cloned().chain(stray).collect();
        if rows.is_empty() {
            return Vec::new();
        }
        let shown = self.focused_session();
        let hover = theme.hover;
        let collapsed = self.collapsed_groups.contains(&(project_id, RowGroup::Run));
        let label = self.group_label(project_id, RowGroup::Run, rows.len(), collapsed, theme, cx);
        if collapsed {
            return vec![label];
        }
        let configured = summary.rows.len();
        let lines = rows.into_iter().enumerate().map(|(index, row)| {
            let title = row.name.clone();
            let running = matches!(row.state, RowState::Running(_));
            let (color, hollow) = match row.state {
                RowState::Running(_) => (theme.status_done, false),
                RowState::Exited(_) => (theme.status_exited, false),
                RowState::Stopped => (theme.status_idle, true),
            };
            let id = SharedString::from(format!("run-row-{project_id}-{}", row.name));
            let live = row.session_id.filter(|_| running);
            let selected = live.is_some() && live == shown;
            let name = row.name.clone();
            let ports: Vec<String> = row.ports.iter().map(|p| format!(":{p}")).collect();
            let action = match live {
                Some(session_id) => row_action(
                    SharedString::from(format!("run-remove-{session_id}")),
                    icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),
                    theme,
                )
                .tooltip(tooltip(theme, "Remove from the sidebar", ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.remove_session(session_id, window, cx);
                })),
                None => row_action(
                    SharedString::from(format!("run-start-{project_id}-{name}")),
                    icon_svg(Icon::Play, theme::ICON_SIZE_SMALL, theme.text_muted),
                    theme,
                )
                .tooltip(tooltip(theme, "Run this command", ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    let name = Some(name.clone());
                    shell.run_action(project_id, RunAction::Start, name, window, cx);
                })),
            };
            let line = div()
                .id(id.clone())
                .group(id.clone())
                .min_h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .rounded(px(radius::SMALL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(selected, |el| {
                    el.bg(rgb(theme.selected))
                        .border_color(rgb(theme.selected_ring))
                })
                .tooltip(tooltip(
                    theme,
                    &format!("{} — {}", row.name, run::state_text(&row.state)),
                    "",
                ))
                .when_some(live, |el, session_id| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.open_session(session_id, window, cx);
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                                shell.open_session(session_id, window, cx);
                                shell.open_pane_menu(session_id, event.position, window, cx);
                            }),
                        )
                })
                .child(status_dot(id.clone(), color, hollow, false))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(font_size::BODY)
                        .text_color(rgb(if running {
                            theme.text
                        } else {
                            theme.text_muted
                        }))
                        .child(row.name),
                )
                .when(running && !ports.is_empty(), |el| {
                    el.child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .text_size(font_size::META)
                            .font_family(theme.terminal_font_family.clone())
                            .text_color(rgb(theme.status_done))
                            .child(ports.join(" ")),
                    )
                })
                .child(
                    action
                        .opacity(opacity::HIDDEN)
                        .group_hover(id, |style| style.opacity(opacity::FULL)),
                )
                .into_any_element();
            // Only configured commands have a place to move to (the config order).
            if index < configured {
                let item = DraggedItem {
                    list: ReorderList::Rows(project_id, RowGroup::Run),
                    index,
                    title: title.into(),
                };
                self.reorderable(line, item, false, 0.0, theme, cx)
            } else {
                line
            }
        });
        std::iter::once(label).chain(lines).collect()
    }

    /// The `from`-th row of `project_id`'s `group` was dropped at index `to`: run commands are
    /// saved in the new order (`project.update`), sessions get positions `1..=n`
    /// (`session.move`, shown at once).
    pub(crate) fn drop_row(
        &mut self,
        project_id: Ulid,
        group: RowGroup,
        from: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if group == RowGroup::Run {
            let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
                return;
            };
            let order: Vec<usize> = (0..project.run_commands.len()).collect();
            let commands = sidebar::reordered(&order, from, to)
                .into_iter()
                .map(|(i, _)| project.run_commands[i].clone())
                .collect();
            let params = workspace_menu::set_run_commands(project_id, commands);
            self.update_project(params, "Cannot reorder the run commands", window, cx);
            return;
        }
        let order = (self.sidebar).group_order(project_id, group == RowGroup::Terminal);
        for (session_id, position) in sidebar::reordered(&order, from, to) {
            let moved = (self.sidebar.sessions().iter())
                .any(|s| s.id == session_id && s.position != position);
            if !moved {
                continue;
            }
            self.sidebar.set_position(session_id, position);
            self.send_edit(
                Request::SessionMove(SessionMoveParams {
                    session_id,
                    position,
                }),
                "Cannot move the session",
                |_: &mut Self, _: serde_json::Value| {},
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// The workspaces with nothing under them, as one line at the bottom: `3 empty workspaces ▸`
    /// with their tiny avatars; a click lists them as normal rows (click, menu, right-click).
    pub(crate) fn render_empty_workspaces(
        &self,
        rows: Vec<WorkspaceRow>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let open = self.empty_workspaces_open;
        let hover = theme.hover;
        let count = rows.len();
        let line = div()
            .id("empty-workspaces")
            .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .on_click(cx.listener(|shell, _, _, cx| {
                shell.empty_workspaces_open = !shell.empty_workspaces_open;
                cx.notify();
            }))
            .child(icon_svg(
                if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                theme::ICON_SIZE_SMALL,
                theme.chevron_color(),
            ))
            .child(div().flex_1().min_w_0().truncate().child(if count == 1 {
                "1 empty workspace".to_owned()
            } else {
                format!("{count} empty workspaces")
            }))
            .when(!open, |el| {
                el.child(
                    div()
                        .flex_none()
                        .flex()
                        .gap(px(space::XS))
                        .children(rows.iter().map(|row| {
                            let color =
                                (row.color.as_deref()).and_then(|name| theme.workspace_color(name));
                            workspace_avatar(&row.name, color, theme)
                                .size(theme::rems_px(theme::WORKSPACE_AVATAR_TINY))
                        })),
                )
            });
        let listed = open.then(|| {
            rows.into_iter().map(|row| {
                let project_id = row.project_id;
                let row = workspace_row(row, false, theme, cx);
                self.with_workspace_menu(row, project_id, theme, cx)
            })
        });
        div()
            .flex()
            .flex_col()
            .child(line)
            .children(listed.into_iter().flatten())
            .into_any_element()
    }

    pub(crate) fn render_sidebar(
        &self,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let shown = self.focused_session();
        let connected = self.connection.is_some();
        let add_workspace_keys = self.keymap.label(Command::AddWorkspace);
        let view = self.sidebar.view(&self.viewed);
        let any_open = self.sidebar.any_expanded();
        let idle_agents = self.idle_agents().len();
        // Without workspaces: the title and `+` (the empty state below offers it too).
        let header = (connected && self.sidebar.is_empty()).then(|| {
            div()
                .flex_none()
                .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
                .flex()
                .items_center()
                .justify_between()
                .px(px(space::M))
                .child(
                    div()
                        .text_size(font_size::META)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text_muted))
                        .child("Workspaces"),
                )
                .child(
                    icon_button("add-workspace", Icon::Plus, theme)
                        .tooltip(tooltip(theme, "Add workspace", add_workspace_keys.clone()))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(window, cx);
                        })),
                )
        });
        let filter_focused = self.filter_focus.is_focused(window);
        let controls = (connected && !self.sidebar.is_empty()).then(|| {
            let filter = self.sidebar.filter();
            let hover = theme.hover;
            // Inside the field's right padding: × while it holds text, else the `/` hint.
            let trailing = if !filter.is_empty() {
                Some(
                    row_action(
                        "sidebar-filter-clear".into(),
                        icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),
                        theme,
                    )
                    .tooltip(tooltip(theme, "Clear filter", ""))
                    .on_click(cx.listener(|shell, _, _, cx| {
                        cx.stop_propagation();
                        shell.sidebar.filter_mut().clear();
                        cx.notify();
                    }))
                    .into_any_element(),
                )
            } else {
                (!filter_focused).then(|| kbd("/".to_owned(), theme, false).into_any_element())
            };
            let field = text_field(
                "sidebar-filter",
                filter.as_str(),
                "Filter sessions…",
                filter_focused,
                filter.all_selected(),
                theme,
            )
            .flex_1()
            .min_w_0()
            // The search mark sits in the padding before the text, the trailing slot after it.
            .relative()
            .pl(px(space::XL))
            .pr(px(space::XL))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(space::S))
                    .flex()
                    .items_center()
                    .child(icon_svg(
                        Icon::Search,
                        theme::ICON_SIZE_SMALL,
                        theme.text_muted,
                    )),
            )
            .children(trailing.map(|trailing| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(px(space::XS))
                    .flex()
                    .items_center()
                    .child(trailing)
            }))
            // A tracked element takes the keyboard on a click.
            .track_focus(&self.filter_focus)
            .on_key_down(cx.listener(Self::filter_key_down))
            .when(filter_focused, |el| {
                el.child(self.typing(&self.filter_focus, Self::filter_key_down, cx))
            });
            let actions = div()
                .flex_none()
                .flex()
                .items_center()
                .when(idle_agents > 0, |el| {
                    el.child(
                        icon_button("remove-idle-agents", Icon::Delete, theme)
                            .tooltip(tooltip(
                                theme,
                                &format!("Remove idle agents ({idle_agents})"),
                                "",
                            ))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.remove_idle_agents(window, cx);
                            })),
                    )
                })
                .child(
                    icon_button(
                        "collapse-workspaces",
                        if any_open {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        },
                        theme,
                    )
                    .tooltip(tooltip(
                        theme,
                        if any_open {
                            "Collapse all workspaces"
                        } else {
                            "Expand all workspaces"
                        },
                        "",
                    ))
                    .on_click(cx.listener(|shell, _, _, cx| {
                        shell.sidebar.toggle_all();
                        cx.notify();
                    })),
                )
                .child(
                    icon_button("add-workspace", Icon::Plus, theme)
                        .tooltip(tooltip(theme, "Add workspace", add_workspace_keys.clone()))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(window, cx);
                        })),
                );
            // All / Working / Needs you as a segmented control; a waiting count is amber.
            let segment =
                |id: &'static str, label: &'static str, count: usize, which: sidebar::Chip| {
                    let active = self.sidebar.chip() == which;
                    let count_color = if which == sidebar::Chip::NeedsYou && count > 0 {
                        theme.status_needs_input
                    } else {
                        theme.text_muted
                    };
                    div()
                        .id(id)
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(space::XS))
                        .py(px(theme::SEGMENT_PADDING))
                        .rounded(px(radius::SMALL))
                        .border(px(theme::BORDER_WIDTH))
                        .cursor_pointer()
                        .whitespace_nowrap()
                        .text_size(font_size::META)
                        .when(active, |el| {
                            el.bg(rgb(theme.surface_raised))
                                .border_color(rgb(theme.border))
                                .text_color(rgb(theme.text))
                        })
                        .when(!active, |el| {
                            el.border_color(gpui::transparent_black())
                                .text_color(rgb(theme.text_muted))
                                .hover(move |style| style.bg(rgb(hover)))
                        })
                        .child(label)
                        .child(div().text_color(rgb(count_color)).child(count.to_string()))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.sidebar.set_chip(which);
                            cx.notify();
                        }))
                };
            // The filter row lines up with the tab strip on its right: same height, same
            // bottom divider.
            div()
                .flex_none()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_none()
                        .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .px(px(space::M))
                        .border_b(px(theme::BORDER_WIDTH))
                        .border_color(rgb(theme.divider))
                        .child(field)
                        .child(actions),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .mx(px(space::M))
                        .my(px(space::S))
                        .p(px(theme::SEGMENT_PADDING))
                        .rounded(px(radius::MEDIUM))
                        .bg(rgb(theme.surface))
                        .child(segment(
                            "chip-all",
                            "All",
                            view.counts.all,
                            sidebar::Chip::All,
                        ))
                        .child(segment(
                            "chip-working",
                            "Working",
                            view.counts.working,
                            sidebar::Chip::Working,
                        ))
                        .child(segment(
                            "chip-needs-you",
                            "Needs you",
                            view.counts.needs_you,
                            sidebar::Chip::NeedsYou,
                        )),
                )
        });
        let errors = [&self.sidebar_error, &self.keymap_error].map(|error| {
            error.clone().map(|error| {
                div()
                    .px(px(space::M))
                    .pb(px(space::S))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("✕ {error}"))
            })
        });
        let body = if connected && self.sidebar.is_empty() {
            // Empty state: one sentence plus the primary action with its shortcut.
            empty_state(theme, None, None, "No workspaces yet.".into(), None)
                .child(
                    button(
                        "add-first-workspace",
                        icon_text(Icon::FolderPlus, "Add workspace", button_fg(theme, true)),
                        theme,
                        true,
                    )
                    .child(kbd(add_workspace_keys, theme, true))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.add_workspace(window, cx);
                    })),
                )
                .into_any_element()
        } else {
            // A workspace's AGENTS, TERMINAL and RUN groups hang under a faint guide line.
            let guide = |rows: Vec<gpui::AnyElement>| {
                div()
                    .ml(px(space::L))
                    .pl(px(space::XS))
                    .border_l(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.subtle))
                    .flex()
                    .flex_col()
                    .children(rows)
                    .into_any_element()
            };
            let filtering = self.sidebar.filtering();
            let mut list = Vec::new();
            let mut agents = Vec::new();
            let mut terminals = Vec::new();
            // The open workspace whose rows are being gathered: its RUN group closes them.
            let mut open: Option<Ulid> = None;
            let mut empty = Vec::new();
            // The AGENTS and TERMINAL labels only when the workspace shows another group too.
            let close = |list: &mut Vec<gpui::AnyElement>,
                         agents: &mut Vec<gpui::AnyElement>,
                         terminals: &mut Vec<gpui::AnyElement>,
                         open: Option<Ulid>,
                         cx: &mut Context<'_, Self>| {
                let shown_project = open.filter(|_| !filtering);
                let run = shown_project
                    .map(|project_id| self.render_run_group(project_id, theme, cx))
                    .unwrap_or_default();
                let labeled = [agents.len(), terminals.len(), run.len()]
                    .iter()
                    .filter(|&&n| n > 0)
                    .count()
                    > 1;
                let mut rows = Vec::new();
                let groups = [(RowGroup::Agents, agents), (RowGroup::Terminal, terminals)];
                for (group, rows_of) in groups {
                    // A label (and so folding) needs the workspace's own, unfiltered rows.
                    match shown_project.filter(|_| labeled && !rows_of.is_empty()) {
                        Some(project_id) => {
                            let key = (project_id, group);
                            let collapsed = self.collapsed_groups.contains(&key);
                            let count = rows_of.len();
                            rows.push(
                                self.group_label(project_id, group, count, collapsed, theme, cx),
                            );
                            if collapsed {
                                rows_of.clear();
                            } else {
                                rows.append(rows_of);
                            }
                        }
                        None => rows.append(rows_of),
                    }
                }
                rows.extend(run);
                if !rows.is_empty() {
                    list.push(guide(rows));
                }
            };
            for row in view.rows {
                match row {
                    Row::Workspace(row) => {
                        close(&mut list, &mut agents, &mut terminals, open.take(), cx);
                        let (project_id, active) =
                            (row.project_id, self.workspace == Some(row.project_id));
                        // Nothing to show under it: on the "N empty workspaces" line instead
                        // (the shown and pinned ones stay in place).
                        if row.empty && !active && !row.pinned && !filtering {
                            empty.push(row);
                            continue;
                        }
                        open = row.expanded.then_some(project_id);
                        let row = workspace_row(row, active, theme, cx);
                        list.push(self.with_workspace_menu(row, project_id, theme, cx));
                    }
                    Row::Session(row) => {
                        let (selected, session_id) =
                            (shown == Some(row.session_id), row.session_id);
                        let (group, rows_of) = if row.shell {
                            (RowGroup::Terminal, &mut terminals)
                        } else {
                            (RowGroup::Agents, &mut agents)
                        };
                        let item = DraggedItem {
                            list: ReorderList::Rows(row.project_id, group),
                            index: rows_of.len(),
                            title: row
                                .title
                                .clone()
                                .unwrap_or_else(|| row.agent.clone())
                                .into(),
                        };
                        // One lookup, for the selected row only.
                        let progress = selected.then(|| self.plans.fraction(session_id)).flatten();
                        let badge = self.agent_badge(&row.agent);
                        let row = session_row(row, badge, selected, progress, now, theme, cx);
                        let row = self.with_peek(row, session_id, now, theme, cx);
                        // Dragging reorders the whole group: off while a filter hides rows.
                        rows_of.push(if filtering {
                            row
                        } else {
                            self.reorderable(row, item, false, 0.0, theme, cx)
                        });
                    }
                    // Hide what's empty: an open workspace without sessions shows nothing.
                    Row::NoSessions { .. } => {}
                }
            }
            close(&mut list, &mut agents, &mut terminals, open, cx);
            if !empty.is_empty() {
                list.push(self.render_empty_workspaces(empty, theme, cx));
            }
            if list.is_empty() && filtering {
                // A query or chip that nothing matches.
                list.push(
                    div()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("No sessions match.")
                        .into_any_element(),
                );
            }
            div()
                .flex()
                .flex_col()
                .px(px(space::XS))
                .children(list)
                .into_any_element()
        };
        let divider = theme.divider;
        let grip = div()
            .id("sidebar-grip")
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(theme::GRIP_WIDTH))
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(move |style| style.bg(rgb(divider)))
            .on_drag(SidebarGrip, |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(
                cx.listener(|shell, event: &DragMoveEvent<SidebarGrip>, window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                    shell.sidebar_width = sidebar::width_from_pointer(
                        f32::from(window.viewport_size().width),
                        f32::from(event.event.position.x),
                    );
                    shell.sidebar_dragging = true;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.end_sidebar_drag()),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.end_sidebar_drag()),
            );
        // The header and the filter stay in view; the workspace list scrolls below them.
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .relative()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.sidebar))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .children(header)
            .children(controls)
            .child(
                div()
                    .id("sidebar")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .on_drop(cx.listener(|shell, item: &DraggedItem, window, cx| {
                        shell.drop_reordered(item, window, cx);
                    }))
                    .children(errors.into_iter().flatten())
                    .child(body),
            )
            .child(self.render_sidebar_footer(false, theme, cx))
            .child(grip)
    }

    /// The compact rail (Toggle compact sidebar): per workspace its avatar, per open workspace
    /// its sessions' status marks and agent chips; each row's tooltip spells out the rest. Same
    /// rows, clicks, right-click menus and keyboard peek as the full sidebar; no filter, chips
    /// or hover peek (the tooltip replaces it). Without workspaces only `+` shows.
    pub(crate) fn render_compact_sidebar(
        &self,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let shown = self.focused_session();
        let header = div()
            .flex_none()
            .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .when(self.connection.is_some(), |header| {
                header.child(
                    icon_button("add-workspace", Icon::Plus, theme)
                        .tooltip(tooltip(
                            theme,
                            "Add workspace",
                            self.keymap.label(Command::AddWorkspace),
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(window, cx);
                        })),
                )
            });
        // An error is too long for the rail: its mark, with the text on hover.
        let mut list = Vec::new();
        for (i, error) in [&self.sidebar_error, &self.keymap_error]
            .into_iter()
            .enumerate()
        {
            if let Some(error) = error {
                list.push(
                    div()
                        .id(SharedString::from(format!("sidebar-error-{i}")))
                        .flex()
                        .justify_center()
                        .py(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .tooltip(tooltip(theme, error, ""))
                        .child("✕")
                        .into_any_element(),
                );
            }
        }
        for row in self.sidebar.view(&self.viewed).rows {
            match row {
                Row::Workspace(row) => {
                    let (project_id, active) =
                        (row.project_id, self.workspace == Some(row.project_id));
                    let row = compact_workspace_row(row, active, theme, cx);
                    list.push(self.with_workspace_menu(row, project_id, theme, cx));
                }
                Row::Session(row) => {
                    let session_id = row.session_id;
                    let badge = self.agent_badge(&row.agent);
                    let row =
                        compact_session_row(row, badge, shown == Some(session_id), now, theme, cx);
                    list.push(self.with_peek(row, session_id, now, theme, cx));
                }
                // The workspace's tooltip already shows it has nothing running (no summary).
                Row::NoSessions { .. } => {}
            }
        }
        div()
            .flex_none()
            .w(theme::rems_px(theme::COMPACT_SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.sidebar))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(header)
            .child(
                div()
                    .id("sidebar")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .px(px(space::XS))
                    .overflow_y_scroll()
                    .children(list),
            )
            .child(self.render_sidebar_footer(true, theme, cx))
    }

    /// The sidebar's footer (was the status bar's left part): the Inbox state (`Inbox clear`,
    /// or amber `2 need you`, a click jumps to the next), the voice input state while there is
    /// one, and the compact toggle. The rail shows the same as icons, stacked. Read from state
    /// the shell keeps: it never asks the daemon and has no timer.
    pub(crate) fn render_sidebar_footer(
        &self,
        compact: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let hover = theme.hover;
        let waiting = self.inbox_entries().len();
        let inbox_color = if waiting > 0 {
            theme.status_needs_input
        } else {
            theme.text_muted
        };
        let inbox = div()
            .id("sidebar-inbox")
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_color(rgb(inbox_color))
            .child(icon_svg(Icon::Inbox, theme::ICON_SIZE_SMALL, inbox_color))
            .when(!compact, |el| {
                el.child(div().truncate().child(inbox::label(waiting)))
            })
            .when(compact, |el| {
                el.tooltip(tooltip(theme, &inbox::label(waiting), ""))
            })
            // Nothing to jump to while clear: no pointer, no hover, no click.
            .when(waiting > 0, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .when(!compact, |el| {
                        el.tooltip(tooltip(
                            theme,
                            "Jump to next item needing attention",
                            self.keymap.label(Command::NextInbox),
                        ))
                    })
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.jump_next_inbox(window, cx);
                    }))
            });
        // Full width ↔ compact rail (a hidden sidebar comes back as the rail).
        let compact_toggle = icon_button(
            "toggle-compact-sidebar",
            if compact {
                Icon::PanelLeftOpen
            } else {
                Icon::PanelLeftClose
            },
            theme,
        )
        .tooltip(tooltip(
            theme,
            if compact {
                "Expand sidebar"
            } else {
                "Compact sidebar"
            },
            self.keymap.label(Command::ToggleCompactSidebar),
        ))
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.toggle_compact_sidebar(window, cx);
        }));
        div()
            .flex_none()
            .flex()
            .when(compact, |el| {
                el.flex_col().gap(px(space::XS)).py(px(space::XS))
            })
            .when(!compact, |el| {
                el.min_h(theme::rems_px(theme::SIDEBAR_FOOTER_HEIGHT))
                    .gap(px(space::S))
                    .px(px(space::S))
            })
            .items_center()
            .overflow_hidden()
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .text_size(font_size::META)
            .child(inbox)
            .children(self.voice_status(compact, theme))
            .when(!compact, |el| el.child(div().flex_1()))
            .child(compact_toggle)
            // Behind a dialog the buttons do nothing, like the keys (`Shell::run`).
            .when(self.modal_open(), |el| el.relative().child(inert_layer()))
    }

    /// Voice input (§20G): the level meter and recording time, Transcribing, or the last
    /// outcome; `None` while idle. `meter_only`: just the meter / dots (the compact rail).
    pub(crate) fn voice_status(&self, meter_only: bool, theme: &Theme) -> Option<gpui::Div> {
        let row = || div().min_w_0().flex().items_center().gap(px(space::XS));
        match &self.voice {
            voice::State::Idle => None,
            voice::State::Recording { since, level, .. } => {
                let seconds = since.elapsed().as_secs();
                let color = theme.status_needs_input;
                Some(
                    row()
                        .child(voice_meter(level.meter(), rgb(color)))
                        .when(!meter_only, |el| {
                            el.child(notice(
                                color,
                                format!(
                                    "{}:{:02} · {} stops",
                                    seconds / 60,
                                    seconds % 60,
                                    self.keymap.label(Command::VoiceInput)
                                ),
                            ))
                        }),
                )
            }
            voice::State::Transcribing => Some(
                row()
                    .child(working_dots(status::now_ms(), rgb(theme.text_muted)))
                    .when(!meter_only, |el| {
                        el.child(notice(theme.text_muted, "Transcribing".to_owned()))
                    }),
            ),
            voice::State::Message(text) => {
                (!meter_only).then(|| row().child(notice(theme.text_muted, text.clone())))
            }
        }
    }

    /// A group label under a workspace (`⌄ AGENTS 3`, `› RUN 2` while folded): a fold chevron,
    /// small muted caps and the row count, with room above to set the groups apart. A click
    /// folds the group to this line.
    pub(crate) fn group_label(
        &self,
        project_id: Ulid,
        group: RowGroup,
        count: usize,
        collapsed: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let id = SharedString::from(format!("group-{project_id}-{group:?}"));
        let hover = theme.hover;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(space::XS))
            .mt(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .text_size(font_size::META)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text_muted))
            .tooltip(tooltip(
                theme,
                if collapsed {
                    "Show this group"
                } else {
                    "Fold this group"
                },
                "",
            ))
            .on_click(cx.listener(move |shell, _, _, cx| {
                let key = (project_id, group);
                if !shell.collapsed_groups.remove(&key) {
                    shell.collapsed_groups.insert(key);
                }
                cx.notify();
            }))
            .child(icon_svg(
                if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                },
                theme::ICON_SIZE_SMALL,
                theme.chevron_color(),
            ))
            .child(group.label())
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight(weight::REGULAR))
                    .text_color(rgb(theme.chevron_color()))
                    .child(count.to_string()),
            )
            .into_any_element()
    }
}

/// A workspace header row: chevron, avatar, bold name, pin; on the right its status: a
/// `● 1 working` pill (pulsing dot) while an agent works, else a muted `2 idle`, plus an amber
/// `? 1` while one needs you; the ⋯ menu on hover. Tooltip: the folder. A click on the chevron
/// only expands / collapses the row; elsewhere it shows the workspace.
pub(crate) fn workspace_row(
    row: WorkspaceRow,
    active: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let project_id = row.project_id;
    let hover = theme.hover;
    let group = SharedString::from(format!("workspace-{project_id}"));
    let color = row
        .color
        .as_deref()
        .and_then(|name| theme.workspace_color(name));
    let status = if row.working > 0 {
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .px(px(space::S))
                .rounded_full()
                .bg(rgb(theme.accent_bg))
                .text_size(font_size::META)
                .text_color(rgb(theme.accent_text))
                .child(status_dot(
                    SharedString::from(format!("workspace-working-{project_id}")),
                    theme.status_working,
                    false,
                    true,
                ))
                .child(format!("{} working", row.working)),
        )
    } else {
        (row.idle > 0).then(|| {
            div()
                .flex_none()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(format!("{} idle", row.idle))
        })
    };
    div()
        .id(group.clone())
        .group(group.clone())
        .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(theme.selected)))
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &row.path, ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.click_workspace_row(project_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, _, window, cx| {
                shell.open_workspace_menu(project_id, window, cx);
            }),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "workspace-chevron-{project_id}"
                )))
                .flex_none()
                .flex()
                .items_center()
                .on_click(cx.listener(move |shell, _, _, cx| {
                    // Only expands / collapses: not a click on the row (which shows it).
                    cx.stop_propagation();
                    shell.sidebar.toggle(project_id);
                    cx.notify();
                }))
                .child(icon_svg(
                    if row.expanded {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    theme::ICON_SIZE_SMALL,
                    theme.chevron_color(),
                )),
        )
        .child(workspace_avatar(&row.name, color, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(font_size::BODY)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .child(row.name),
        )
        .when(row.pinned, |el| el.child(pin_mark()))
        .children(status)
        .when(row.asking > 0, |el| {
            el.child(
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.status_needs_input))
                    .child(format!(
                        "{} {}",
                        sidebar::status_display(SessionState::NeedsInput).icon,
                        row.asking
                    )),
            )
        })
        .child(
            row_action(
                SharedString::from(format!("workspace-menu-{project_id}")),
                icon_svg(Icon::Ellipsis, theme::ICON_SIZE_SMALL, theme.text_muted),
                theme,
            )
            // Shown on hover (SPEC §14 progressive disclosure); right-click opens it too.
            .opacity(opacity::HIDDEN)
            .group_hover(group, |style| style.opacity(opacity::FULL))
            .tooltip(tooltip(
                theme,
                "Workspace menu (or right-click the row)",
                "",
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on the row (which would collapse it).
                cx.stop_propagation();
                shell.open_workspace_menu(project_id, window, cx);
            })),
        )
        .into_any_element()
}

/// A workspace's first letter on a tint of its color (SPEC §10, ADR 0020); muted without one.
/// The letter is `text` (see [`Theme::avatar_colors`]); the tint carries the color.
pub(crate) fn workspace_avatar(name: &str, color: Option<u32>, theme: &Theme) -> gpui::Div {
    let color = color.unwrap_or(theme.text_muted);
    let (letter, _) = theme.avatar_colors(color, theme.sidebar);
    div()
        .flex_none()
        .size(theme::rems_px(theme::WORKSPACE_AVATAR_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .bg(theme.tint(color))
        .text_color(rgb(letter))
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .child(
            name.chars()
                .next()
                .map(|c| c.to_uppercase().collect::<String>())
                .unwrap_or_default(),
        )
}

/// A session row: the agent glyph (in an accent chip while the agent works; its hover gives
/// the status in words), the title (ellipsized), the state glyph of a waiting or finished
/// agent (none while idle), the time right-aligned, and × on hover. The selected row is ringed.
pub(crate) fn session_row(
    row: SessionRow,
    badge: String,
    selected: bool,
    progress: Option<f32>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let session_id = row.session_id;
    let unseen = row.unseen;
    let mark = row.mark();
    let working = mark == Some(SessionState::Working);
    let time = row.time_text(now);
    // Agent rows show just the title (the agent chip already names the agent), selected or
    // not; terminal rows keep `powershell · title` (their chip does not say which shell).
    // `progress` is the plan bar of the selected row, under the name line.
    let name = match &row.title {
        Some(title) if row.shell => format!("{} · {title}", row.agent),
        Some(title) => title.clone(),
        None => row.agent.clone(),
    };
    let hover = theme.hover;
    let row_id = SharedString::from(format!("session-{session_id}"));
    div()
        .id(row_id.clone())
        .group(row_id.clone())
        .min_h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .when(progress.is_some(), |el| el.py(px(space::XS)))
        .rounded(px(radius::SMALL))
        // Same box selected or not, so the ring does not shift the row.
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::transparent_black())
        .when(selected, |el| {
            el.bg(rgb(theme.selected))
                .border_color(rgb(theme.selected_ring))
        })
        // A finish not yet viewed: tinted until the pane is focused.
        .when(unseen && !selected, |el| {
            el.bg(theme.tint(theme.status_done))
        })
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_session(session_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                // Show the pane first: the menu's Rename edits the pane header's title, which
                // exists only for the pane on screen (same as a left click).
                shell.open_session(session_id, window, cx);
                shell.open_pane_menu(session_id, event.position, window, cx);
            }),
        )
        .on_hover(cx.listener(move |shell, hovered: &bool, window, cx| {
            shell.hover_row(session_id, *hovered, window, cx);
        }))
        .child(
            // The glyph says what runs; its hover says the state in words and when it changed.
            div()
                .id(SharedString::from(format!("session-status-{session_id}")))
                .flex_none()
                .tooltip(status_tooltip(
                    theme,
                    row.state,
                    row.state_source,
                    row.state_changed_at,
                ))
                .child(agent_chip(badge, theme).when(working, |chip| {
                    chip.bg(rgb(theme.accent_bg))
                        .border_color(rgb(theme.status_working))
                        .text_color(rgb(theme.accent_text))
                })),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .when(selected || unseen, |el| {
                            el.font_weight(FontWeight(if unseen {
                                weight::SEMIBOLD
                            } else {
                                weight::MEDIUM
                            }))
                        })
                        .child(name),
                )
                .children(
                    progress
                        .map(|fraction| progress_bar(fraction, theme::PLAN_ROW_BAR_HEIGHT, theme)),
                ),
        )
        .when(row.pinned, |el| el.child(pin_mark()))
        .children(mark.filter(|_| !working).map(|state| {
            div()
                .flex_none()
                .text_size(font_size::META)
                .text_color(rgb(sidebar::status_color(theme, state)))
                .child(status::icon(state))
        }))
        .children(time.map(|time| {
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(time)
        }))
        .child(
            row_action(
                SharedString::from(format!("session-remove-{session_id}")),
                icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),
                theme,
            )
            // Shown on hover (SPEC §14 progressive disclosure).
            .opacity(opacity::HIDDEN)
            .group_hover(row_id.clone(), |style| style.opacity(opacity::FULL))
            .tooltip(tooltip(theme, "Remove from the sidebar", ""))
            .on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on the row (which would open the session).
                cx.stop_propagation();
                shell.remove_session(session_id, window, cx);
            })),
        )
        .into_any_element()
}

/// A workspace in the compact rail: its avatar, then `?` while a session needs input, else its
/// working count. Clicks as [`workspace_row`]; the tooltip has the name, summary and folder.
pub(crate) fn compact_workspace_row(
    row: WorkspaceRow,
    active: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let project_id = row.project_id;
    let hover = theme.hover;
    let color = row
        .color
        .as_deref()
        .and_then(|name| theme.workspace_color(name));
    let mark = if row.needs_input {
        let icon = sidebar::status_display(SessionState::NeedsInput).icon;
        Some((icon.to_owned(), theme.status_needs_input))
    } else {
        (row.working > 0).then(|| (row.working.to_string(), theme.text_muted))
    };
    div()
        .id(SharedString::from(format!("workspace-{project_id}")))
        .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(theme.selected)))
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &sidebar::workspace_tip(&row), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.click_workspace_row(project_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, _, window, cx| {
                shell.open_workspace_menu(project_id, window, cx);
            }),
        )
        .child(workspace_avatar(&row.name, color, theme))
        // A fixed-width slot, empty without a mark, so every avatar sits in one column.
        .child(
            div()
                .flex_none()
                .w(theme::rems_px(theme::STATUS_MARK_WIDTH))
                .flex()
                .justify_center()
                .children(mark.map(|(text, color)| {
                    div()
                        .text_size(font_size::META)
                        .font_weight(FontWeight(weight::SEMIBOLD))
                        .text_color(rgb(color))
                        .child(text)
                })),
        )
        .into_any_element()
}

/// A session in the compact rail: its status mark and agent chip. Clicks as [`session_row`];
/// the tooltip has `agent · full title` and the status in words. No hover peek (the tooltip
/// replaces it); the keyboard peek still hangs on this row.
pub(crate) fn compact_session_row(
    row: SessionRow,
    badge: String,
    selected: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let session_id = row.session_id;
    let hover = theme.hover;
    div()
        .id(SharedString::from(format!("session-{session_id}")))
        .h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .when(row.unseen && !selected, |el| {
            el.bg(theme.tint(theme.status_done))
        })
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &sidebar::session_tip(&row, now), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_session(session_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                // As in `session_row`: show the pane first (Rename edits its header).
                shell.open_session(session_id, window, cx);
                shell.open_pane_menu(session_id, event.position, window, cx);
            }),
        )
        .child(mark_slot(row.mark(), now, theme))
        .child(agent_chip(badge, theme))
        .into_any_element()
}

/// A sidebar row's state mark in a narrow fixed slot (empty without a mark), so agent chips
/// stay in one column without pushing the rows in: one dot that blinks on the status tick
/// while working, else the state glyph.
fn mark_slot(mark: Option<SessionState>, now: i64, theme: &Theme) -> gpui::AnyElement {
    let slot = div()
        .flex_none()
        .w(theme::rems_px(theme::SIDEBAR_MARK_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    let Some(state) = mark else {
        return slot.into_any_element();
    };
    let color = rgb(sidebar::status_color(theme, state));
    if state != SessionState::Working {
        return slot
            .text_size(font_size::META)
            .text_color(color)
            .child(status::icon(state))
            .into_any_element();
    }
    slot.child(
        div()
            .size(theme::rems_px(theme::STATUS_DOT))
            .rounded_full()
            .bg(color)
            .opacity(if status::working_dot(now) == 0 {
                opacity::STATUS_DOT_DIM
            } else {
                opacity::FULL
            }),
    )
    .into_any_element()
}
