//! The right panel (file tree, session cards, Git list) and its title bar toggle.

use crate::*;

impl Shell {
    /// `ToggleInspector` / the title bar's `◨`: hides or shows the window's right panel (its
    /// tab is kept); from a view without it, shows Terminals with the panel open.
    pub(crate) fn toggle_inspector(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let host = self.space_host(window);
        if self.workspace_of(host).is_none() {
            return;
        }
        let mode_before = self.panel(host).mode();
        let (tree_before, git_before) = if self.panel_view(host) {
            let before = (self.files_on_screen(host), self.git_shown(host));
            let panel = self.panel_mut(host);
            panel.open = !panel.open;
            before
        } else {
            self.panel_mut(host).open = true;
            self.show_view(View::Terminals, window, cx);
            // `show_view` refreshed the tree / Git list if it came on screen.
            (self.files_on_screen(host), self.git_shown(host))
        };
        self.panel_changed(tree_before, git_before, mode_before, window, cx);
    }

    /// The right panel of Space window `host`, to change it.
    pub(crate) fn panel_mut(&mut self, host: HostId) -> &mut inspector::RightPanel {
        let default = self.panel_default;
        &mut (self.hosts.entry(host))
            .or_insert_with(|| shell::hosts::HostUi {
                right_panel: default,
                ..Default::default()
            })
            .right_panel
    }

    /// A click on the panel's `Files` / `Session` tab.
    pub(crate) fn show_panel_tab(
        &mut self,
        tab: PanelTab,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let host = self.space_host(window);
        let (tree_before, git_before, mode_before) = (
            self.files_on_screen(host),
            self.git_shown(host),
            self.panel(host).mode(),
        );
        self.panel_mut(host).show(tab);
        self.panel_changed(tree_before, git_before, mode_before, window, cx);
    }

    /// Ctrl+Shift+3: the right panel on the file tree, which gets the keyboard; from a view
    /// without the panel, Terminals first.
    pub(crate) fn show_file_tree(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace_in(window) else {
            return;
        };
        if !self.panel_view(self.space_host(window)) {
            self.show_view(View::Terminals, window, cx);
        }
        self.show_panel_tab(PanelTab::Files, window, cx);
        // Without loaded tabs the Terminals view draws no panel: nothing to focus.
        if self.tabs.contains_key(&project_id) {
            window.focus(&self.files_focus, cx);
        }
    }

    /// Ctrl+Shift+4: the right panel on the Git tab, whose list gets the keyboard; from a
    /// view without the panel, Terminals first.
    pub(crate) fn show_git_panel(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace_in(window) else {
            return;
        };
        if !self.panel_view(self.space_host(window)) {
            self.show_view(View::Terminals, window, cx);
        }
        self.show_panel_tab(PanelTab::Git, window, cx);
        // Without loaded tabs the Terminals view draws no panel: nothing to focus.
        if self.view(self.space_host(window)) != View::Terminals
            || self.tabs.contains_key(&project_id)
        {
            window.focus(&self.git_focus, cx);
        }
    }

    /// After the panel opened, closed or switched tab: a tree or Git list that came on screen
    /// is refreshed, a hidden one gives the keyboard back, the grip of a hidden panel ends its
    /// drag, and `[ui] right_panel` is saved.
    fn panel_changed(
        &mut self,
        tree_before: bool,
        git_before: bool,
        mode_before: mkrew_core::settings::RightPanelMode,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let host = self.space_host(window);
        let panel = self.panel(host);
        if !panel.open {
            // The grip is gone: no mouse-up will end a drag in progress.
            self.end_inspector_drag(cx);
        }
        if self.file_tree_shown(host) {
            if !tree_before && let Some(project_id) = self.workspace_of(host) {
                self.refresh_files(project_id, window, cx);
            }
        } else if self.files_focus.is_focused(window) {
            self.focus_center(window, cx);
        }
        if self.git_shown(host)
            && !git_before
            && let Some(project_id) = self.workspace_of(host)
        {
            self.refresh_git(project_id, window, cx);
        }
        // The Git tab went away under the keyboard: its list, filter field or commit box.
        if !self.git_panel_shown(host)
            && [
                &self.git_focus,
                &self.git_filter_focus,
                &self.git_commit_focus,
            ]
            .into_iter()
            .any(|focus| focus.is_focused(window))
        {
            self.focus_center(window, cx);
        }
        // A new Space window starts with the panel changed last.
        self.panel_default = panel;
        let value = format!("\"{}\"", panel.mode().name());
        if panel.mode() != mode_before
            && let Err(error) =
                mkrew_core::settings::write_ui_value(&self.settings_file, "right_panel", &value)
        {
            self.settings_write_failed("the right panel", error, cx);
        }
        self.sync_watches(cx);
        cx.notify();
    }

