//! Workspace, file, tab and pane menus.

use crate::*;

impl Shell {
    /// The workspace menu: "Pin", "Open in Zed", "Run setup…", the default agent and the default shell
    /// for new panes, the workspace color and "Remove workspace…". Floats over the sidebar; a
    /// click outside closes it.
    pub(crate) fn render_workspace_menu(
        &self,
        menu: &WorkspaceMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let project_id = menu.project_id;
        let current = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id)
            .and_then(|project| project.default_agent.as_deref());
        let current_color = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .and_then(|project| project.color.as_deref());
        let pinned = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .is_some_and(|project| project.pinned);
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let agents = agent_rows(
            &menu.agents,
            SessionKind::Agent,
            current,
            "default-agent",
            theme,
            cx,
            move |shell, agent, window, cx| shell.set_default_agent(project_id, agent, window, cx),
        );
        let current_shell = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .and_then(|project| project.default_shell.as_deref());
        // Loading and errors already show once, under the agents.
        let shells = match &menu.agents {
            Agents::Loaded(_) => agent_rows(
                &menu.agents,
                SessionKind::Shell,
                current_shell,
                "default-shell",
                theme,
                cx,
                move |shell, id, window, cx| shell.set_default_shell(project_id, id, window, cx),
            ),
            _ => Vec::new(),
        };
        let heading = |text: &'static str| {
            div()
                .px(px(space::S))
                .pt(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let panel = menu_panel("workspace-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.workspace_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("pin-workspace".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child(if pinned {
                        "Unpin"
                    } else {
                        "Pin"
                    }))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_workspace(project_id, !pinned, window, cx);
                    })),
            )
            .child(separator())
            .child(
                menu_item("open-in-zed".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Open in Zed"),
                    )
                    .child(
                        div()
                            .text_size(font_size::META)
                            .child(self.keymap.label(Command::OpenInZed)),
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.open_in_zed(project_id, cx);
                    })),
            )
            .child(
                menu_item("open-in-explorer".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Open in File Explorer"),
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.open_workspace_in_explorer(project_id, cx);
                    })),
            )
            .child(separator())
            .child(
                menu_item("workspace-run-setup".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Run setup…"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.open_run_setup(project_id, false, window, cx);
                    })),
            )
            .child(separator())
            .child(heading("Default agent for new panes"))
            .children(agents)
            .child(separator())
            .child(heading("Default shell for new panes"))
            .children(shells)
            .child(separator())
            .child(heading("Color"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .py(px(space::S))
                    .child(color_swatch(
                        "workspace-color",
                        None,
                        current_color,
                        theme,
                        cx,
                        move |shell, color, window, cx| {
                            shell.set_workspace_color(project_id, color, window, cx);
                        },
                    ))
                    .children(WORKSPACE_COLORS.iter().map(|name| {
                        color_swatch(
                            "workspace-color",
                            Some(*name),
                            current_color,
                            theme,
                            cx,
                            move |shell, color, window, cx| {
                                shell.set_workspace_color(project_id, color, window, cx);
                            },
                        )
                    })),
            )
            .child(separator())
            .child(
                menu_item("remove-workspace".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            // The destructive color of `danger_button`.
                            .text_color(rgb(theme.diff_del_text))
                            .child("Remove workspace…"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.archive_workspace(project_id, window, cx);
                    })),
            );
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }

    /// The Files tree's right-click menu at the click position; a click outside closes it.
    pub(crate) fn render_file_menu(
        &self,
        menu: &FileMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (project_id, target, is_dir) = (menu.project_id, menu.target, menu.is_dir);
        let path = SharedString::from(menu.path.clone());
        let root = menu.path.is_empty();
        let item = |id: &'static str, label: &'static str| {
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
        };
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        // In the folder clicked, or next to the file clicked.
        let dir = if is_dir {
            menu.path.as_str()
        } else {
            menu.path.rsplit_once('/').map_or("", |(dir, _)| dir)
        };
        let new_entry = |id: &'static str, label: &'static str, folder: bool| {
            let entry = NewEntry {
                project_id,
                target,
                dir: dir.to_owned(),
                folder,
            };
            item(id, label).on_click(cx.listener(move |shell, _, window, cx| {
                shell.start_new_entry(entry.clone(), window, cx);
            }))
        };
        let (copy_path, ignore_path, explorer_path, zed_path) =
            (path.clone(), path.clone(), path.clone(), path.clone());
        let panel = menu_panel("file-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.file_menu = None;
                cx.notify();
            }))
            .child(new_entry("file-menu-new-file", "New file…", false))
            .child(new_entry("file-menu-new-folder", "New folder…", true))
            .child(separator())
            .child(
                item("file-menu-copy-path", "Copy path").on_click(cx.listener(
                    move |shell, _, _, cx| {
                        shell.copy_file_path(project_id, target, &copy_path, cx);
                    },
                )),
            )
            // The folder itself cannot ignore itself.
            .when(!root, |panel| {
                panel.child(
                    item("file-menu-gitignore", "Add to .gitignore").on_click(cx.listener(
                        move |shell, _, window, cx| {
                            shell.ignore_file_path(
                                project_id,
                                target,
                                &ignore_path,
                                is_dir,
                                window,
                                cx,
                            );
                        },
                    )),
                )
            })
            .child(separator())
            .child(
                item("file-menu-explorer", "Open in File Explorer").on_click(cx.listener(
                    move |shell, _, _, cx| {
                        shell.open_path_in_explorer(project_id, target, &explorer_path, is_dir, cx);
                    },
                )),
            )
            .child(item("file-menu-zed", "Open in Zed").on_click(cx.listener(
                move |shell, _, _, cx| {
                    shell.open_path_in_zed(project_id, target, &zed_path, cx);
                },
            )));
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }

    /// The tab menu (right-click a tab, or the tab bar's ⋯): Pin, Split right / down, Rename,
    /// Close; on the active tab also the focused pane's Rename pane, Close pane and, for an
    /// agent, Follow in Zed (✓ while on) and Turns…. Floats at the click; a click outside closes it.
    pub(crate) fn render_tab_menu(
        &self,
        menu: &TabMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let tab_id = menu.tab_id;
        let pinned = (self.tabs.values())
            .find_map(|tabs| tabs.tab(tab_id))
            .is_some_and(|tab| tab.pinned);
        let active_tab = (self.workspace)
            .and_then(|project_id| self.tabs.get(&project_id))
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id);
        let pane = self
            .focused_session()
            .filter(|_| active_tab == Some(tab_id))
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let item = |id: &'static str, label: &'static str, command: Command| {
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
                .child(
                    div()
                        .text_size(font_size::META)
                        .child(self.keymap.label(command)),
                )
        };
        let panel = menu_panel("tab-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.tab_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("pin-tab".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child(if pinned {
                        "Unpin"
                    } else {
                        "Pin"
                    }))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_tab(tab_id, !pinned, window, cx);
                    })),
            )
            .child(separator())
            .child(
                item("tab-menu-split-right", "Split right", Command::SplitRight).on_click(
                    cx.listener(|shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.run(&Run(Command::SplitRight), window, cx);
                    }),
                ),
            )
            .child(
                item("tab-menu-split-down", "Split down", Command::SplitDown).on_click(
                    cx.listener(|shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.run(&Run(Command::SplitDown), window, cx);
                    }),
                ),
            )
            .child(
                item("tab-menu-rename", "Rename", Command::RenameTab).on_click(cx.listener(
                    |shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.start_rename(window, cx);
                    },
                )),
            )
            .child(
                item("tab-menu-close", "Close", Command::CloseTab).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.close_tab(tab_id, window, cx);
                    },
                )),
            )
            .when_some(pane, |panel, info| {
                let session = info.id;
                let agent = info.kind == SessionKind::Agent;
                let renamable = mkrew_core::run::command_name(info).is_none();
                let following = self.follow.is_on(session);
                panel
                    .child(separator())
                    .when(renamable, |panel| {
                        panel.child(
                            item("tab-menu-rename-pane", "Rename pane", Command::RenamePane)
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.tab_menu = None;
                                    shell.start_pane_rename(session, window, cx);
                                })),
                        )
                    })
                    .child(
                        item("tab-menu-close-pane", "Close pane", Command::ClosePane).on_click(
                            cx.listener(|shell, _, window, cx| {
                                shell.tab_menu = None;
                                shell.close_focused_pane(window, cx);
                            }),
                        ),
                    )
                    .when(agent, |panel| {
                        panel.child(
                            menu_item("tab-menu-follow-zed".into(), theme)
                                .child(
                                    div()
                                        .flex_1()
                                        .text_color(rgb(theme.text))
                                        .child("Follow in Zed"),
                                )
                                .child(div().text_color(rgb(theme.text)).child(if following {
                                    "✓"
                                } else {
                                    ""
                                }))
                                .on_click(cx.listener(move |shell, _, _, cx| {
                                    shell.tab_menu = None;
                                    shell.toggle_follow(session, cx);
                                })),
                        )
                    })
                    .when(agent && self.features.turns, |panel| {
                        panel.child(
                            menu_item("tab-menu-turns".into(), theme)
                                .child(div().flex_1().text_color(rgb(theme.text)).child("Turns…"))
                                .tooltip(tooltip(
                                    theme,
                                    "The agent's turns: restore the files to before one",
                                    "",
                                ))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.tab_menu = None;
                                    shell.open_turns(session, window, cx);
                                })),
                        )
                    })
            });
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }

    /// The split button's menu (SPEC §10): Split right, Split down, then the layouts, each
    /// drawn small with its shortcut. A layout holding fewer panes than the tab has is greyed.
    /// Floats at the click; a click outside closes it.
    pub(crate) fn render_layout_menu(
        &self,
        menu: &LayoutMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let panes = (self.tabs.values())
            .find_map(|tabs| tabs.tab(menu.tab_id))
            .map_or(0, |tab| tab.sessions().len());
        let shortcut = |command: Command| {
            div()
                .text_size(font_size::META)
                .child(self.keymap.label(command))
        };
        let pick = |command: Command| {
            cx.listener(move |shell: &mut Self, _: &gpui::ClickEvent, window, cx| {
                shell.layout_menu = None;
                shell.run(&Run(command), window, cx);
            })
        };
        let split = |id: &'static str, label: &'static str, command: Command| {
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
                .child(shortcut(command))
                .on_click(pick(command))
        };
        let mut panel = menu_panel("layout-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.layout_menu = None;
                cx.notify();
            }))
            .child(split(
                "layout-menu-split-right",
                "Split right",
                Command::SplitRight,
            ))
            .child(split(
                "layout-menu-split-down",
                "Split down",
                Command::SplitDown,
            ))
            .child(split(
                "layout-menu-equalize",
                "Equalize panes",
                Command::EqualizePanes,
            ))
            .child(
                div()
                    .h(px(theme::BORDER_WIDTH))
                    .my(px(space::XS))
                    .bg(rgb(theme.border)),
            )
            .child(
                div()
                    .px(px(space::S))
                    .py(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("Layouts"),
            );
        for preset in Preset::ALL {
            let command = Command::Layout(preset);
            let id = SharedString::from(format!("layout-menu-{}", preset.name()));
            let row = if panes <= preset.panes() {
                menu_item(id, theme).on_click(pick(command))
            } else {
                // The same box as a menu item, without its hover: it cannot be picked.
                let why = format!("This tab has {panes} panes; close some first");
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .opacity(opacity::DISABLED)
                    .tooltip(tooltip(theme, &why, ""))
            };
            panel = panel.child(
                row.child(layout_thumb(preset, theme))
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child(preset.label()),
                    )
                    .child(shortcut(command)),
            );
        }
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }

    /// The pane menu: Pin, Rename (inline, like F2; greyed for a run pane, which is named by its
    /// command) and the color swatches of the workspace palette. Floats at the click.
    pub(crate) fn render_pane_menu(
        &self,
        menu: &PaneMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let session_id = menu.session_id;
        let session = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session_id)?;
        let renamable = mkrew_core::run::command_name(session).is_none();
        let current = session.color.as_deref();
        let pinned = session.pinned;
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let panel = menu_panel("pane-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.pane_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("pin-pane".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child(if pinned {
                        "Unpin"
                    } else {
                        "Pin"
                    }))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_session(session_id, !pinned, window, cx);
                    })),
            )
            .child(separator())
            .child(
                menu_item("pane-menu-rename".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child("Rename"))
                    .child(
                        div()
                            .text_size(font_size::META)
                            .child(self.keymap.label(Command::RenamePane)),
                    )
                    .when(renamable, |el| {
                        el.on_click(cx.listener(move |shell, _, window, cx| {
                            shell.pane_menu = None;
                            shell.start_pane_rename(session_id, window, cx);
                        }))
                    })
                    .when(!renamable, |el| el.opacity(opacity::DISABLED)),
            )
            .child(separator())
            .child(
                div()
                    .px(px(space::S))
                    .pt(px(space::XS))
                    .text_size(font_size::META)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child("Change color"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::S))
                    .p(px(space::S))
                    .children(
                        std::iter::once(None)
                            .chain(WORKSPACE_COLORS.iter().map(|name| Some(*name)))
                            .map(|color| {
                                color_swatch(
                                    "pane-color",
                                    color,
                                    current,
                                    theme,
                                    cx,
                                    move |shell, color, window, cx| {
                                        shell.set_pane_color(session_id, color, window, cx);
                                    },
                                )
                            }),
                    ),
            );
        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(space::S))
                    .child(panel),
            )
            .into_any_element(),
        )
    }

    /// The Share menu of an agent pane: "Follow in Zed" (✓ while on). Floats at the click.
    pub(crate) fn render_share_menu(
        &self,
        menu: &PaneMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session_id = menu.session_id;
        let following = self.follow.is_on(session_id);
        let panel = menu_panel("share-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.share_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("share-follow-zed".into(), theme)
                    .child(
                        div()
                            .flex_none()
                            .w(px(space::M))
                            .text_color(rgb(theme.accent_text))
                            .child(if following { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Follow in Zed"),
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.share_menu = None;
                        shell.toggle_follow(session_id, cx);
                    })),
            );
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }

    /// `row` with the workspace menu dropping down below it while the menu is open for
    /// `project_id` (both sidebar widths).
    pub(crate) fn with_workspace_menu(
        &self,
        row: gpui::AnyElement,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        match self
            .workspace_menu
            .as_ref()
            .filter(|menu| menu.project_id == project_id)
        {
            Some(menu) => div()
                .child(row)
                .child(self.render_workspace_menu(menu, theme, cx))
                .into_any_element(),
            None => row,
        }
    }

    /// A left click on a workspace row: the first shows the workspace; clicking the shown one
    /// expands/collapses it.
    pub(crate) fn click_workspace_row(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace == Some(project_id) {
            self.sidebar.toggle(project_id);
            cx.notify();
        } else {
            self.activate_workspace(project_id, window, cx);
        }
    }
}

