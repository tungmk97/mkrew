//! Tab bar, tabs, rename field, pane frame and header.

use crate::*;

impl Shell {
    /// The tab strip: the tabs (agent chip, status dot, pin, title, ×) and `+`; on the right
    /// the notices, then for a single pane its status, Reading width and Share (a split shows
    /// them in the pane headers), the split / layout button and the ⋯ menu.
    pub(crate) fn render_tab_bar(
        &self,
        project_id: Ulid,
        tabs: &Tabs,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        // A single pane has no header: its status sits at the right end of this bar.
        let single = tabs
            .active()
            .map(Tab::rects)
            .filter(|rects| rects.len() == 1)
            .and_then(|rects| rects.first().map(|(session, _)| *session))
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let active = tabs.active().map(|tab| tab.id);
        let hover = theme.hover;
        div()
            .flex_none()
            .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .bg(rgb(theme.chrome))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .children(tabs.tabs().iter().map(|tab| {
                let tab_id = tab.id;
                let is_active = active == Some(tab_id);
                if let Some(renaming) = self
                    .renaming
                    .as_ref()
                    .filter(|r| r.target == RenameTarget::Tab(tab_id))
                {
                    return self.render_rename(&renaming.draft, theme, cx);
                }
                // The most urgent state among the tab's panes (SPEC §11), as one dot.
                let urgent = status::most_urgent(self.sidebar.sessions(), &tab.sessions());
                let dot = self.tab_dot(tab, urgent, theme);
                let group = SharedString::from(format!("tab-{tab_id}"));
                let icon = match tab_kind(tab, self.sidebar.sessions()) {
                    TabKind::Agent(profile) => Some(agent_chip(self.agent_badge(&profile), theme)),
                    TabKind::Shell => Some(agent_chip(">_".to_owned(), theme)),
                    TabKind::Empty => None,
                };
                div()
                    .id(group.clone())
                    .group(group.clone())
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .pl(px(space::S))
                    .pr(px(space::XS))
                    .py(px(space::XS))
                    .rounded(px(radius::MEDIUM))
                    // Same box as the rename field, so the bar does not shift while renaming.
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::transparent_black())
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .text_size(font_size::BODY)
                    .when(is_active, |el| {
                        el.bg(rgb(theme.surface_raised))
                            .border_color(rgb(theme.border))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                            // Accent line along the bottom, inset by the corner radius.
                            .child(
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .left(px(radius::MEDIUM))
                                    .right(px(radius::MEDIUM))
                                    .h(px(theme::PANE_ACCENT_WIDTH))
                                    .rounded_full()
                                    .bg(rgb(theme.accent)),
                            )
                    })
                    .when(!is_active, |el| {
                        el.text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .on_click(
                        cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                            if let Some(tabs) = shell.active_tabs_mut() {
                                tabs.activate(tab_id);
                            }
                            shell.sync_panes(window, cx);
                            if event.click_count() == 2 {
                                shell.start_rename(window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                            shell.open_tab_menu(tab_id, event.position, window, cx);
                        }),
                    )
                    .children(icon)
                    .when_some(urgent, |el, session| {
                        el.tooltip(status_tooltip(
                            theme,
                            session.state,
                            session.state_source,
                            session.state_changed_at,
                        ))
                    })
                    .children(dot.map(|(color, hollow, pulse)| {
                        status_dot(
                            SharedString::from(format!("tab-dot-{tab_id}")),
                            color,
                            hollow,
                            pulse,
                        )
                    }))
                    .when(tab.pinned, |el| el.child(pin_mark()))
                    .child(tab.title.clone())
                    .child(
                        row_action(
                            SharedString::from(format!("tab-close-{tab_id}")),
                            icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),
                            theme,
                        )
                        // Always on the active tab, on hover for the others.
                        .when(!is_active, |el| {
                            el.opacity(opacity::HIDDEN)
                                .group_hover(group, |style| style.opacity(opacity::FULL))
                        })
                        .tooltip(tooltip(
                            theme,
                            "Close tab",
                            if is_active {
                                self.keymap.label(Command::CloseTab)
                            } else {
                                String::new()
                            },
                        ))
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                // Never activate the tab being closed.
                                cx.stop_propagation();
                                shell.close_tab(tab_id, window, cx);
                            },
                        )),
                    )
                    .into_any_element()
            }))
            .child(
                icon_button("new-tab", Icon::Plus, theme)
                    .tooltip(tooltip(theme, "New tab", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.create_tab(project_id, None, window, cx);
                    })),
            )
            // Pushes the notices, the pane status and the split / ⋯ group to the right.
            .child(div().flex_1())
            .children(
                self.tab_error
                    .clone()
                    .map(|error| notice(theme.text, format!("✕ {error}"))),
            )
            .when(
                tabs.active().and_then(|tab| tab.zoomed()).is_some(),
                |bar| {
                    bar.child(notice(theme.text_muted, "⤢ Pane zoomed".to_owned()))
                        .child(
                            button(
                                "unzoom",
                                icon_text(
                                    Icon::Minimize2,
                                    "Show all panes",
                                    button_fg(theme, false),
                                ),
                                theme,
                                false,
                            )
                            .tooltip(tooltip(
                                theme,
                                "Show all panes",
                                self.keymap.label(Command::ZoomPane),
                            ))
                            .on_click(cx.listener(
                                |shell, _, window, cx| {
                                    shell.run(&Run(Command::ZoomPane), window, cx);
                                },
                            )),
                        )
                },
            )
            .children(
                lifecycle::running_warning(self.sidebar.sessions(), project_id)
                    .map(|warning| notice(theme.text_muted, format!("⚠ {warning}"))),
            )
            .when_some(single, |bar, info| {
                // F2 on a single pane: the rename field shows here (it has no header).
                let renaming = (self.renaming.as_ref())
                    .is_some_and(|r| r.target == RenameTarget::Session(info.id));
                bar.when(renaming, |bar| {
                    bar.child(div().child(self.pane_title(info, theme, cx)))
                })
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .text_size(font_size::META)
                        .children(self.pane_status(info, now, theme, cx))
                        .children(self.pane_controls(info, theme, cx)),
                )
            })
            .child(
                icon_button("tab-split", Icon::Columns2, theme)
                    .tooltip(tooltip(theme, "Split or pick a layout", ""))
                    .on_click(cx.listener(|shell, event: &gpui::ClickEvent, _, cx| {
                        shell.open_layout_menu(event.position(), cx);
                    })),
            )
            .when_some(active, |bar, tab_id| {
                bar.child(
                    icon_button("tab-menu-button", Icon::Ellipsis, theme)
                        .tooltip(tooltip(theme, "Tab and pane menu", ""))
                        .on_click(cx.listener(
                            move |shell, event: &gpui::ClickEvent, window, cx| {
                                shell.open_tab_menu(tab_id, event.position(), window, cx);
                            },
                        )),
                )
            })
    }

    /// The title being renamed inline: the draft with a caret, in the active tab's place.
    pub(crate) fn render_rename(
        &self,
        draft: &Input,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let selected = draft.all_selected();
        div()
            .id("tab-rename")
            .flex()
            .items_center()
            .min_w(px(theme::TAB_RENAME_MIN_WIDTH))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.pane_focus_border))
            .bg(rgb(theme.surface_raised))
            .whitespace_nowrap()
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.rename_focus)
            .on_key_down(cx.listener(Self::rename_key_down))
            .relative()
            .child(self.typing(&self.rename_focus, Self::rename_key_down, cx))
            .child(
                div()
                    .when(selected, |el| el.bg(rgb(theme.terminal_selection_bg)))
                    .child(draft.as_str().to_owned()),
            )
            .when(!selected, |el| {
                el.child(
                    div()
                        .flex_none()
                        .w(px(theme::BORDER_WIDTH))
                        .h(font_size::BODY)
                        .bg(rgb(theme.text)),
                )
            })
            .into_any_element()
    }

    /// The active tab's split tree: each pane placed at its fraction of the area.
    /// `composer_focused`: the focused pane's composer has the keyboard.
    pub(crate) fn render_tab(
        &self,
        tabs: &Tabs,
        now: i64,
        composer_focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        if tabs.active().and_then(|tab| tab.layout()).is_none() {
            return empty_state(
                theme,
                Some(">_"),
                Some("No panes in this tab"),
                "Start an agent or a shell, or pick a session from the sidebar.".into(),
                None,
            )
            .flex_1()
            .child(match self.starting_pane {
                Some(kind) => starting_button("empty-starting", kind, now, theme, true),
                None => div()
                    .flex()
                    .gap(px(space::S))
                    .child(
                        button(
                            "empty-new-agent",
                            icon_text(Icon::Bot, "New agent", button_fg(theme, true)),
                            theme,
                            true,
                        )
                        .child(kbd(self.keymap.label(Command::SplitAgent), theme, true))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.new_pane(SessionKind::Agent, Axis::Vertical, window, cx);
                        })),
                    )
                    .child(
                        secondary_button(
                            "empty-new-shell",
                            icon_text(Icon::SquareTerminal, "New shell", theme.text),
                            theme,
                        )
                        .child(kbd(self.keymap.label(Command::SplitShell), theme, false))
                        .tooltip(tooltip(theme, "New shell pane in the workspace folder", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.new_pane(SessionKind::Shell, Axis::Vertical, window, cx);
                        })),
                    )
                    .into_any_element(),
            });
        }
        let focused = tabs.active().and_then(|tab| tab.focused());
        let rects = tabs.active().map(Tab::rects).unwrap_or_default();
        let dividers = tabs.active().map(Tab::dividers).unwrap_or_default();
        let several = rects.len() > 1;
        let half_gap = px(theme::PANE_HALF_GAP);
        let panes: Vec<_> = rects
            .into_iter()
            .map(|(session, rect)| {
                let is_focused = focused == Some(session);
                div()
                    .absolute()
                    .left(relative(rect.x as f32))
                    .top(relative(rect.y as f32))
                    .w(relative(rect.w as f32))
                    .h(relative(rect.h as f32))
                    .p(half_gap)
                    .child(self.pane_frame(
                        session,
                        several,
                        is_focused,
                        is_focused && composer_focused,
                        now,
                        theme,
                        cx,
                    ))
            })
            .collect();
        let dividers: Vec<_> = dividers
            .into_iter()
            .map(|divider| pane_divider(divider, theme, cx))
            .collect();
        div().flex_1().min_h_0().p(half_gap).child(
            div()
                .id("tab-panes")
                .size_full()
                .relative()
                .children(panes)
                .children(dividers)
                // Every divider drag lands here: `bounds` is this area, the panes' 0..1 space.
                .on_drag_move(
                    cx.listener(|shell, event: &DragMoveEvent<Divider>, window, cx| {
                        let divider = *event.drag(cx);
                        cx.set_active_drag_cursor_style(divider_cursor(divider.axis), window);
                        let bounds = event.bounds;
                        let at = event.event.position - bounds.origin;
                        let x = f64::from(at.x / bounds.size.width);
                        let y = f64::from(at.y / bounds.size.height);
                        let ratio = divider.ratio_at(x, y);
                        // ponytail: live ratio, one `session.resize` per changed column count
                        // like the session panel's edge; throttle if a TUI flickers.
                        if shell
                            .active_tabs_mut()
                            .is_some_and(|tabs| tabs.drag_divider(divider.index, ratio))
                        {
                            shell.pane_dragging = true;
                            cx.notify();
                        }
                    }),
                ),
        )
    }

    /// One pane: the terminal (or why it is not shown; at reading width, centered, for an agent
    /// pane with it on), its composer, its border. Among several panes it gets a compact header
    /// and the unfocused ones a dim; a single pane has no header (the tab bar shows its status)
    /// and its color is a 2 px line on top.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pane_frame(
        &self,
        session: Ulid,
        several: bool,
        is_focused: bool,
        composer_focused: bool,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let content = match self.panes.get(&session) {
            Some(pane) => pane.clone().into_any_element(),
            None => {
                let (sentence, detail) = self.pane_placeholder(session);
                message(theme, None, sentence, detail)
                    .size_full()
                    .into_any_element()
            }
        };
        let info = self.sidebar.sessions().iter().find(|s| s.id == session);
        let border = status::pane_border(theme, info.map(|s| s.state), several, is_focused);
        let reading = info.is_some_and(|info| self.reading_on(info));
        let color = (info.and_then(|info| info.color.as_deref()))
            .and_then(|name| theme.workspace_color(name));
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, window, cx| {
                    if let Some(tabs) = shell.active_tabs_mut() {
                        tabs.focus(session);
                    }
                    shell.focus_pane(window, cx);
                    cx.notify();
                }),
            )
            .on_drag_move::<ExternalPaths>(cx.listener(
                move |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    if event.bounds.contains(&event.event.position) {
                        shell.drop_hover = Some((session, event.drag(cx).clone()));
                    } else if shell
                        .drop_hover
                        .as_ref()
                        .is_some_and(|(hovered, _)| *hovered == session)
                    {
                        shell.drop_hover = None;
                    }
                },
            ))
            .child(drop_catcher(session, cx))
            .when(several, |el| {
                el.children(info.map(|info| self.pane_header(info, color, now, theme, cx)))
            })
            .when(!several, |el| {
                el.when_some(color, |el, color| {
                    el.child(
                        div()
                            .flex_none()
                            .h(px(theme::PANE_ACCENT_WIDTH))
                            .rounded_t(px(radius::MEDIUM))
                            .bg(rgb(color)),
                    )
                })
            })
            // Reading width: the terminal element is narrower, so the PTY gets fewer columns
            // (the program wraps there); nothing reflows its output.
            .child(
                div().flex_1().min_h_0().flex().justify_center().child(
                    div()
                        .h_full()
                        .w_full()
                        .when(reading, |el| el.max_w(px(theme::READING_WIDTH)))
                        .child(content),
                ),
            )
            .children(self.composers.shown(session).map(|draft| {
                self.composer_panel(session, draft, is_focused, composer_focused, theme, cx)
            }))
            .when(several && !is_focused, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgb(theme.bg))
                        .opacity(theme.unfocused_pane_dim),
                )
            })
            // Drawn above the terminal so its corners stay rounded.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(border)),
            )
    }

    /// An agent pane at reading width: its own toggle, else `[ui] reading_width`. Shell and run
    /// panes are always full width.
    pub(crate) fn reading_on(&self, info: &Session) -> bool {
        info.kind == SessionKind::Agent
            && *self
                .reading_width
                .get(&info.id)
                .unwrap_or(&self.reading_default)
    }

    /// The status of `info`'s pane, each part only when it has something to say: the task
    /// folder, the status pill, the conflict warning, `⧗ N queued` sends and the context share.
    /// The tab bar shows them for a single pane, the compact header among several.
    pub(crate) fn pane_status(
        &self,
        info: &Session,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let session = info.id;
        let mut parts = Vec::new();
        if let Some(folder) = pane_text::folder(info.task_id.and_then(|id| self.board.task(id))) {
            parts.push(
                div()
                    .flex_none()
                    .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                    .truncate()
                    .text_color(rgb(theme.text_muted))
                    .child(folder)
                    .into_any_element(),
            );
        }
        let pill = pane_text::pill_text(
            info.state,
            info.exit_code,
            info.state_changed_at,
            now,
            self.turns.get(session),
        );
        parts.push(status_pill(info, pill, now, theme).into_any_element());
        let paths = self.conflicts.for_session(session);
        if !paths.is_empty() {
            parts.push(
                conflict_badge(
                    format!("pane-{session}"),
                    &paths,
                    theme,
                    move |shell| shell.conflicts.dismiss_session(session),
                    cx,
                )
                .flex_none()
                .max_w(relative(theme::PANE_CONFLICT_MAX_FRACTION))
                .into_any_element(),
            );
        }
        let queued = self.composers.queue(session).len();
        if queued > 0 {
            parts.push(
                div()
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("⧗ {queued} queued"))
                    .into_any_element(),
            );
        }
        if let Some(share) = self.usages.get(session).and_then(usage::context_share) {
            parts.push(
                div()
                    .id(SharedString::from(format!("pane-context-{session}")))
                    .flex_none()
                    .text_color(rgb(if share.warning {
                        theme.status_needs_input
                    } else {
                        theme.text_muted
                    }))
                    .tooltip(tooltip(theme, &share.hover, ""))
                    .child(if share.warning {
                        format!("⚠ {}", share.label)
                    } else {
                        share.label
                    })
                    .into_any_element(),
            );
        }
        parts
    }

    /// An agent pane's controls: the Reading width toggle and `Share` (its menu: Follow in Zed).
    /// Nothing for a shell or run pane.
    pub(crate) fn pane_controls(
        &self,
        info: &Session,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        if info.kind != SessionKind::Agent {
            return Vec::new();
        }
        let session = info.id;
        let reading = self.reading_on(info);
        let following = self.follow.is_on(session);
        let reading_toggle = icon_button_in(
            SharedString::from(format!("pane-reading-{session}")),
            Icon::BookOpen,
            if reading {
                theme.text
            } else {
                theme.text_muted
            },
            theme,
        )
        .when(reading, |el| el.bg(rgb(theme.surface_raised)))
        .tooltip(tooltip(
            theme,
            if reading {
                "Reading width: on (the agent wraps at about 130 columns). Click for full width"
            } else {
                "Reading width: off (full width). Click to wrap at about 130 columns"
            },
            "",
        ))
        .on_click(cx.listener(move |shell, _, _, cx| {
            let on = shell
                .sidebar
                .sessions()
                .iter()
                .find(|s| s.id == session)
                .is_some_and(|info| shell.reading_on(info));
            shell.reading_width.insert(session, !on);
            cx.notify();
        }));
        let share = button_frame(
            "pane-share",
            icon_text(
                Icon::Share2,
                "Share",
                if following {
                    theme.text
                } else {
                    theme.text_muted
                },
            ),
        )
        .flex_none()
        .px(px(space::XS))
        .py_0()
        .text_size(font_size::META)
        .active(|style| style.opacity(opacity::ACTIVE))
        .when(following, |el| {
            el.bg(rgb(theme.surface_raised))
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
        })
        .when(!following, |el| {
            let hover = theme.hover;
            el.text_color(rgb(theme.text_muted))
                .hover(move |style| style.bg(rgb(hover)))
        })
        .tooltip(tooltip(
            theme,
            if following {
                "Share: following this agent's edits in Zed"
            } else {
                "Share this agent's work"
            },
            "",
        ))
        .on_click(cx.listener(move |shell, event: &gpui::ClickEvent, _, cx| {
            shell.close_menus();
            shell.share_menu = Some(PaneMenu {
                session_id: session,
                position: event.position(),
            });
            cx.notify();
        }));
        vec![reading_toggle.into_any_element(), share.into_any_element()]
    }

    /// The title of `info`'s pane, or the inline rename field while it is renamed (F2 or a
    /// double-click, ADR 0031; a run pane is named by its command and is not renamed).
    pub(crate) fn pane_title(
        &self,
        info: &Session,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = info.id;
        let renaming = self
            .renaming
            .as_ref()
            .filter(|r| r.target == RenameTarget::Session(session));
        if let Some(renaming) = renaming {
            // Mouse downs stay in the field: the pane's handler would focus the terminal, and
            // the blur would commit half-typed text.
            return div()
                .flex_1()
                .min_w_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .child(self.render_rename(&renaming.draft, theme, cx))
                .into_any_element();
        }
        let renamable = mkrew_core::run::command_name(info).is_none();
        div()
            .id(SharedString::from(format!("pane-title-{session}")))
            .flex_1()
            .min_w_0()
            .truncate()
            .text_color(rgb(theme.text_muted))
            .children(run::pane_title(info, self.ports.get(info.id)))
            // Also the empty space after the title of an untitled pane.
            .when(renamable, |el| {
                el.on_click(
                    cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            shell.start_pane_rename(session, window, cx);
                        }
                    }),
                )
            })
            .into_any_element()
    }

    /// A pane's compact header among several panes (SPEC §11): the agent chip (tinted by the
    /// pane color, which also draws a line under the header), the title, the status parts and
    /// the agent controls. Right-click: the pane menu.
    pub(crate) fn pane_header(
        &self,
        info: &Session,
        color: Option<u32>,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = info.id;
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            // Transparent without a color, so the header keeps its height.
            .border_b(px(theme::PANE_ACCENT_WIDTH))
            .border_color(match color {
                Some(color) => Hsla::from(rgb(color)),
                None => gpui::transparent_black(),
            })
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::META)
            .child(
                agent_chip(self.agent_badge(&info.agent_profile), theme)
                    .when_some(color, |chip, color| {
                        chip.bg(theme.tint(color)).border_color(rgb(color))
                    }),
            )
            .child(self.pane_title(info, theme, cx))
            .children(self.pane_status(info, now, theme, cx))
            .children(self.pane_controls(info, theme, cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                    shell.open_pane_menu(session, event.position, window, cx);
                }),
            )
            .into_any_element()
    }

    /// A tab's status dot from its most urgent pane: amber needs input, red exited, a pulsing
    /// accent while an agent works, green for done or a live run process, else a hollow ring
    /// (idle, starting). `(color, hollow, pulse)`; `None` for an empty tab.
    pub(crate) fn tab_dot(
        &self,
        tab: &Tab,
        urgent: Option<&Session>,
        theme: &Theme,
    ) -> Option<(u32, bool, bool)> {
        let urgent = urgent?;
        let sessions = self.sidebar.sessions();
        let run_live = tab.sessions().iter().any(|id| {
            sessions.iter().any(|s| {
                s.id == *id
                    && s.state != SessionState::Exited
                    && mkrew_core::run::command_name(s).is_some()
            })
        });
        Some(match urgent.state {
            SessionState::NeedsInput => (theme.status_needs_input, false, false),
            SessionState::Exited => (theme.status_exited, false, false),
            SessionState::Working if urgent.kind == SessionKind::Agent => {
                (theme.status_working, false, true)
            }
            _ if run_live => (theme.status_done, false, false),
            SessionState::Done => (theme.status_done, false, false),
            _ => (theme.status_idle, true, false),
        })
    }

    /// Why a pane on screen has no terminal yet.
    pub(crate) fn pane_placeholder(&self, session: Ulid) -> (SharedString, Option<SharedString>) {
        // An exit explains a failed attach better than its error does.
        if let Some(s) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session && s.state == SessionState::Exited)
        {
            let text = match s.exit_code {
                Some(code) => format!("✕ Session exited (code {code})."),
                None => "✕ Session exited.".to_owned(),
            };
            return (text.into(), None);
        }
        match self.attach_errors.get(&session) {
            Some(error) => (
                "✕ Cannot attach the session.".into(),
                Some(error.clone().into()),
            ),
            None => ("Attaching…".into(), None),
        }
    }
}

