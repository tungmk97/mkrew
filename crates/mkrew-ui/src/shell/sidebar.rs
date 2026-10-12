//! Sidebar (full and compact) and the status bar.

use crate::*;

/// Payload of a drag of the sidebar's right edge (a resize, nothing is dropped).
pub(crate) struct SidebarGrip;

impl Shell {
    /// The sidebar's edge was released: saves the dragged width as `[ui] sidebar_width`.
    pub(crate) fn end_sidebar_drag(&mut self) {
        if std::mem::take(&mut self.sidebar_dragging) {
            self.save_width("sidebar_width", self.sidebar_width);
        }
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
        let header = div()
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
            );
        let header = header.when(connected && self.sidebar.is_empty(), |header| {
            header.child(
                icon_button("add-workspace", Icon::Plus, theme)
                    .tooltip(tooltip(theme, "Add workspace", add_workspace_keys.clone()))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.add_workspace(window, cx);
                    })),
            )
        });
        let idle_agents = self.idle_agents().len();
        let header = header.when(connected && !self.sidebar.is_empty(), |header| {
            header.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
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
                    ),
            )
        });
        let filter_focused = self.filter_focus.is_focused(window);
        let controls = (connected && !self.sidebar.is_empty()).then(|| {
            let filter = self.sidebar.filter();
            let hover = theme.hover;
            let chip =
                |id: &'static str, label: &'static str, count: usize, which: sidebar::Chip| {
                    let active = self.sidebar.chip() == which;
                    div()
                        .id(id)
                        .flex_none()
                        .px(px(space::S))
                        .rounded_full()
                        .cursor_pointer()
                        .text_size(font_size::META)
                        .when(active, |el| {
                            el.bg(rgb(theme.accent_bg))
                                .text_color(rgb(theme.accent_text))
                        })
                        .when(!active, |el| {
                            el.text_color(rgb(theme.text_muted))
                                .hover(move |style| style.bg(rgb(hover)))
                        })
                        .child(format!("{label} {count}"))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.sidebar.set_chip(which);
                            cx.notify();
                        }))
                };
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .px(px(space::M))
                .pb(px(space::S))
                .child(
                    // A tracked element takes the keyboard on a click.
                    text_field(
                        "sidebar-filter",
                        filter.as_str(),
                        "Filter sessions…",
                        filter_focused,
                        filter.all_selected(),
                        theme,
                    )
                    // The search mark sits in the padding before the text.
                    .relative()
                    .pl(px(space::XL))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(space::S))
                            .flex()
                            .items_center()
                            .text_color(rgb(theme.text_muted))
                            .child(icon_svg(
                                Icon::Search,
                                theme::ICON_SIZE_SMALL,
                                theme.text_muted,
                            )),
                    )
                    .track_focus(&self.filter_focus)
                    .on_key_down(cx.listener(Self::filter_key_down))
                    .when(filter_focused, |el| {
                        el.child(self.typing(&self.filter_focus, Self::filter_key_down, cx))
                    }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .child(chip("chip-all", "All", view.counts.all, sidebar::Chip::All))
                        .child(chip(
                            "chip-working",
                            "Working",
                            view.counts.working,
                            sidebar::Chip::Working,
                        ))
                        .child(chip(
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
            // A workspace's sessions hang under a faint guide line.
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
            let mut list = Vec::new();
            let mut sessions = Vec::new();
            for row in view.rows {
                match row {
                    Row::Workspace(row) => {
                        if !sessions.is_empty() {
                            list.push(guide(std::mem::take(&mut sessions)));
                        }
                        let (project_id, active) =
                            (row.project_id, self.workspace == Some(row.project_id));
                        let summary = self.run_summary(project_id);
                        let run_tip = summary.any_live().then(|| summary.tooltip());
                        let row = workspace_row(row, active, run_tip, theme, cx);
                        list.push(self.with_workspace_menu(row, project_id, theme, cx));
                    }
                    Row::Session(row) => {
                        let (selected, session_id) =
                            (shown == Some(row.session_id), row.session_id);
                        // One lookup, for the selected row only.
                        let progress = selected.then(|| self.plans.fraction(session_id)).flatten();
                        let badge = self.agent_badge(&row.agent);
                        let row = session_row(row, badge, selected, progress, now, theme, cx);
                        sessions.push(self.with_peek(row, session_id, now, theme, cx));
                    }
                    Row::NoSessions { .. } => sessions.push(
                        div()
                            .px(px(space::S))
                            .py(px(space::XS))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child("No sessions yet.")
                            .into_any_element(),
                    ),
                }
            }
            if !sessions.is_empty() {
                list.push(guide(sessions));
            }
            if list.is_empty() && self.sidebar.filtering() {
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
            .child(header)
            .children(controls)
            .child(
                div()
                    .id("sidebar")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(errors.into_iter().flatten())
                    .child(body),
            )
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
    }

    /// The status bar (SPEC §10): sidebar toggles (hide, compact), Inbox state, git HEAD; then the focused agent
    /// pane's usage. Everything is read from state the shell already keeps: it never asks the
    /// daemon and has no timer.
    /// The last child is the right-panel toggle (`inspector_toggle`).
    pub(crate) fn render_status_bar(&self, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::Div {
        let hover = theme.hover;
        // Behind a dialog the buttons do nothing, like the keys (`Shell::run`).
        let modal = self.modal_open();
        let toggle = div()
            .id("toggle-sidebar")
            .flex_none()
            .flex()
            .items_center()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .tooltip(tooltip(
                theme,
                if self.sidebar_layout.hidden() {
                    "Show sidebar"
                } else {
                    "Hide sidebar"
                },
                self.keymap.label(Command::ToggleSidebar),
            ))
            .on_click(cx.listener(|shell, _, window, cx| shell.toggle_sidebar(window, cx)))
            .child(icon_svg(
                Icon::PanelLeft,
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ))
            .when(modal, |el| el.relative().child(inert_layer()));
        // Full width ↔ compact rail (a hidden sidebar comes back as the rail).
        let compact = self.sidebar_layout.compact();
        let compact_toggle = div()
            .id("toggle-compact-sidebar")
            .flex_none()
            .flex()
            .items_center()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
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
            }))
            .child(icon_svg(
                if compact {
                    Icon::PanelLeftOpen
                } else {
                    Icon::PanelLeftClose
                },
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ))
            .when(modal, |el| el.relative().child(inert_layer()));

        let waiting = self.inbox_entries().len();
        let inbox = div()
            .id("status-inbox")
            .flex_none()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_color(rgb(if waiting > 0 {
                theme.status_needs_input
            } else {
                theme.text_muted
            }))
            .child(inbox::label(waiting))
            // Nothing to jump to while clear: no pointer, no hover, no click.
            .when(waiting > 0, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .tooltip(tooltip(
                        theme,
                        "Jump to next item needing attention",
                        self.keymap.label(Command::NextInbox),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.jump_next_inbox(window, cx);
                    }))
            })
            .when(modal, |el| el.relative().child(inert_layer()));

        let git = self
            .git_summary()
            .and_then(|git| Some((git.head_label()?, git.branch_tooltip()?)))
            .map(|(label, lines)| {
                div()
                    .id("status-git")
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, &lines, ""))
                    .child(label)
            });

        // Voice input (§20G): level meter and recording time, transcribing, or the last outcome.
        let voice_status = match &self.voice {
            voice::State::Idle => None,
            voice::State::Recording { since, level, .. } => {
                let seconds = since.elapsed().as_secs();
                let color = theme.status_needs_input;
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .child(voice_meter(level.meter(), rgb(color)))
                        .child(notice(
                            color,
                            format!(
                                "{}:{:02} · {} stops",
                                seconds / 60,
                                seconds % 60,
                                self.keymap.label(Command::VoiceInput)
                            ),
                        )),
                )
            }
            voice::State::Transcribing => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .child(working_dots(status::now_ms(), rgb(theme.text_muted)))
                    .child(notice(theme.text_muted, "Transcribing".to_owned())),
            ),
            voice::State::Message(text) => Some(notice(theme.text_muted, text.clone())),
        };

        let agent = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id))
            .filter(|session| session.kind == SessionKind::Agent)
            .map(|session| self.status_agent_fields(session, theme))
            .unwrap_or_default();

        div()
            .flex_none()
            .h(theme::rems_px(theme::STATUS_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::S))
            .overflow_hidden()
            .bg(rgb(theme.sidebar))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::META)
            .child(toggle)
            .child(compact_toggle)
            .child(inbox)
            .children(git)
            .children(voice_status)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::M))
                    .children(agent),
            )
            .child(self.inspector_toggle(theme, cx))
    }
}