/// The rows of a profile list (agents or shells) in a menu (`current` checked): loading, the
/// error, an empty hint or one item per profile; a click calls `pick` with the profile id.
pub(crate) fn agent_rows(
    agents: &Agents,
    kind: SessionKind,
    current: Option<&str>,
    id_prefix: &str,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
    pick: impl Fn(&mut Shell, String, &mut Window, &mut Context<'_, Shell>) + Clone + 'static,
) -> Vec<gpui::AnyElement> {
    let muted_line = |text: String| {
        div()
            .px(px(space::S))
            .py(px(space::XS))
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .child(text)
    };
    match agents {
        Agents::Loading => vec![muted_line("Loading agents…".into()).into_any_element()],
        Agents::Failed(error) => {
            vec![
                div()
                    .flex()
                    .pl(px(space::S))
                    .child(
                        div()
                            .py(px(space::XS))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.status_exited))
                            .child("✕"),
                    )
                    .child(muted_line(format!("Cannot list the agents: {error}")).pl(px(space::XS)))
                    .into_any_element(),
            ]
        }
        Agents::Loaded(profiles) => {
            let choices = workspace_menu::agent_choices(profiles, kind, current);
            if choices.is_empty() {
                let empty = match kind {
                    SessionKind::Agent => "No agent profiles. Add one to the profiles folder.",
                    SessionKind::Shell => {
                        "No shell found on PATH. Add a shell profile to the profiles folder."
                    }
                };
                vec![muted_line(empty.into()).into_any_element()]
            } else {
                choices
                    .into_iter()
                    .map(|choice| {
                        let id = SharedString::from(format!("{id_prefix}-{}", choice.id));
                        let agent = choice.id;
                        let pick = pick.clone();
                        menu_item(id, theme)
                            .child(
                                div()
                                    .w(px(space::M))
                                    .text_color(rgb(theme.accent))
                                    .child(if choice.selected { "✓" } else { "" }),
                            )
                            .when(!agent.is_empty(), |el| {
                                el.child(agent_chip(
                                    sidebar::agent_badge(choice.icon.as_deref(), &agent),
                                    theme,
                                ))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .when(choice.selected, |el| {
                                        el.text_color(rgb(theme.text))
                                            .font_weight(FontWeight(weight::MEDIUM))
                                    })
                                    .child(choice.label),
                            )
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                pick(shell, agent.clone(), window, cx);
                            }))
                            .into_any_element()
                    })
                    .collect()
            }
        }
    }
}

/// A small drawing of `preset`'s panes for the layout menu.
fn layout_thumb(preset: Preset, theme: &Theme) -> gpui::Div {
    div()
        .relative()
        .flex_none()
        .w(px(theme::LAYOUT_THUMB_WIDTH))
        .h(px(theme::LAYOUT_THUMB_HEIGHT))
        .children(preset.rects().into_iter().map(|rect| {
            div()
                .absolute()
                .left(relative(rect.x as f32))
                .top(relative(rect.y as f32))
                .w(relative(rect.w as f32))
                .h(relative(rect.h as f32))
                .p(px(theme::BORDER_WIDTH))
                .child(
                    div()
                        .size_full()
                        .rounded(px(radius::SMALL))
                        .bg(rgb(theme.subtle)),
                )
        }))
}