/// The button of a pane being started: "Starting agent" with the `working` dots, no click.
pub(crate) fn starting_button(
    id: &'static str,
    kind: SessionKind,
    now: i64,
    theme: &Theme,
    primary: bool,
) -> gpui::AnyElement {
    let label = match kind {
        SessionKind::Agent => "Starting agent",
        SessionKind::Shell => "Starting shell",
    };
    let color = if primary {
        theme.on_accent
    } else {
        theme.text_muted
    };
    disabled_button(id, label, theme, primary)
        .opacity(opacity::FULL)
        .gap(px(space::S))
        .cursor(CursorStyle::OperationNotAllowed)
        .child(working_dots(now, rgb(color)))
        .into_any_element()
}

/// Fire-and-forget `session.detach`; a failure is logged.
pub(crate) fn detach(connection: &Connection, session_id: Ulid, cx: &mut Context<'_, Shell>) {
    let reply = connection.request(Request::SessionDetach(SessionRef { session_id }));
    cx.background_executor()
        .spawn(async move {
            match reply.await {
                Ok(Outcome::Result(_)) => {}
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%session_id, "session.detach failed: {}", body.message);
                }
                Err(error) => tracing::warn!(%session_id, "session.detach failed: {error:#}"),
            }
        })
        .detach();
}