    /// The keyboard back to the center after the panel's focused tab went away: the focused
    /// pane (a file pane keeps it in its editor, a diff pane in `git_focus`).
    pub(crate) fn focus_center(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.focus_pane(window, cx);
    }

    /// Alt+arrows (SPEC §13): the next pane that way. Past the edge (or without panes) ←
    /// enters the sidebar and → the right panel's file tree or Git list, only while on screen;
    /// from the panel ← comes back to the center.
    pub(crate) fn move_focus(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let sideways = matches!(direction, Direction::Left | Direction::Right);
        let in_panel = (self.file_tree_shown(self.space_host(window))
            && self.files_focus.is_focused(window))
            || (self.git_panel_shown(self.space_host(window)) && self.git_focus.is_focused(window));
        if sideways && in_panel {
            if direction == Direction::Left {
                self.focus_center(window, cx);
            }
            return;
        }
        if (self.detached_key(window).is_some()
            || self.view(self.space_host(window)) == View::Terminals)
            && self
                .with_tab(window, |tabs| tabs.focus_neighbour(direction))
                .unwrap_or(false)
        {
            // A focus move ends a zoom: the hidden panes come back (and re-attach).
            self.sync_panes(window, cx);
            cx.notify();
            return;
        }
        match direction {
            // The compact rail widens (it has no filter); a hidden sidebar stays hidden.
            Direction::Left if self.sidebar_drawn() => self.focus_sidebar(window, cx),
            Direction::Right => {
                // Without loaded tabs the Terminals view draws no panel.
                let drawn = self.view(self.space_host(window)) != View::Terminals
                    || self
                        .workspace_in(window)
                        .is_some_and(|id| self.tabs.contains_key(&id));
                if drawn && self.file_tree_shown(self.space_host(window)) {
                    window.focus(&self.files_focus, cx);
                } else if drawn && self.git_panel_shown(self.space_host(window)) {
                    window.focus(&self.git_focus, cx);
                }
            }
            _ => {}
        }
    }

    /// The edge was released: saves the dragged width as `[ui] inspector_width`.
    pub(crate) fn end_inspector_drag(&mut self, cx: &mut Context<'_, Self>) {
        if std::mem::take(&mut self.inspector_dragging) {
            self.save_width("inspector_width", self.inspector_width, cx);
        }
    }

    /// Saves a dragged panel width as `[ui] <key>` (one write per drag; the file is hand-edited
    /// config). A file that does not parse is not written; the palette lists why (§20.0) and a
    /// toast says the width was not saved.
    pub(crate) fn save_width(&mut self, key: &str, width: f32, cx: &mut Context<'_, Self>) {
        // Clamped to its theme limits while dragged, so it fits a u16.
        let width = (width.round() as u16).to_string();
        if let Err(error) = mkrew_core::settings::write_ui_value(&self.settings_file, key, &width) {
            self.settings_write_failed("the panel width", error, cx);
        }
    }

