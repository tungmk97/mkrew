//! Tab bar, tabs, rename field, pane frame and header.

use crate::*;

impl Shell {
    pub(crate) fn render_tab_bar(
        &self,
        project_id: Ulid,
        tabs: &Tabs,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let active = tabs.active().map(|tab| tab.id);
        let hover = theme.hover;
        div()
            .flex_none()
            .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .bg(rgb(theme.bg))
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
                // The most urgent state among the tab's panes (SPEC §11).
                let urgent = status::most_urgent(self.sidebar.sessions(), &tab.sessions());
                let group = SharedString::from(format!("tab-{tab_id}"));
                let icon = match tab_kind(tab, self.sidebar.sessions()) {
                    TabKind::Agent(profile) => Some(agent_chip(self.agent_badge(&profile), theme)),
                    TabKind::Shell => Some(agent_chip(">_".to_owned(), theme)),
                    TabKind::Empty => None,
                };
                div()
                    .id(group.clone())
                    .group(group.clone())
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
                        .child(status_mark(session.state, now, theme))
                    })
                    .when(tab.pinned, |el| el.child(pin_mark()))
                    .child(tab.title.clone())
                    .child(
                        row_action(
                            SharedString::from(format!("tab-close-{tab_id}")),
                            "✕",
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
            // Pushes the notices, Close pane and the + / split / ⋯ group to the right.
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
                            button("unzoom", "Show all panes", theme, false)
                                .tooltip(tooltip(
                                    theme,
                                    "Show all panes",
                                    self.keymap.label(Command::ZoomPane),
                                ))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.run(&Run(Command::ZoomPane), window, cx);
                                })),
                        )
                },
            )
            .children(
                lifecycle::running_warning(self.sidebar.sessions(), project_id)
                    .map(|warning| notice(theme.text_muted, format!("⚠ {warning}"))),
            )
            .when(self.focused_session().is_some(), |bar| {
                bar.child(
                    button("close-pane", "Close pane", theme, false)
                        .tooltip(tooltip(
                            theme,
                            "Close the focused pane",
                            self.keymap.label(Command::ClosePane),
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_focused_pane(window, cx);
                        })),
                )
            })
            .child(
                button("new-tab", "+", theme, false)
                    .tooltip(tooltip(theme, "New tab", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.create_tab(project_id, None, window, cx);
                    })),
            )
            .child(
                button("tab-split", "◫", theme, false)
                    .tooltip(tooltip(theme, "Split or pick a layout", ""))
                    .on_click(cx.listener(|shell, event: &gpui::ClickEvent, _, cx| {
                        shell.open_layout_menu(event.position(), cx);
                    })),
            )
            .when_some(active, |bar, tab_id| {
                bar.child(
                    button("tab-menu-button", "⋯", theme, false)
                        .tooltip(tooltip(theme, "Tab menu", ""))
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
                        button("empty-new-agent", "New agent", theme, true)
                            .child(kbd(self.keymap.label(Command::SplitAgent), theme, true))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.new_pane(SessionKind::Agent, Axis::Vertical, window, cx);
                            })),
                    )
                    .child(
                        secondary_button("empty-new-shell", "New shell", theme)
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

    /// One pane: the terminal (or why it is not shown), its composer, its border, and the
    /// unfocused dim.
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
            .children(info.map(|info| {
                let paths = self.conflicts.for_session(session);
                let conflict = (!paths.is_empty()).then(|| {
                    conflict_badge(
                        format!("pane-{session}"),
                        &paths,
                        theme,
                        move |shell| shell.conflicts.dismiss_session(session),
                        cx,
                    )
                });
                let queued = self.composers.queue(session).len();
                // Keyed by session: several panes share the nearest keyed parent.
                let following = self.follow.is_on(session);
                let turns = (info.kind == SessionKind::Agent).then(|| {
                    let follow_tip = if following {
                        "Following this agent's edits in Zed: click to stop"
                    } else {
                        "Open each file this agent edits in Zed"
                    };
                    div()
                        .id(SharedString::from(format!("pane-turns-{session}")))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(
                            button_frame(
                                "pane-follow",
                                if following {
                                    "Following in Zed"
                                } else {
                                    "Follow in Zed"
                                },
                            )
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
                                let hover = theme.surface_raised;
                                el.text_color(rgb(theme.text_muted))
                                    .hover(move |style| style.bg(rgb(hover)))
                            })
                            .tooltip(tooltip(theme, follow_tip, ""))
                            .on_click(cx.listener(
                                move |shell, _, _, cx| {
                                    shell.toggle_follow(session, cx);
                                },
                            )),
                        )
                        .when(self.features.turns, |el| {
                            el.child(
                                quiet_button("pane-turns", "Turns…", theme, theme.surface_raised)
                                    .px(px(space::XS))
                                    .py_0()
                                    .text_size(font_size::META)
                                    .tooltip(tooltip(
                                        theme,
                                        "The agent's turns: restore the files to before one",
                                        "",
                                    ))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.open_turns(session, window, cx);
                                    })),
                            )
                        })
                });
                let renaming = self
                    .renaming
                    .as_ref()
                    .filter(|r| r.target == RenameTarget::Session(session));
                let title = match renaming {
                    // Mouse downs stay in the field: the pane's handler would focus the
                    // terminal, and the blur would commit half-typed text.
                    Some(renaming) => div()
                        .flex_1()
                        .min_w_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| cx.stop_propagation()),
                        )
                        .child(self.render_rename(&renaming.draft, theme, cx))
                        .into_any_element(),
                    None => {
                        let renamable = mkrew_core::run::command_name(info).is_none();
                        div()
                            .id(SharedString::from(format!("pane-title-{session}")))
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text_muted))
                            .children(run::pane_title(info, self.ports.get(info.id)))
                            // Also the empty space after the agent name of an untitled pane.
                            .when(renamable, |el| {
                                el.on_click(cx.listener(
                                    move |shell, event: &gpui::ClickEvent, window, cx| {
                                        if event.click_count() == 2 {
                                            shell.start_pane_rename(session, window, cx);
                                        }
                                    },
                                ))
                            })
                            .into_any_element()
                    }
                };
                let workspace = (self.sidebar.projects().iter())
                    .find(|project| project.id == info.project_id)
                    .map(|project| project.name.as_str());
                let folder = pane_text::folder(info.task_id.and_then(|id| self.board.task(id)));
                let breadcrumb =
                    pane_text::breadcrumb(workspace, folder.as_deref(), &info.agent_profile);
                let pill = pane_text::pill_text(
                    info.state,
                    info.exit_code,
                    info.state_changed_at,
                    now,
                    self.turns.get(session),
                );
                let color = info
                    .color
                    .as_deref()
                    .and_then(|name| theme.workspace_color(name));
                pane_header(
                    info,
                    self.agent_badge(&info.agent_profile),
                    breadcrumb,
                    pill,
                    title,
                    self.usages.get(session).and_then(usage::context_share),
                    conflict,
                    turns,
                    queued,
                    color,
                    now,
                    theme,
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                        shell.open_pane_menu(session, event.position, window, cx);
                    }),
                )
            }))
            .child(div().flex_1().min_h_0().child(content))
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

