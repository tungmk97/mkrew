//! Window chrome: the main area, title bar, branch-less workspace switcher.

use crate::*;

impl Shell {
    pub(crate) fn open_switcher(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.begin_menu(window, cx);
        self.workspace_switcher = true;
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    pub(crate) fn open_new_menu(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.begin_menu(window, cx);
        // Closing the menu gives the keyboard to the pane, not to `▾`, so the `+` row (open
        // while it holds focus) collapses with it (SPEC §10).
        self.menu_return = None;
        self.new_menu = true;
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    /// Every workspace (the shown one checked, one in another window marked ◳), then "Add
    /// workspace…". A pick opens it the way the window mode says (`route_workspace`).
    pub(crate) fn switcher_rows(&self, project_id: Ulid) -> Rows {
        let mut rows: Rows = self
            .sidebar
            .projects()
            .iter()
            .map(|project| {
                let id = project.id;
                MenuItem::new(
                    project.name.clone(),
                    act(move |shell, window, cx| {
                        let at = shell.menu_anchor(&Anchor::Switcher, window, cx);
                        let ctrl = window.modifiers().secondary();
                        shell.route_workspace(id, Then::Nothing, ctrl, Some(at), window, cx);
                    }),
                )
                .lead(Lead::Avatar {
                    name: project.name.clone(),
                    color: project.color.clone(),
                })
                .checked(id == project_id)
                .keys(if id != project_id && self.space_of(id).is_some() {
                    "◳"
                } else {
                    ""
                })
                .row()
            })
            .collect();
        rows.push(MenuRow::Separator);
        rows.push(
            MenuItem::new(
                "Add workspace…",
                act(|shell, window, cx| shell.add_workspace(window, cx)),
            )
            .row(),
        );
        rows
    }

    /// The `+ ▾` menu: one row per installed agent profile (the default first and checked,
    /// with the agent tab shortcut), the plain "Agent" row while none is listed, "Shell" and
    /// "Empty tab". Every row opens a new tab and shows Terminals; a new pane in the current
    /// tab comes from Split.
    pub(crate) fn new_menu_rows(&self, host: HostId) -> Rows {
        let mut rows: Rows = Vec::new();
        for agent in self.pane_agents(host) {
            let profile = agent.id.clone();
            let item = MenuItem::new(
                agent.name.clone(),
                act(move |shell, window, cx| {
                    shell.open_new_tab(
                        Some((SessionKind::Agent, Some(profile.clone()))),
                        window,
                        cx,
                    );
                }),
            )
            .lead(Lead::Agent(sidebar::agent_badge(
                agent.icon.as_deref(),
                &agent.id,
            )))
            .checked(agent.default);
            rows.push(
                if agent.default {
                    item.keys(self.keymap.label(Command::NewAgentTab))
                } else {
                    item
                }
                .row(),
            );
        }
        if rows.is_empty() {
            rows.push(
                MenuItem::new(
                    "Agent",
                    act(|shell, window, cx| {
                        shell.open_new_tab(Some((SessionKind::Agent, None)), window, cx);
                    }),
                )
                .keys(self.keymap.label(Command::NewAgentTab))
                .row(),
            );
        }
        rows.push(
            MenuItem::new(
                "Shell",
                act(|shell, window, cx| {
                    shell.open_new_tab(Some((SessionKind::Shell, None)), window, cx);
                }),
            )
            .keys(self.keymap.label(Command::NewShellTab))
            .row(),
        );
        if mkrew_ui::browser::available().is_ok() {
            rows.push(
                MenuItem::new(
                    "Browser",
                    act(|shell, window, cx| shell.new_browser(None, window, cx)),
                )
                .row(),
            );
        }
        rows.push(
            MenuItem::new(
                "Empty tab",
                act(|shell, window, cx| shell.open_new_tab(None, window, cx)),
            )
            .row(),
        );
        rows
    }

    /// While a lost connection is down (spec §6.3): "Connection to <name> lost. Next try in N s."
    /// (static, set once per attempt; "Reconnecting…" while one runs, nothing after a version
    /// mismatch), the last attempt's reason below it, and **Try now**, under the title bar.
    pub(crate) fn render_reconnect_banner(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.reconnecting() {
            return None;
        }
        let what = self.machine_name().unwrap_or("the daemon");
        let text = match (self.reconnect_in, &self.status) {
            (Some(wait), _) => format!(
                "Connection to {what} lost. Next try in {} s.",
                wait.as_secs()
            ),
            (None, Status::Connecting) => format!("Connection to {what} lost. Reconnecting…"),
            (None, _) => format!("Connection to {what} lost."),
        };
        Some(
            div()
                .id("reconnect-banner")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::M))
                .px(px(space::M))
                .py(px(space::S))
                .bg(rgb(theme.warning_bg))
                .border_b(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.divider))
                .text_size(font_size::META)
                .text_color(rgb(theme.text))
                .child(
                    div().min_w_0().flex_1().child(text).children(
                        self.attempt_detail()
                            .map(|detail| div().text_color(rgb(theme.text_muted)).child(detail)),
                    ),
                )
                // Not while an attempt runs (a second one would not help); after a version
                // mismatch it is the only way to try again.
                .children((!matches!(self.status, Status::Connecting)).then(|| {
                    button("reconnect-now", "Try now", theme, true)
                        .on_click(cx.listener(|shell, _, _, cx| shell.connect(cx)))
                }))
                .into_any_element(),
        )
    }

    pub(crate) fn render_main(
        &self,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        // The last screen stays while a lost connection is down (`reconnecting`).
        let Some(project_id) = self
            .workspace_in(window)
            .filter(|_| self.shows_workspaces())
        else {
            let (sentence, detail) = self.status_line();
            return message(theme, Some("Mkrew"), sentence, detail)
                .children(self.status_action(theme, cx))
                .flex_1()
                .h_full()
                .into_any_element();
        };
        let view = self.view(self.space_host(window));
        let main = div().flex_1().min_w_0().h_full().flex().flex_col();
        let panel = self
            .panel_shown(self.space_host(window))
            .then(|| self.render_right_panel(project_id, now, theme, window, cx));
        match (view, self.tabs.get(&project_id)) {
            (View::Board, _) => main.child(self.render_board(project_id, now, theme, window, cx)),
            (View::Docs, _) => main.child(self.render_docs(project_id, theme, window, cx)),
            (View::Terminals, Some(tabs)) => {
                // Voice text goes to the focused terminal when it has the keyboard: ring it.
                let voice_ring = self.voice_ring(theme, cx).filter(|_| {
                    (tabs.active().and_then(Tab::focused_session))
                        .and_then(|session| self.panes.get(&session))
                        .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
                });
                let every_tab_detached = !tabs.tabs().is_empty()
                    && (tabs.tabs().iter()).all(|tab| self.is_detached_tab(project_id, tab.id));
                let terminals = div().flex_1().min_w_0().h_full().flex().flex_col().child(
                    if every_tab_detached {
                        empty_state(
                            theme,
                            Some(">_"),
                            Some("Every tab of this workspace is in another window"),
                            "Press + for a new tab here.".into(),
                            Some("Or use Move all back in the other window.".into()),
                        )
                        .flex_1()
                    } else {
                        self.render_tab(
                            project_id,
                            tabs.active(),
                            now,
                            self.composer_focus.is_focused(window),
                            voice_ring,
                            theme,
                            self.hosts
                                .get(&self.host_of(window))
                                .map(|ui| ui.content.clone())
                                .unwrap_or_default(),
                            window,
                            cx,
                        )
                    },
                );
                let row = div().flex_1().min_h_0().flex();
                main.child(match self.panel_side {
                    PanelSide::Right => row.child(terminals).children(panel),
                    PanelSide::Left => row.children(panel).child(terminals),
                })
            }
            (View::Terminals, None) => {
                let (sentence, detail) = self.status_line();
                let action = self.status_action(theme, cx);
                main.child(
                    message(theme, None, sentence, detail)
                        .children(action)
                        .flex_1(),
                )
            }
        }
        .into_any_element()
    }

    /// The title bar above everything (SPEC §10, ADR 0106), one `TITLE_BAR_HEIGHT` bar: on the
    /// left (Space window) the machine chip, the sidebar toggle and the workspace switcher; the
    /// tab area (the terminal, file and Diff tabs, the `+` row); a drag area; on the right the
    /// voice status (only while the sidebar is hidden), the "need you" chip (a Space window in
    /// mode 2, ADR 0113), the limits chip (Space windows, only at 80% or more; it shows
    /// without a workspace too), the Run pill, the update button, the Board and Docs icons,
    /// Split (while Terminals shows in a Space window, always in a detached one) and the panel
    /// toggle; on Windows minimize, maximize, close. A detached window (`host` is not a Space
    /// window) has no left group, no workspace controls and no limits chip. A 1 px rule sets
    /// each group apart (left, tab area, right); the drag area sits
    /// between the last two. Every icon is gray: color is for status only. Without a workspace
    /// only the sidebar toggle, the limits chip, the drag area and the controls show, so the
    /// window can always be moved and closed.
    pub(crate) fn render_title_bar(
        &self,
        host: HostId,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let main = host.space().is_some();
        let workspace = self
            .workspace_in(window)
            .filter(|_| main && self.shows_workspaces());
        let view = self.view(self.space_host(window));
        // An open dialog keeps the mouse too: the bar's controls are inert, only the drag area
        // and the window controls stay live (they never touch the app's state).
        let modal = self.modal_open() || self.reconnecting();
        // Never shrinks: the tab area gives way first.
        let mut left = div().flex_none().flex().items_center().gap(px(space::S));
        if main {
            left = left.children(self.machine_chip(theme)).children(
                (self.window_mode == WindowMode::Single).then(|| self.sidebar_toggle(theme, cx)),
            );
        }
        if let Some(project_id) = workspace {
            left = left.child(self.render_workspace_switcher(project_id, theme, cx));
        }
        // The tab area; its strip is only recorded (for drops from other windows) while it is
        // painted.
        let tab_area = match host {
            HostId::Space(_) => workspace.and_then(|project_id| {
                let tabs = self.tabs.get(&project_id)?;
                Some(
                    self.render_tab_area(project_id, tabs, now, theme, window, cx)
                        .into_any_element(),
                )
            }),
            HostId::Detached(key) => {
                Some(self.render_detached_tab_area(key, now, theme, window, cx))
            }
        };
        if main
            && tab_area.is_none()
            && let Some(ui) = self.hosts.get(&host)
        {
            *ui.strip.borrow_mut() = None;
        }
        // The only drag region: the tab area is a sibling, so a click on a tab never moves
        // the window.
        let drag = div()
            .flex_1()
            // A drag region always remains: the tab area shrinks first.
            .min_w(px(space::XXL))
            .h_full()
            .window_control_area(WindowControlArea::Drag)
            .when(cfg!(target_os = "macos"), |el| {
                el.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, event: &MouseDownEvent, window, _| {
                        if event.click_count == 2 {
                            window.titlebar_double_click();
                        } else {
                            window.start_window_move();
                        }
                    }),
                )
            });
        let has_tabs = tab_area.is_some();
        let mut bar = div()
            .flex_none()
            .h(theme::rems_px(theme::TITLE_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::S))
            .pl(px(theme::title_bar_inset(
                Os::CURRENT,
                window.is_fullscreen(),
            )))
            .bg(rgb(theme.pane_gap))
            .font_family(theme.ui_font_family.clone())
            .child(left.when(modal, |el| el.relative().child(inert_layer())))
            // Rules set the groups apart: left | tabs | drag | right (never drag or press
            // targets, the drag area stays its own sibling).
            .when(main && has_tabs, |el| el.child(title_rule(theme)))
            .children(tab_area.map(|area| {
                div()
                    .min_w_0()
                    .flex_shrink_1()
                    .overflow_hidden()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(area)
                    .when(modal, |el| el.relative().child(inert_layer()))
            }))
            .when(has_tabs, |el| el.child(title_rule(theme)))
            .child(drag);
        let controls = DRAWN_CONTROLS && !window.is_fullscreen();
        let mut right: Vec<gpui::AnyElement> = Vec::new();
        // The sidebar footer shows voice input; with the sidebar hidden it shows here, so a
        // live microphone is never unmarked.
        if !self.sidebar_drawn() {
            right.extend((self.voice_status(false, theme)).map(IntoElement::into_any_element));
        }
        right.extend(self.render_need_you_chip(host, theme, cx));
        if main {
            right.extend(
                self.render_limits_chip(theme, cx)
                    .map(IntoElement::into_any_element),
            );
        }
        if let Some(project_id) = workspace {
            right.push(
                self.render_run_button(project_id, theme, cx)
                    .into_any_element(),
            );
        }
        if main {
            right.extend(
                self.render_update_button(theme, cx)
                    .map(IntoElement::into_any_element),
            );
        }
        // The focused agent pane's toggles (Chat, Reading width, Share), while its terminals
        // are on screen.
        if !main || (workspace.is_some() && view == View::Terminals) {
            right.extend(self.pane_toggles(window, theme, cx));
        }
        if workspace.is_some() {
            right.push(
                self.view_icon(View::Board, Icon::SquareKanban, view, theme, cx)
                    .into_any_element(),
            );
            right.extend(self.features.docs.then(|| {
                self.view_icon(View::Docs, Icon::BookOpen, view, theme, cx)
                    .into_any_element()
            }));
        }
        if let HostId::Detached(key) = host {
            right.push(
                icon_button("move-all-back", Icon::Minimize2, theme)
                    .tooltip(tooltip(theme, "Move all tabs back to the main window", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.move_all_back(key, window, cx);
                    }))
                    .into_any_element(),
            );
        }
        // Split acts on the shown terminal tab: only while Terminals shows in the main window.
        if !main || (workspace.is_some() && view == View::Terminals) {
            right.push(
                icon_button("tab-split", Icon::Columns2, theme)
                    .tooltip(tooltip(
                        theme,
                        "Split or pick a layout",
                        self.keymap.label(Command::SplitRight),
                    ))
                    .on_click(cx.listener(|shell, event: &gpui::ClickEvent, window, cx| {
                        shell.open_layout_menu(event.position(), window, cx);
                    }))
                    .into_any_element(),
            );
        }
        if workspace.is_some() {
            right.push(self.inspector_toggle(host, theme, cx).into_any_element());
        }
        let has_right = !right.is_empty();
        let right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .when(!controls, |el| el.pr(px(space::M)))
            .children(right);
        bar = bar
            .when(has_right, |el| el.child(title_rule(theme)))
            .child(right.when(modal, |el| el.relative().child(inert_layer())));
        if controls {
            // No click handlers: Windows hit-tests these areas (`HTMINBUTTON` / `HTMAXBUTTON` /
            // `HTCLOSE`) and runs the action itself; `HTMAXBUTTON` also brings the Snap
            // Layouts flyout.
            let max_icon = if window.is_maximized() {
                Icon::Copy
            } else {
                Icon::Square
            };
            bar = bar.child(
                div()
                    .flex_none()
                    .h_full()
                    .flex()
                    .items_center()
                    // A thin rule sets the window's own controls apart from the app's.
                    .child(
                        div()
                            .w(px(theme::BORDER_WIDTH))
                            .h(theme::rems_px(theme::TITLE_RULE_HEIGHT))
                            .mr(px(space::XS))
                            .bg(rgb(theme.divider)),
                    )
                    .child(window_control(
                        "win-min",
                        Icon::Minus,
                        WindowControlArea::Min,
                        theme,
                    ))
                    .child(window_control(
                        "win-max",
                        max_icon,
                        WindowControlArea::Max,
                        theme,
                    ))
                    .child(window_control(
                        "win-close",
                        Icon::X,
                        WindowControlArea::Close,
                        theme,
                    )),
            );
        }
        bar
    }

    /// In a machine window, right after the logo: the server icon, the machine's name and a
    /// status dot (green connected, amber connecting, red not reachable or another version).
    /// Not clickable; the tooltip says the state in words, so the dot is never the only cue
    /// (§11). It stays while disconnected.
    fn machine_chip(&self, theme: &Theme) -> Option<gpui::Stateful<gpui::Div>> {
        let name = self.machine_name()?;
        let state = self.machine_state.as_ref();
        let dot = super::machines::state_color(theme, state);
        let label = state.map_or_else(|| MachineState::Connecting.label(), MachineState::label);
        Some(
            div()
                .id("machine-chip")
                .min_w_0()
                .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                .flex()
                .items_center()
                .gap(px(space::XS))
                .h(theme::rems_px(theme::PILL_HEIGHT))
                .px(px(space::S))
                .rounded(px(radius::PILL))
                .bg(rgb(theme.well))
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .tooltip(tooltip(theme, &format!("{name}: {label}"), ""))
                .child(icon_svg(
                    Icon::Server,
                    theme::ICON_SIZE_SMALL,
                    theme.text_muted,
                ))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(name.to_owned()),
                )
                .child(status_dot(dot, false)),
        )
    }

    /// The workspace name with ▾: opens a dropdown of every workspace (click switches) and
    /// "Add workspace…".
    pub(crate) fn render_workspace_switcher(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let projects = self.sidebar.projects();
        let current = projects.iter().find(|project| project.id == project_id);
        let name = current.map_or("Workspace", |project| project.name.as_str());
        let color = current
            .and_then(|project| project.color.as_deref())
            .and_then(|name| theme.workspace_color(name));
        let button = div()
            .id("workspace-switch")
            .relative()
            .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::S))
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .rounded(px(radius::MEDIUM))
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::SEMIBOLD))
            .text_color(rgb(theme.text))
            .pressable(Look::Chip, true, theme)
            // Not while its dropdown is open: the tooltip would cover the rows.
            .when(!self.workspace_switcher, |el| {
                el.tooltip(tooltip(
                    theme,
                    "Switch workspace",
                    format!(
                        "{} … {}",
                        self.keymap.label(Command::Workspace(1)),
                        self.keymap.label(Command::Workspace(9))
                    ),
                ))
            })
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_press(
                true,
                cx.listener(|shell, _, window, cx| {
                    shell.open_switcher(window, cx);
                }),
            )
            .child(workspace_avatar(name, color, theme))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(name.to_owned()),
            )
            .child(icon_svg(
                Icon::ChevronDown,
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ))
            .child(record_bounds(Anchor::Switcher, cx));
        let menu = (self.workspace_switcher && !self.modal_open()).then(|| {
            render_menu(
                self.menu_state(),
                "workspace-switch-panel",
                self.switcher_rows(project_id),
                None,
                Place::Dropdown,
                theme,
                cx,
            )
        });
        div()
            .min_w_0()
            .child(button)
            .children(menu)
            .into_any_element()
    }
}