    /// The title bar's panel toggle: a gray icon showing the panel's side, on a raised fill
    /// while the panel is open.
    pub(crate) fn inspector_toggle(
        &self,
        host: HostId,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let open = self.panel_shown(host);
        let glyph = match self.panel_side {
            PanelSide::Right => Icon::PanelRight,
            PanelSide::Left => Icon::PanelLeft,
        };
        icon_button_in(
            "toggle-inspector",
            glyph,
            if open { theme.text } else { theme.text_muted },
            theme,
        )
        .when(open, |el| el.bg(rgb(theme.surface_raised)))
        .tooltip(tooltip(
            theme,
            if open { "Hide panel" } else { "Show panel" },
            self.keymap.label(Command::ToggleInspector),
        ))
        .on_click(cx.listener(|shell, _, window, cx| shell.toggle_inspector(window, cx)))
    }

    /// Moves the panel to the other side and saves `[ui] panel_side` (ADR 0115).
    pub(crate) fn toggle_panel_side(&mut self, cx: &mut Context<'_, Self>) {
        self.panel_side = self.panel_side.other();
        let value = format!("\"{}\"", self.panel_side.name());
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "panel_side", &value)
        {
            self.settings_write_failed("the panel side", error, cx);
        }
        cx.notify();
    }

    /// The drawn sidebar's width in px: what sits left of a left panel.
    fn sidebar_px(&self, window: &Window) -> f32 {
        if !self.sidebar_drawn() {
            0.0
        } else if self.sidebar_layout.compact() {
            f32::from(theme::rems_px(theme::COMPACT_SIDEBAR_WIDTH).to_pixels(window.rem_size()))
        } else {
            self.sidebar_width
        }
    }

    /// The title bar's sidebar toggle (left of the workspace switcher): hide or show the sidebar.
    pub(crate) fn sidebar_toggle(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        icon_button("toggle-sidebar", Icon::PanelLeft, theme)
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
    }

    /// The right panel (SPEC §10, §12A, §12B): `Files | Session | Git` tabs, right of
    /// Terminals. Rendered only while open; it is a sibling of the
    /// panes, never inside the terminal element.
    pub(crate) fn render_right_panel(
        &self,
        project_id: Ulid,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let side = self.panel_side;
        let grip = div()
            .id("inspector-grip")
            .absolute()
            .top_0()
            .bottom_0()
            .when(side == PanelSide::Right, |el| el.left_0())
            .when(side == PanelSide::Left, |el| el.right_0())
            .w(px(theme::GRIP_WIDTH))
            .cursor(CursorStyle::ResizeLeftRight)
            .on_drag(InspectorGrip, |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(
                cx.listener(|shell, event: &DragMoveEvent<InspectorGrip>, window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                    // ponytail: live width, one `session.resize` per changed column count;
                    // throttle if an agent TUI flickers while dragging.
                    let side = shell.panel_side;
                    let left = match side {
                        PanelSide::Right => 0.0,
                        PanelSide::Left => shell.sidebar_px(window),
                    };
                    shell.inspector_width = inspector::width_from_pointer(
                        f32::from(window.viewport_size().width),
                        f32::from(event.event.position.x) - left,
                        side,
                    );
                    shell.inspector_dragging = true;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|shell, _, _, cx| shell.end_inspector_drag(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, _, _, cx| shell.end_inspector_drag(cx)),
            );
        let tab = self.panel_tab(self.space_host(window));
        let changed = self.git_summary(project_id).map_or(0, GitSummary::changed);
        // The tab row has the height of the title bar's tab chips and the sidebar's filter
        // row on its left: same height, same bottom divider. Icon tabs on the left, Tern style
        // (ADR 0110); the title is in the tooltip.
        let tabs = div()
            .flex_none()
            .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .children(
                [PanelTab::Files, PanelTab::Session, PanelTab::Git]
                    .into_iter()
                    .filter(|&each| each != PanelTab::Session || self.features.session)
                    .map(|each| {
                        let (title, icon, keys) = match each {
                            PanelTab::Files => (
                                "Files",
                                Icon::Files,
                                self.keymap.label(Command::ShowFileTree),
                            ),
                            PanelTab::Session => ("Session", Icon::Bot, String::new()),
                            PanelTab::Git => (
                                "Git",
                                Icon::GitBranch,
                                self.keymap.label(Command::ShowGitPanel),
                            ),
                        };
                        panel_tab(
                            SharedString::from(format!("panel-{title}")),
                            icon,
                            each == tab,
                            theme,
                        )
                        .tooltip(tooltip(theme, title, keys))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_panel_tab(each, window, cx);
                        }))
                        // The workspace's changed files over all repositories, hidden at 0.
                        .when(each == PanelTab::Git && changed > 0, |el| {
                            el.child(tinted_chip(changed.to_string(), theme.accent_text, theme))
                        })
                    }),
            );
        let body = match tab {
            PanelTab::Files => self.render_file_tree(project_id, theme, window, cx),
            PanelTab::Session => {
                let session = self
                    .panel_session(self.space_host(window))
                    .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
                let thread = session.and_then(|s| self.render_thread_section(s, theme, cx));
                let cards = div()
                    .id("inspector-body")
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(space::M))
                    .p(px(space::M))
                    .children(match session {
                        Some(session) => {
                            vec![self.render_session_card(session, now, theme, cx)]
                        }
                        None => {
                            vec![meta_line(theme, "No session focused".into()).into_any_element()]
                        }
                    })
                    // Follows the workspace, not the focused pane.
                    .children(
                        self.render_changes_card(project_id, theme, cx)
                            .map(IntoElement::into_any_element),
                    )
                    .children(
                        session
                            .filter(|s| {
                                s.kind == SessionKind::Agent && s.state != SessionState::Exited
                            })
                            .map(|s| self.render_plan_card(s, theme, cx)),
                    );
                match thread {
                    // The cards keep their height (at most half); the thread fills the rest.
                    Some(thread) => div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .child(cards.flex_none().max_h(relative(0.5)))
                        .child(thread)
                        .into_any_element(),
                    None => cards.size_full().into_any_element(),
                }
            }
            PanelTab::Git => self.render_git_panel(project_id, theme, window, cx),
        };
        div()
            .flex_none()
            .w(px(self.inspector_width))
            .h_full()
            .relative()
            .when(side == PanelSide::Right, |el| {
                el.border_l(px(theme::BORDER_WIDTH))
            })
            .when(side == PanelSide::Left, |el| {
                el.border_r(px(theme::BORDER_WIDTH))
            })
            .border_color(rgb(theme.divider))
            .bg(rgb(theme.bg))
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .flex()
            .flex_col()
            .child(tabs)
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            .child(grip)
            .into_any_element()
    }

    /// One line per repository with changes (`M 23 ?3` + Review diff); `None` when the
    /// workspace has no repository (the card is hidden).
    pub(crate) fn render_changes_card(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or("workspace", |p| p.name.as_str());
        let lines = inspector::changes_card(self.git_summary(project_id), workspace)?;
        let card = inspector_card(theme, "Changes");
        if lines.is_empty() {
            return Some(card.child(meta_line(theme, "Working tree clean".into())));
        }
        Some(
            card.children(lines.into_iter().enumerate().map(|(i, line)| {
                let repo = line.repo.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(div().flex_1().min_w_0().truncate().child(line.label))
                    .children(
                        line.modified.map(|text| {
                            div().flex_none().text_color(rgb(theme.accent)).child(text)
                        }),
                    )
                    .children(line.untracked.map(|text| {
                        div()
                            .flex_none()
                            .text_color(rgb(theme.diff_add_text))
                            .child(text)
                    }))
                    .child(
                        row_action(
                            SharedString::from(format!("inspector-review-{i}")),
                            "Review diff",
                            theme,
                        )
                        .flex_none()
                        .tooltip(tooltip(theme, "Open the Git view on this repository", ""))
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                shell.review_diff(project_id, repo.clone(), window, cx);
                            },
                        )),
                    )
            })),
        )
    }

    /// The right panel's Plan card for an agent session (ADR 0039): the linked file's
    /// progress and checklist, or a button to link one.
    pub(crate) fn render_plan_card(
        &self,
        session: &Session,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let id = session.id;
        let linked = session.plan_path.is_some();
        let card = self.plans.get(id).map(plan::card);
        let title = match (&card, &session.plan_path) {
            (Some(card), _) => format!("Plan {}", card.name),
            (None, Some(path)) => format!("Plan {}", plan::name_of(path)),
            (None, None) => "Plan".to_owned(),
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::BODY)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text))
                    .child(title),
            )
            .children(
                card.as_ref()
                    .filter(|card| card.error.is_none())
                    .map(|card| meta_line(theme, card.count.clone().into()).flex_none()),
            );
        let body = match &card {
            Some(card) => match &card.error {
                Some(error) => div().child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.diff_del_text))
                        .child(error.clone()),
                ),
                None => div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(progress_bar(card.fraction, theme::PLAN_BAR_HEIGHT, theme))
                    .children(card.rows.iter().map(|row| plan_row(row, theme)))
                    .children(
                        (card.more > 0)
                            .then(|| meta_line(theme, format!("{} more", card.more).into())),
                    ),
            },
            None if linked => div().child(meta_line(theme, "Reading the plan…".into())),
            None => div().child(meta_line(
                theme,
                "Link a Markdown checklist to follow this session's progress.".into(),
            )),
        };
        let actions = if linked {
            div()
                .flex()
                .gap(px(space::S))
                .child(quiet_button("plan-change", "Change…", theme).on_click(
                    cx.listener(move |shell, _, window, cx| shell.pick_plan(id, window, cx)),
                ))
                .child(
                    quiet_button(
                        "plan-unlink",
                        icon_text(Icon::Unlink, "Unlink", theme.text_muted),
                        theme,
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.link_plan(id, None, window, cx);
                    })),
                )
        } else {
            div().flex().child(
                button(
                    "plan-link",
                    icon_text(Icon::Link, "Link plan…", button_fg(theme, true)),
                    theme,
                    true,
                )
                .on_click(cx.listener(move |shell, _, window, cx| shell.pick_plan(id, window, cx))),
            )
        };
        inspector_card(theme, "Plan")
            .child(header)
            .child(body)
            .child(actions)
            .children(
                self.plan_error
                    .as_ref()
                    .filter(|(sid, _)| *sid == id)
                    .map(|(_, error)| {
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.diff_del_text))
                            .child(error.clone())
                    }),
            )
            .into_any_element()
    }

    /// Agent, title, state + time, usage, Open transcript and Stop of `session`.
    pub(crate) fn render_session_card(
        &self,
        session: &Session,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let card = inspector::session_card(session, self.usages.get(session.id), now);
        let id = session.id;
        let open = match &card.transcript {
            Some(_) => button(
                "inspector-transcript",
                icon_text(Icon::FileText, "Open transcript", button_fg(theme, false)),
                theme,
                false,
            )
            .tooltip(tooltip(
                theme,
                "Open the agent's transcript file in Zed",
                "",
            ))
            .on_click(cx.listener(move |shell, _, _, cx| shell.open_transcript(id, cx)))
            .into_any_element(),
            None => disabled_button(
                "inspector-transcript",
                icon_text(Icon::FileText, "Open transcript", theme.text_muted),
                theme,
                false,
            )
            .tooltip(tooltip(theme, "No transcript yet", ""))
            .into_any_element(),
        };
        let stop = match inspector::stop_action(Some(session)) {
            inspector::StopAction::Nothing => None,
            action => Some(action),
        }
        .map(|action| {
            let hint = if action == inspector::StopAction::Ask {
                "Stop this session (asks first)"
            } else {
                "Stop this shell"
            };
            danger_button("inspector-stop", "Stop", theme)
                .tooltip(tooltip(theme, hint, ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.stop_session(id, window, cx);
                }))
        });
        inspector_card(theme, "Session")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(agent_chip(
                        self.agent_badge(&session.agent_profile),
                        theme.text,
                        theme,
                    ))
                    .child(div().min_w_0().truncate().child(card.agent.clone()))
                    .child(div().flex_1())
                    .child(status_mark(session.state, theme)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight(weight::MEDIUM))
                    .child(card.title.clone()),
            )
            .child(meta_line(theme, card.state.clone().into()))
            .children(card.usage.iter().map(|(label, value)| {
                div()
                    .flex()
                    .gap(px(space::S))
                    .text_size(font_size::META)
                    .child(
                        div()
                            .w(theme::rems_px(theme::INSPECTOR_LABEL_WIDTH))
                            .flex_none()
                            .text_color(rgb(theme.text_muted))
                            .child(*label),
                    )
                    .child(div().min_w_0().truncate().child(value.clone()))
            }))
            .child(div().flex().gap(px(space::S)).child(open).children(stop))
            .into_any_element()
    }

    /// "Open transcript": the agent's `.jsonl` in Zed. A failure (Zed missing, file gone) toasts
    /// like the other Zed actions.
    pub(crate) fn open_transcript(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        if self.local_only(cx) {
            return;
        }
        let Some(path) = self
            .usages
            .get(session)
            .and_then(|usage| usage.transcript.clone())
        else {
            return;
        };
        if let Err(error) = zed::open(Path::new(&path), None) {
            tracing::warn!(%session, path, "open transcript failed: {error}");
            self.toast_error(error.to_string(), cx);
        }
        cx.notify();
    }

    /// The card's Stop: `session.kill`. A running agent asks first; the pane stays until the
    /// session exits (ADR 0033).
    pub(crate) fn stop_session(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let action =
            inspector::stop_action(self.sidebar.sessions().iter().find(|s| s.id == session));
        match action {
            inspector::StopAction::Nothing => {}
            inspector::StopAction::Kill => self.kill(session, window, cx),
            inspector::StopAction::Ask => {
                let answer = window.prompt(
                    PromptLevel::Critical,
                    "This session is still running.",
                    Some("Stops the agent. Its pane closes when it exits."),
                    &inspector::STOP_ANSWERS,
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    if !task_dialog::confirms_delete(answer.await.ok()) {
                        return;
                    }
                    let _ = Shell::answer(&this, cx, |shell, window, cx| {
                        // It may have exited while the dialog was open.
                        if !shell.is_exited(session) {
                            shell.kill(session, window, cx);
                        }
                    });
                })
                .detach();
            }
        }
    }
}

