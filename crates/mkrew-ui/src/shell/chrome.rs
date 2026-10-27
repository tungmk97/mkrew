//! Window chrome: the main area, title bar, view tabs, branch chip, workspace switcher.

use crate::*;

impl Shell {
    pub(crate) fn render_main(
        &self,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(project_id) = self.workspace.filter(|_| self.connection.is_some()) else {
            let (sentence, detail) = self.status_line();
            return message(theme, Some("Mkrew"), sentence, detail)
                .children(self.status_action(theme, cx))
                .flex_1()
                .h_full()
                .into_any_element();
        };
        let view = self.view();
        let main = div().flex_1().min_w_0().h_full().flex().flex_col();
        let panel = self
            .panel_shown()
            .then(|| self.render_right_panel(project_id, now, theme, window, cx));
        match (view, self.tabs.get(&project_id)) {
            (View::Board, _) => main.child(self.render_board(project_id, now, theme, window, cx)),
            (View::Docs, _) => main.child(self.render_docs(project_id, theme, window, cx)),
            (View::Editor, _) => main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_editor(project_id, theme, window, cx))
                    .children(panel),
            ),
            (View::Git, _) => main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_diff_view(project_id, theme, window, cx))
                    .children(panel),
            ),
            (View::Terminals, Some(tabs)) => {
                // Voice text goes to the focused terminal when it has the keyboard: ring it.
                let voice_ring = self.voice_ring(theme, cx).filter(|_| {
                    (tabs.active().and_then(Tab::focused))
                        .and_then(|session| self.panes.get(&session))
                        .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
                });
                let terminals = div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_tab_bar(project_id, tabs, now, theme, cx))
                    .child(self.render_tab(
                        tabs,
                        now,
                        self.composer_focus.is_focused(window),
                        voice_ring,
                        theme,
                        cx,
                    ));
                main.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(terminals)
                        .children(panel),
                )
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

    /// The title bar above everything (SPEC §10, ADR 0034): logo, sidebar toggle, workspace
    /// switcher (with the repository label) and views on the left; a drag area; the Run pill,
    /// History, the theme toggle, the phone icon (Phone access, ADR 0053), the session panel
    /// toggle and "+ New" (Agent / Shell); on Windows minimize, maximize, close. Every icon is
    /// gray: color is for status only. Without a workspace only the logo, the sidebar toggle,
    /// the drag area and the controls show, so the window can always be moved and closed.
    pub(crate) fn render_title_bar(
        &self,
        shown: View,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let workspace = self.workspace.filter(|_| self.connection.is_some());
        let mode = format!(
            "Theme: {} · {}",
            match theme.mode {
                theme::ThemeMode::System => "System",
                theme::ThemeMode::Light => "Light",
                theme::ThemeMode::Dark => "Dark",
            },
            theme.name
        );
        // An open dialog keeps the mouse too: the bar's controls are inert, only the drag area
        // and the window controls stay live (they never touch the app's state).
        let modal = self.modal_open();
        let mut left = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(app_logo(theme))
            .child(self.sidebar_toggle(theme, cx));
        if let Some(project_id) = workspace {
            left = left.child(self.render_workspace_switcher(project_id, theme, cx));
        }
        // Siblings of the controls, never their parent: the first area a hit test finds wins.
        // Two of them, so the views sit centered between the left and right groups.
        let drag = |cx: &mut Context<'_, Self>| {
            div()
                .flex_1()
                .min_w_0()
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
                })
        };
        let mut bar = div()
            .flex_none()
            .h(theme::rems_px(theme::TITLE_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::S))
            .pl(px(if cfg!(target_os = "macos") {
                theme::MAC_TRAFFIC_LIGHTS_WIDTH
            } else {
                space::M
            }))
            .bg(rgb(theme.chrome))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(left.when(modal, |el| el.relative().child(inert_layer())))
            .child(drag(cx));
        if workspace.is_some() {
            bar = bar
                .child(
                    div()
                        .flex_none()
                        .child(self.render_view_tabs(shown, theme, cx))
                        .when(modal, |el| el.relative().child(inert_layer())),
                )
                .child(drag(cx));
        }
        let controls = DRAWN_CONTROLS && !window.is_fullscreen();
        let mut right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .when(!controls, |el| el.pr(px(space::M)))
            // The sidebar footer shows voice input; with the sidebar hidden it shows here, so a
            // live microphone is never unmarked.
            .when(self.sidebar_layout.hidden(), |el| {
                el.children(self.voice_status(false, theme))
            });
        if let Some(project_id) = workspace {
            right = right
                .child(self.render_run_button(project_id, theme, cx))
                .child(
                    icon_button("history", Icon::History, theme)
                        .tooltip(tooltip(
                            theme,
                            "History: logs of the workspace's finished sessions",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_history(project_id, window, cx);
                        })),
                );
        }
        right = right
            .child(
                icon_button("toggle-theme", theme::mode_icon(theme.mode), theme)
                    .tooltip(tooltip(
                        theme,
                        &mode,
                        self.keymap.label(Command::ToggleTheme),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.toggle_theme(window, cx);
                    })),
            )
            .children(self.render_update_button(theme, cx))
            .children(self.render_phone_button(theme, cx));
        if workspace.is_some() {
            right = right.child(self.inspector_toggle(theme, cx)).child(
                div()
                    .pl(px(space::XS))
                    .child(self.render_new_button(now, theme, cx)),
            );
        }
        bar = bar.child(right.when(modal, |el| el.relative().child(inert_layer())));
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

    /// The views as a segmented control; the Git one carries the `Git N` badge (N = changed
    /// files over all repositories of the workspace, hidden at 0).
    pub(crate) fn render_view_tabs(
        &self,
        shown: View,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let changed = self.git_summary().map_or(0, GitSummary::changed);
        let editor = self.workspace.is_some_and(|id| self.has_file_tabs(id));
        segmented(theme).children(View::bar(self.features.docs, editor).map(|view| {
            segment(
                SharedString::from(format!("view-{}", view.title())),
                view == shown,
                theme,
            )
            .tooltip(tooltip(
                theme,
                view.title(),
                self.keymap.label(view.command()),
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.show_view(view, window, cx);
            }))
            .child(view.title())
            .when(view == View::Git && changed > 0, |el| {
                el.child(tinted_chip(changed.to_string(), theme.accent_text, theme))
            })
        }))
    }

    /// The workspace name with ▾: opens a dropdown of every workspace (click switches) and
    /// "Add workspace…". After it, muted, the repository label `⎇ master` / `⎇ 2 repos` (its
    /// only place; tooltip: one line per repository; a click shows the Git view), hidden while
    /// the summary is not loaded or the folder holds no repository.
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
        let hover = theme.hover;
        let button = div()
            .id("workspace-switch")
            .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::S))
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .rounded(px(radius::MEDIUM))
            .cursor_pointer()
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::SEMIBOLD))
            .text_color(rgb(theme.text))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
            .tooltip(tooltip(
                theme,
                "Switch workspace",
                self.keymap.label(Command::NextWorkspace),
            ))
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_click(cx.listener(|shell, _, _, cx| {
                shell.workspace_switcher = true;
                cx.notify();
            }))
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
            ));
        let repos = self.git_summary().and_then(|summary| {
            let label = summary.branch_label()?;
            let tip = summary.branch_tooltip().unwrap_or_default();
            Some(
                div()
                    .id("title-branch")
                    .min_w_0()
                    .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .h(theme::rems_px(theme::PILL_HEIGHT))
                    .px(px(space::S))
                    .rounded(px(radius::PILL))
                    .bg(rgb(theme.well))
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .hover(move |style| style.bg(rgb(hover)))
                    .tooltip(tooltip(theme, &tip, ""))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.show_view(View::Git, window, cx);
                    }))
                    .child(icon_svg(
                        Icon::GitBranch,
                        theme::ICON_SIZE_SMALL,
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(label),
                    ),
            )
        });
        let menu = (self.workspace_switcher && !self.modal_open()).then(|| {
            let rows = projects.iter().map(|project| {
                let id = project.id;
                let color = project
                    .color
                    .as_deref()
                    .and_then(|name| theme.workspace_color(name));
                menu_item(SharedString::from(format!("switch-{id}")), theme)
                    .child(workspace_avatar(&project.name, color, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text))
                            .child(project.name.clone()),
                    )
                    .when(id == project_id, |el| {
                        el.child(div().text_color(rgb(theme.accent_text)).child("✓"))
                    })
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.activate_workspace(id, window, cx);
                    }))
            });
            let panel = menu_panel("workspace-switch-panel", theme)
                .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                    shell.workspace_switcher = false;
                    cx.notify();
                }))
                .children(rows)
                .child(
                    div()
                        .h(px(theme::BORDER_WIDTH))
                        .my(px(space::XS))
                        .bg(rgb(theme.border)),
                )
                .child(
                    menu_item("switch-add".into(), theme)
                        .child(
                            div()
                                .flex_1()
                                .text_color(rgb(theme.text))
                                .child("Add workspace…"),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.workspace_switcher = false;
                            shell.add_workspace(window, cx);
                        })),
                );
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(space::S))
                    .child(div().pt(px(space::XS)).child(panel)),
            )
        });
        div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .child(div().child(button).children(menu))
            .children(repos)
            .into_any_element()
    }

    /// "+ New ▾": a dropdown of "Agent" and "Shell" (each with its shortcut) starting a new pane
    /// with the workspace's default agent or shell. While a pane starts it shows the loading
    /// mark instead (one start at a time).
    pub(crate) fn render_new_button(
        &self,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        if let Some(kind) = self.starting_pane {
            return starting_button("new-pane", kind, now, theme, true);
        }
        let button = button(
            "new-pane",
            icon_text(Icon::Plus, "New", theme.on_accent).child(icon_svg(
                Icon::ChevronDown,
                theme::ICON_SIZE_SMALL,
                theme.on_accent,
            )),
            theme,
            true,
        )
        .flex_none()
        .tooltip(tooltip(
            theme,
            "New agent or shell pane",
            format!(
                "{} / {}",
                self.keymap.label(Command::SplitAgent),
                self.keymap.label(Command::SplitShell)
            ),
        ))
        // An open menu already closed on this mouse down (outside it): the click reopens it.
        .on_click(cx.listener(|shell, _, _, cx| {
            shell.new_menu = true;
            cx.notify();
        }));
        let menu = (self.new_menu && !self.modal_open()).then(|| {
            // One row per installed agent profile, the default first and checked; with none
            // listed (still loading) the plain "Agent" row starts the default.
            let agents = self.pane_agents();
            let mut rows: Vec<gpui::AnyElement> = agents
                .into_iter()
                .map(|agent| {
                    let profile = agent.id.clone();
                    menu_item(format!("new-agent-pane-{}", agent.id).into(), theme)
                        .child(
                            div()
                                .w(px(space::M))
                                .text_color(rgb(theme.accent))
                                .child(if agent.default { "✓" } else { "" }),
                        )
                        .child(agent_chip(
                            sidebar::agent_badge(agent.icon.as_deref(), &agent.id),
                            theme.text,
                            theme,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_color(rgb(theme.text))
                                .when(agent.default, |el| {
                                    el.font_weight(FontWeight(weight::MEDIUM))
                                })
                                .child(agent.name),
                        )
                        .when(agent.default, |el| {
                            el.child(kbd(self.keymap.label(Command::SplitAgent), theme, false))
                        })
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.new_menu = false;
                            shell.show_view(View::Terminals, window, cx);
                            shell.new_pane_with(
                                SessionKind::Agent,
                                Some(profile.clone()),
                                Axis::Vertical,
                                window,
                                cx,
                            );
                        }))
                        .into_any_element()
                })
                .collect();
            let plain = if rows.is_empty() {
                vec![(
                    "new-agent-pane",
                    "Agent",
                    SessionKind::Agent,
                    Command::SplitAgent,
                )]
            } else {
                vec![]
            };
            rows.extend(
                plain
                    .into_iter()
                    .chain([(
                        "new-shell-pane",
                        "Shell",
                        SessionKind::Shell,
                        Command::SplitShell,
                    )])
                    .map(|(id, label, kind, command)| {
                        menu_item(id.into(), theme)
                            .child(div().flex_1().text_color(rgb(theme.text)).child(label))
                            .child(kbd(self.keymap.label(command), theme, false))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                cx.stop_propagation();
                                shell.new_menu = false;
                                // The title bar shows it in every view; the new pane is seen in Terminals.
                                shell.show_view(View::Terminals, window, cx);
                                shell.new_pane(kind, Axis::Vertical, window, cx);
                            }))
                            .into_any_element()
                    }),
            );
            let panel = menu_panel("new-pane-panel", theme)
                .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                    shell.new_menu = false;
                    cx.notify();
                }))
                .children(rows);
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(space::S))
                    .child(div().pt(px(space::XS)).child(panel)),
            )
        });
        div().child(button).children(menu).into_any_element()
    }
}
