//! The session panel (inspector) and its cards.

use crate::*;

impl Shell {
    /// `ToggleInspector` / the status-bar button: opens or closes the session panel; from
    /// another view it shows Terminals with the panel open (the panel lives there only).
    pub(crate) fn toggle_inspector(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.workspace.is_none() {
            return;
        }
        if self.view() != View::Terminals {
            self.inspector_open = true;
            self.show_view(View::Terminals, window, cx);
        } else {
            self.inspector_open = !self.inspector_open;
            if !self.inspector_open {
                // The grip is gone: no mouse-up will end a drag in progress.
                self.end_inspector_drag();
            }
        }
        cx.notify();
    }

    /// The edge was released: saves the dragged width as `[ui] inspector_width`.
    pub(crate) fn end_inspector_drag(&mut self) {
        if std::mem::take(&mut self.inspector_dragging) {
            self.save_width("inspector_width", self.inspector_width);
        }
    }

    /// Saves a dragged panel width as `[ui] <key>` (one write per drag; the file is hand-edited
    /// config). A file that does not parse is not written; the palette lists why (§20.0).
    pub(crate) fn save_width(&mut self, key: &str, width: f32) {
        // Clamped to its theme limits while dragged, so it fits a u16.
        let width = (width.round() as u16).to_string();
        if let Err(error) = mkrew_core::settings::write_ui_value(&self.settings_file, key, &width) {
            self.settings_error = Some(error.to_string());
        }
    }

    /// The right-end status-bar button: shows or hides the session panel.
    pub(crate) fn inspector_toggle(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let hover = theme.hover;
        div()
            .id("toggle-inspector")
            .flex_none()
            .flex()
            .items_center()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .when(self.inspector_open, |el| el.bg(rgb(theme.accent_bg)))
            .when(!self.inspector_open, |el| {
                el.hover(move |style| style.bg(rgb(hover)))
            })
            .tooltip(tooltip(
                theme,
                if self.inspector_open {
                    "Hide session panel"
                } else {
                    "Show session panel"
                },
                self.keymap.label(Command::ToggleInspector),
            ))
            .on_click(cx.listener(|shell, _, window, cx| shell.toggle_inspector(window, cx)))
            .child(icon_svg(
                Icon::PanelRight,
                theme::ICON_SIZE_SMALL,
                if self.inspector_open {
                    theme.accent_text
                } else {
                    theme.text_muted
                },
            ))
            .when(self.modal_open(), |el| el.relative().child(inert_layer()))
    }

    /// The focused agent's fields, in order: context meter, tokens, cache, model. Each is left
    /// out without data (never `0`): no usage entry yet, no window, no cache, no model.
    pub(crate) fn status_agent_fields(
        &self,
        session: &Session,
        theme: &Theme,
    ) -> Vec<gpui::AnyElement> {
        let text = |text: String| {
            div()
                .flex_none()
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element()
        };
        let mut fields = Vec::new();
        if let Some(shown) = self.usages.get(session.id).map(usage::fields) {
            fields.extend(
                shown
                    .context
                    .map(|share| context_meter(&share, theme).into_any_element()),
            );
            fields.extend(shown.tokens.map(text));
            fields.extend(shown.cache.map(text));
            fields.extend(shown.model.map(text));
        }
        fields
    }

    /// The session panel (SPEC §10): the focused pane's cards, right of Terminals. Rendered
    /// only while open; it is a sibling of the panes, never inside the terminal element.
    pub(crate) fn render_inspector(
        &self,
        project_id: Ulid,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let grip = div()
            .id("inspector-grip")
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(px(theme::GRIP_WIDTH))
            .cursor(CursorStyle::ResizeLeftRight)
            .on_drag(InspectorGrip, |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(
                cx.listener(|shell, event: &DragMoveEvent<InspectorGrip>, window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                    // ponytail: live width, one `session.resize` per changed column count;
                    // throttle if an agent TUI flickers while dragging.
                    shell.inspector_width = inspector::width_from_pointer(
                        f32::from(window.viewport_size().width),
                        f32::from(event.event.position.x),
                    );
                    shell.inspector_dragging = true;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.end_inspector_drag()),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.end_inspector_drag()),
            );
        let cards = div()
            .id("inspector-body")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::M))
            .children(match session {
                Some(session) => vec![self.render_session_card(session, now, theme, cx)],
                None => vec![meta_line(theme, "No session focused".into()).into_any_element()],
            })
            // Follows the workspace, not the focused pane.
            .children(
                self.render_changes_card(project_id, theme, cx)
                    .map(IntoElement::into_any_element),
            )
            .children(
                session
                    .filter(|s| s.kind == SessionKind::Agent && s.state != SessionState::Exited)
                    .map(|s| self.render_plan_card(s, theme, cx)),
            );
        div()
            .flex_none()
            .w(px(self.inspector_width))
            .h_full()
            .relative()
            .border_l(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .bg(rgb(theme.bg))
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .child(cards)
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
        let lines = inspector::changes_card(self.git_summary(), workspace)?;
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
                .child(
                    quiet_button("plan-change", "Change…", theme, theme.hover).on_click(
                        cx.listener(move |shell, _, window, cx| shell.pick_plan(id, window, cx)),
                    ),
                )
                .child(
                    quiet_button(
                        "plan-unlink",
                        icon_text(Icon::Unlink, "Unlink", theme.text_muted),
                        theme,
                        theme.hover,
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
                    .child(agent_chip(self.agent_badge(&session.agent_profile), theme))
                    .child(div().min_w_0().truncate().child(card.agent.clone()))
                    .child(div().flex_1())
                    .child(status_mark(session.state, now, theme)),
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

    /// "Open transcript": the agent's `.jsonl` in Zed. A failure (Zed missing, file gone) shows
    /// in the sidebar like the other Zed actions.
    pub(crate) fn open_transcript(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        let Some(path) = self
            .usages
            .get(session)
            .and_then(|usage| usage.transcript.clone())
        else {
            return;
        };
        match zed::open(Path::new(&path), None) {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(%session, path, "open transcript failed: {error}");
                self.sidebar_error = Some(error.to_string());
            }
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
                    let _ = this.update_in(cx, |shell, window, cx| {
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