pub(crate) fn workspace_row(
    row: WorkspaceRow,
    active: bool,
    run_tip: Option<String>,
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
        .child(icon_svg(
            if row.expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            theme::ICON_SIZE_SMALL,
            theme.chevron_color(),
        ))
        .child(workspace_avatar(&row.name, color, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::BODY)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(row.name),
        )
        .when(row.pinned, |el| el.child(pin_mark()))
        .when_some(run_tip, |el, tip| {
            el.child(
                div()
                    .id(SharedString::from(format!("workspace-run-{project_id}")))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, &tip, ""))
                    .child("▶"),
            )
        })
        .when_some(row.summary.clone(), |el, summary| {
            el.child(
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(summary),
            )
        })
        .when(row.needs_input, |el| {
            el.child(
                div()
                    .text_size(font_size::META)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.status_needs_input))
                    .child(sidebar::status_display(SessionState::NeedsInput).icon),
            )
        })
        .child(
            row_action(
                SharedString::from(format!("workspace-menu-{project_id}")),
                icon_svg(Icon::Ellipsis, theme::ICON_SIZE_SMALL, theme.text_muted),
                theme,
            )
            // Shown on hover, and always on the active row (SPEC §14 progressive disclosure).
            .when(!active, |el| {
                el.opacity(opacity::HIDDEN)
                    .group_hover(group, |style| style.opacity(opacity::FULL))
            })
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
        .when(selected, |el| el.bg(rgb(theme.selected)))
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
            // The icon says the state; its hover says it in words and when it changed.
            div()
                .id(SharedString::from(format!("session-status-{session_id}")))
                .flex_none()
                .tooltip(status_tooltip(
                    theme,
                    row.state,
                    row.state_source,
                    row.state_changed_at,
                ))
                .child(mark_slot(row.mark(), now, theme)),
        )
        .child(agent_chip(badge, theme))
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
                        .when(selected, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(name),
                )
                .children(
                    progress
                        .map(|fraction| progress_bar(fraction, theme::PLAN_ROW_BAR_HEIGHT, theme)),
                ),
        )
        .when(row.pinned, |el| el.child(pin_mark()))
        // A right-aligned slot of a minimum width, kept without a time, so every row's time
        // and title end in one column.
        .child(
            div()
                .flex_none()
                .min_w(theme::rems_px(theme::SESSION_TIME_WIDTH))
                .flex()
                .justify_end()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .children(time),
        )
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