/// Payload of a drag of the session panel's left edge (a resize, nothing is dropped).
pub(crate) struct InspectorGrip;

/// A card of the session panel: a muted title over its content (a block on `card`, like a board
/// card). The Changes and Plan cards use it too.
pub(crate) fn inspector_card(theme: &Theme, title: &'static str) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(space::S))
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.card))
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(title),
        )
}

/// One checklist line of the Plan card: a check box and the item text; the first unchecked
/// item is in the accent text color, checked ones are muted.
pub(crate) fn plan_row(row: &plan::Row, theme: &Theme) -> gpui::Div {
    let color = match (row.done, row.current) {
        (true, _) => theme.text_muted,
        (false, true) => theme.accent_text,
        (false, false) => theme.text_dim,
    };
    let check = div()
        .flex_none()
        .mt(px(space::XS / 2.0))
        .w(px(theme::DOC_TASK_BOX_SIZE))
        .h(px(theme::DOC_TASK_BOX_SIZE))
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if row.done { theme.accent } else { theme.subtle }))
        .when(row.done, |el| el.bg(rgb(theme.accent)));
    div()
        .flex()
        .items_start()
        .gap(px(space::S))
        .child(check)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(font_size::BODY)
                .text_color(rgb(color))
                .when(row.current, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(row.text.clone()),
        )
}