/// Delivers an OS file drop on `session`'s pane (see `Shell::drop_hover`).
// ponytail: works around gpui-pre 0.3.7 keeping keyboard modality during OLE drags
// (window.rs:5415 skips FileDrop), which makes every hover-gated mouse-up listener (`on_drop`,
// `capture_any_mouse_up`) miss drops after typing. Use plain `.on_drop` on the pane when
// upstream treats FileDrop as mouse input.
pub(crate) fn drop_catcher(session: Ulid, cx: &Context<'_, Shell>) -> impl IntoElement + use<> {
    let view = cx.entity();
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let view = view.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                // A plain click has no active drag: stale paths are never typed.
                if phase != DispatchPhase::Capture || !cx.has_active_drag() {
                    return;
                }
                view.update(cx, |shell, cx| {
                    let hovered = shell.drop_hover.take_if(|(hovered, _)| *hovered == session);
                    if let Some((_, paths)) = hovered {
                        cx.stop_propagation();
                        shell.drop_paths(session, &paths, window, cx);
                    }
                });
            });
        },
    )
    .absolute()
    .size_full()
}

/// A pane's status pill (SPEC §11): status mark + `Working · 1h 25m` (with the last turn's
/// summary when done); hovering it shows the status source and last change.
pub(crate) fn status_pill(
    session: &Session,
    pill: String,
    now: i64,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let (state, changed_at) = (session.state, session.state_changed_at);
    div()
        .id(SharedString::from(format!("pane-status-{}", session.id)))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .px(px(space::S))
        .rounded_full()
        .whitespace_nowrap()
        .bg(rgb(theme.surface_raised))
        .text_color(rgb(theme.text_muted))
        .tooltip(status_tooltip(
            theme,
            state,
            session.state_source,
            changed_at,
        ))
        .child(status_mark(state, now, theme))
        .child(pill)
}