/// A quiet button starting a new agent or shell pane; `keys` = its shortcut label. While a
/// pane of `starting` kind is starting, that kind's button shows the loading mark and the other
/// fades out (one start at a time).
#[allow(clippy::too_many_arguments)]
pub(crate) fn new_pane_button(
    id: &'static str,
    label: &'static str,
    kind: SessionKind,
    keys: String,
    starting: Option<SessionKind>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    match starting {
        Some(started) if started == kind => return starting_button(id, kind, now, theme, false),
        Some(_) => return disabled_button(id, label, theme, false).into_any_element(),
        None => {}
    }
    let tip = match kind {
        SessionKind::Agent => "New pane with the workspace's default agent",
        SessionKind::Shell => "New pane with the workspace's default shell",
    };
    button(id, label, theme, false)
        .tooltip(tooltip(theme, tip, keys))
        .on_click(cx.listener(move |shell, _, window, cx| {
            // The title bar shows it in every view; the new pane is seen in Terminals.
            shell.show_view(View::Terminals, window, cx);
            shell.new_pane(kind, Axis::Vertical, window, cx);
        }))
        .into_any_element()
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

/// The status bar's context meter: a thin track filled to the share, then `ctx 62%`. From
/// 80 % (`ContextShare::warning`) the fill and the text turn amber, as in the pane header.
pub(crate) fn context_meter(share: &ContextShare, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let color = if share.warning {
        theme.status_needs_input
    } else {
        theme.accent
    };
    div()
        .id("status-context")
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .text_color(rgb(if share.warning {
            theme.status_needs_input
        } else {
            theme.text_muted
        }))
        .tooltip(tooltip(theme, &share.hover, ""))
        .child(
            div()
                .w(theme::rems_px(theme::STATUS_BAR_METER_WIDTH))
                .h(px(theme::STATUS_BAR_METER_HEIGHT))
                .rounded_full()
                .bg(rgb(theme.border))
                .child(
                    div()
                        .h_full()
                        .w(relative(f32::from(share.fill_percent) / 100.0))
                        .rounded_full()
                        .bg(rgb(color)),
                ),
        )
        .child(share.label.clone())
}

/// A pane's header (SPEC §11): the agent badge, the `<workspace> › <folder> › <agent>`
/// breadcrumb, one status pill (status mark + `Working · 1h 25m`, with the last turn's summary
/// when done; hovering it shows the status source and last change), the title (renamed inline
/// with F2 or a double-click, ADR 0031), an optional conflict warning, the number of queued
/// sends (§20B.2), the context share (`⚠` and amber from 80 %, §20E.3), and on the right an
/// agent's "Follow in Zed" and "Turns…" (§20C.5, §20C.2). A colored pane (ADR 0037) tints its
/// chip and draws an accent line under the header.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pane_header(
    session: &Session,
    badge: String,
    breadcrumb: String,
    pill: String,
    title: gpui::AnyElement,
    context: Option<ContextShare>,
    conflict: Option<gpui::Stateful<gpui::Div>>,
    turns: Option<gpui::Stateful<gpui::Div>>,
    queued: usize,
    color: Option<u32>,
    now: i64,
    theme: &Theme,
) -> gpui::Div {
    let (state, changed_at) = (session.state, session.state_changed_at);
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
        .child(agent_chip(badge, theme).when_some(color, |chip, color| {
            chip.bg(theme.tint(color)).border_color(rgb(color))
        }))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(breadcrumb),
        )
        .child(
            div()
                .id(SharedString::from(format!("pane-status-{}", session.id)))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .px(px(space::S))
                .rounded_full()
                .bg(rgb(theme.surface_raised))
                .text_color(rgb(theme.text_muted))
                .tooltip(status_tooltip(
                    theme,
                    state,
                    session.state_source,
                    changed_at,
                ))
                .child(status_mark(state, now, theme))
                .child(pill),

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
        )
        .child(title)
        .children(conflict.map(|badge| {
            badge
                .flex_none()
                .max_w(relative(theme::PANE_CONFLICT_MAX_FRACTION))
        }))
        .when(queued > 0, |el| {
            el.child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("⧗ {queued} queued")),
            )
        })
        .children(context.map(|share| {
            div()
                .id(SharedString::from(format!("pane-context-{}", session.id)))
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
        }))
        .children(turns)
}