fn divider_cursor(axis: Axis) -> CursorStyle {
    match axis {
        Axis::Vertical => CursorStyle::ResizeLeftRight,
        Axis::Horizontal => CursorStyle::ResizeUpDown,
    }
}

/// The grab strip over a split's divider: centred on the gap between its two children, as long
/// as the split. Dragging it is handled by the panes area (`render_tab`), which knows the size.
fn pane_divider(divider: Divider, theme: &Theme, cx: &mut Context<'_, Shell>) -> gpui::AnyElement {
    let area = divider.area;
    let at = |start: f64, len: f64| relative((start + len * divider.ratio) as f32);
    let grip = px(theme::GRIP_WIDTH);
    let centre = px(-theme::GRIP_WIDTH / 2.0);
    let hover = theme.divider;
    // A zero-thick line along the divider; the strip hangs half on each side of it.
    let (line, strip) = match divider.axis {
        Axis::Vertical => (
            div()
                .left(at(area.x, area.w))
                .top(relative(area.y as f32))
                .h(relative(area.h as f32)),
            div().top_0().bottom_0().left(centre).w(grip),
        ),
        Axis::Horizontal => (
            div()
                .top(at(area.y, area.h))
                .left(relative(area.x as f32))
                .w(relative(area.w as f32)),
            div().left_0().right_0().top(centre).h(grip),
        ),
    };
    line.absolute()
        .child(
            strip
                .id(("pane-divider", divider.index))
                .absolute()
                .cursor(divider_cursor(divider.axis))
                .hover(move |style| style.bg(rgb(hover)))
                .on_drag(divider, |_, _, _, cx| cx.new(|_| EmptyView))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|shell, _, window, cx| shell.end_pane_drag(window, cx)),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|shell, _, window, cx| shell.end_pane_drag(window, cx)),
                ),
        )
        .into_any_element()
}
