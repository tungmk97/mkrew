//! Board widgets: columns, drop slots, task cards and their buttons, the drag preview.

use crate::*;

/// Shell state the cards of a board render with.
pub(crate) struct CardContext<'a> {
    pub(crate) selected: Option<Ulid>,
    /// Tasks whose run is starting.
    pub(crate) starting: &'a HashSet<Ulid>,
    pub(crate) follow_up: Option<&'a FollowUp>,
    pub(crate) follow_up_focus: &'a FocusHandle,
    pub(crate) follow_up_scroll: &'a ScrollHandle,
    /// Composing IME chars of the follow-up field (see [`Shell::typing`]).
    pub(crate) ime_marked: &'a Rc<ime::Composing<FocusHandle>>,
    /// See [`Shell::typing`].
    pub(crate) typing_target: &'a ime::FocusedField<Shell>,
    /// The follow-up field has the keyboard (shows its caret).
    pub(crate) follow_up_focused: bool,
    /// Shortcut label of Run on the selected card.
    pub(crate) run_keys: String,
    pub(crate) conflicts: &'a Conflicts,
    pub(crate) run_with: Option<&'a RunWith>,
    pub(crate) fan_out: Option<&'a FanOutPicker>,
    pub(crate) usages: &'a Usages,
    pub(crate) sessions: &'a [Session],
    /// Profile icons by id, for the agent chip (see [`Shell::agent_badge`]).
    pub(crate) agent_icons: &'a HashMap<String, String>,
}

/// A board column: its title and card count over the cards, or a quiet "No tasks". Every
/// card is a drop slot (land before it), and so is the free space under the last one.
pub(crate) fn board_column(
    column: board::Column,
    cards: &CardContext<'_>,
    now: i64,
    new_task: Option<gpui::AnyElement>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let count = column.cards.len();
    let status = column.status;
    div()
        .flex_1()
        .min_w(px(theme::BOARD_COLUMN_MIN_WIDTH))
        .flex()
        .flex_col()
        .gap(px(space::S))
        .child(
            div()
                .min_h(px(theme::BOARD_COLUMN_HEADER_HEIGHT))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::XS))
                .child(
                    div()
                        .text_size(font_size::BODY)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                        .child(board::column_title(column.status)),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(count.to_string()),
                )
                .children(new_task.map(|button| div().ml_auto().child(button))),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "board-column-{:?}",
                    column.status
                )))
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                // The slots' own vertical padding makes the `S` gap between cards.
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface))
                .overflow_y_scroll()
                .when(count == 0, |el| {
                    el.child(
                        div()
                            .p(px(space::S))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child("No tasks"),
                    )
                })
                .children(column.cards.into_iter().map(|card| {
                    let task_id = card.task_id;
                    drop_slot(status, Some(task_id), theme, cx)
                        .py(px(space::XS))
                        .child(task_card(card, cards, now, theme, cx))
                }))
                .child(
                    drop_slot(status, None, theme, cx)
                        .flex_1()
                        .min_h(px(space::XL)),
                ),
        )
        .into_any_element()
}

/// Where a dragged card lands in the `status` column: right before the card `before`, or at
/// the end. Hovering it with a card shows an accent line along its top edge (SPEC §14).
pub(crate) fn drop_slot(
    status: TaskStatus,
    before: Option<Ulid>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let name = match before {
        Some(task_id) => format!("drop-{task_id}"),
        None => format!("drop-end-{status:?}"),
    };
    let group = SharedString::from(name.clone());
    let accent = theme.accent;
    div()
        .id(SharedString::from(name))
        .group(group.clone())
        .relative()
        .flex_none()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                // Centred on the slot's top edge, i.e. in the gap above the card.
                .top(px(-theme::DROP_INDICATOR_HEIGHT / 2.0))
                .h(px(theme::DROP_INDICATOR_HEIGHT))
                .rounded(px(radius::SMALL))
                .group_drag_over::<DraggedCard>(group, move |style| style.bg(rgb(accent)))
                // No-op: gpui applies group drag styles only to elements with a hitbox, and a
                // drag style gives it one.
                .drag_over::<DraggedCard>(|style, _, _, _| style),
        )
        .on_drop(
            cx.listener(move |shell, dragged: &DraggedCard, window, cx| {
                shell.drop_card(dragged.task_id, status, before, window, cx);
            }),
        )
}

/// A task card: title (Edit on hover), agent, tokens used and run time, then its session's status
/// (SPEC §10, §11), then its column's action or a follow-up field. Dragging it carries a
/// [`DraggedCard`]; clicking it selects it and opens its session's terminal.
pub(crate) fn task_card(
    card: Card,
    cards: &CardContext<'_>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let run_time = card.run_time(now);
    // A group card runs no agent of its own (SPEC §20E.3).
    let tokens = card
        .children
        .is_empty()
        .then(|| cards.usages.task_total(card.task_id, cards.sessions))
        .flatten()
        .map(usage::format_tokens);
    let shows_error_row = card.shows_error_row();
    let action = card.action();
    let can_undo = card.can_undo();
    let can_view_changes = card.can_view_changes();
    let manages_worktree = card.manages_worktree();
    let can_compare = card.can_compare();
    let can_run_with = card.can_run_with() && !cards.starting.contains(&card.task_id);
    let can_fan_out = card.can_fan_out() && !cards.starting.contains(&card.task_id);
    let current_agent = card.agent_profile().map(str::to_owned);
    let opens = card.opens_session();
    let follow_up_session = card.follow_up_session();
    let task_id = card.task_id;
    let conflict_paths = cards.conflicts.for_task(task_id);
    let selected = cards.selected == Some(task_id);
    let group = SharedString::from(format!("card-{task_id}"));
    let edit_hover = theme.border;
    let preview_style = PopupStyle::of(theme);
    div()
        .id(group.clone())
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.card_clicked(task_id, opens, window, cx);
        }))
        .on_drag(
            DraggedCard {
                task_id,
                title: card.title.clone(),
            },
            move |dragged, _, _, cx| {
                let style = preview_style.clone();
                let title = dragged.title.clone().into();
                cx.new(|_| CardPreview { style, title })
            },
        )
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.surface_raised))
        // Always bordered, so selecting a card does not shift its content.
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if selected {
            theme.pane_focus_border
        } else {
            theme.surface_raised
        }))
        .when(!selected, |el| {
            el.hover(move |style| style.border_color(rgb(edit_hover)))
        })
        .cursor_pointer()
        .group(group.clone())
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(space::S))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(font_size::CARD_TITLE)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                        .child(card.title),
                )
                // Shown on hover (SPEC §14 progressive disclosure).
                .child(
                    div()
                        .id(SharedString::from(format!("card-edit-{task_id}")))
                        .flex_none()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                        // One step above the card's own `surface_raised`.
                        .hover(move |style| style.bg(rgb(edit_hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(theme, "Edit or delete this task", ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            // Not a click on the card (which would open its terminal).
                            cx.stop_propagation();
                            if let Some(task) = shell.board.task(task_id).cloned() {
                                shell.open_task_dialog(TaskDialog::edit(task), window, cx);
                            }
                        }))
                        .child("Edit"),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .children(current_agent.as_deref().map(|agent| {
                    agent_chip(
                        sidebar::agent_badge(
                            cards.agent_icons.get(agent).map(String::as_str),
                            agent,
                        ),
                        theme,
                    )
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(card.agent),
                )
                .children(tokens.map(|tokens| {
                    div()
                        .id(SharedString::from(format!("card-tokens-{task_id}")))
                        .flex_none()
                        .tooltip(tooltip(
                            theme,
                            "Tokens used by this task's agent (input + output)",
                            "",
                        ))
                        .child(tokens)
                }))
                .children(run_time.map(|time| div().flex_none().child(time))),
        )
        .children(card.branch.map(|branch| {
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(branch)
        }))
        // A group card: one line per child (SPEC §20E.2).
        .children(
            card.children
                .into_iter()
                .map(|child| fan_out_child_row(child, now, theme)),
        )
        .when_some(card.session, |el, session| {
            el.child(
                div()
                    .id(SharedString::from(format!("card-status-{task_id}")))
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .tooltip(status_tooltip(
                        theme,
                        session.state,
                        session.state_source,
                        session.state_changed_at,
                    ))
                    .child(status_mark(session.state, now, theme))
                    .child(status::text(
                        session.state,
                        session.exit_code,
                        session.state_changed_at,
                        now,
                    )),
            )
        })
        .when(!conflict_paths.is_empty(), |el| {
            el.child(conflict_badge(
                format!("card-{task_id}"),
                &conflict_paths,
                theme,
                move |shell| shell.conflicts.dismiss_task(task_id),
                cx,
            ))
        })
        .when(shows_error_row, |el| {
            let exited = sidebar::status_display(SessionState::Exited);
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(
                        div()
                            .text_color(rgb(theme.status_exited))
                            .child(exited.icon),
                    )
                    .child("Last run failed"),
            )
        })
        .when_some(follow_up_session, |el, session_id| {
            el.child(follow_up_field(task_id, session_id, cards, theme, cx))
        })
        .when(
            action.is_some() || can_undo || manages_worktree || can_compare,
            |el| {
                // Secondary: shown on hover, left of the column's action (SPEC §14).
                let undo = can_undo.then(|| {
                    quiet_button("card-undo", "Undo", theme, edit_hover)
                        .opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                        .tooltip(tooltip(
                            theme,
                            "Undo task: review and discard its changes",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.selected_card = Some(task_id);
                            shell.open_undo_dialog(task_id, window, cx);
                        }))
                });
                let changes = can_view_changes.then(|| {
                    quiet_button("card-changes", "Changes", theme, edit_hover)
                        .opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                        .tooltip(tooltip(
                            theme,
                            "View changes: the task's diff since its run started",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.selected_card = Some(task_id);
                            shell.view_task_changes(task_id, window, cx);
                        }))
                });
                // The worktree's two actions (SPEC §20E.1), hover-shown like Undo.
                let worktree_actions = manages_worktree.then(|| {
                    [
                        (
                            "card-merge",
                            "Merge back…",
                            "Merge back: merge the task's branch into your checkout",
                            worktree_dialog::Kind::Merge,
                        ),
                        (
                            "card-remove-worktree",
                            "Remove worktree…",
                            "Remove worktree: delete its folder (an unmerged branch is kept)",
                            worktree_dialog::Kind::Remove,
                        ),
                    ]
                    .map(|(id, text, tip, kind)| {
                        quiet_button(id, text, theme, edit_hover)
                            .opacity(opacity::HIDDEN)
                            .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                            .tooltip(tooltip(theme, tip, ""))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                cx.stop_propagation();
                                shell.selected_card = Some(task_id);
                                shell.open_worktree_dialog(kind, task_id, window, cx);
                            }))
                    })
                });
                let run_with = can_run_with.then(|| {
                    run_with_button(
                        task_id,
                        current_agent.as_deref(),
                        group.clone(),
                        cards,
                        theme,
                        cx,
                    )
                });
                let fan_out =
                    can_fan_out.then(|| fan_out_button(task_id, group.clone(), cards, theme, cx));
                // Every child finished: the group card's next step (SPEC §20E.2), always shown.
                let compare = can_compare.then(|| {
                    // Accent only on the selected card, like Run.
                    let compare = if selected {
                        button("card-compare", "Compare", theme, true)
                    } else {
                        quiet_button("card-compare", "Compare", theme, edit_hover)
                    };
                    compare
                        .tooltip(tooltip(
                            theme,
                            "Compare the agents' results and pick one to merge back",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.selected_card = Some(task_id);
                            shell.open_compare(task_id, window, cx);
                        }))
                });
                el.child(
                    div()
                        .flex()
                        // A Review card can show seven buttons; a narrow column wraps them.
                        .flex_wrap()
                        .justify_end()
                        .gap(px(space::S))
                        .children(run_with)
                        .children(fan_out)
                        .children(changes)
                        .children(worktree_actions.into_iter().flatten())
                        .children(undo)
                        .children(compare)
                        .children(action.map(|action| {
                            card_action(action, task_id, selected, cards, theme, cx)
                        })),
                )
            },
        )
        .into_any_element()
}

/// "Run with…" on a Todo / Review card (SPEC §20B.6): shown on hover (always while its picker
/// is open); the picker of agent profiles drops down from below it, a click outside closes it.
pub(crate) fn run_with_button(
    task_id: Ulid,
    current: Option<&str>,
    group: SharedString,
    cards: &CardContext<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Div {
    let menu = cards.run_with.filter(|menu| menu.task_id == task_id);
    // One step above the card's own `surface_raised`, like its other quiet buttons.
    let button = quiet_button("card-run-with", "Run with…", theme, theme.border)
        .when(menu.is_none(), |el| {
            el.opacity(opacity::HIDDEN)
                .group_hover(group, |style| style.opacity(opacity::FULL))
        })
        .tooltip(tooltip(
            theme,
            "Run with another agent: changes the task's agent, then runs it",
            "",
        ))
        // An open picker already closed on this mouse down (outside it): the click reopens it.
        .on_click(cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            shell.open_run_with(task_id, window, cx);
        }));
    let Some(menu) = menu else {
        return div().child(button);
    };
    let agents = agent_rows(
        &menu.agents,
        SessionKind::Agent,
        current,
        "run-with",
        theme,
        cx,
        move |shell, agent, window, cx| {
            // Not a click on the card under the menu (which would open its terminal).
            cx.stop_propagation();
            shell.run_task_with(task_id, agent, window, cx);
        },
    );
    let panel = menu_panel("run-with-menu", theme)
        .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
            shell.run_with = None;
            cx.notify();
        }))
        .child(
            div()
                .px(px(space::S))
                .pt(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child("Run with"),
        )
        .children(agents);
    div().child(button).child(deferred(
        anchored()
            .snap_to_window_with_margin(px(space::S))
            .child(div().pt(px(space::XS)).child(panel)),
    ))
}

/// "Several agents…" on a Todo / Review card (SPEC §20E.2), next to "Run with…" and shown the
/// same way; its picker ticks 2–5 agent profiles, then runs them, each in its own worktree.
pub(crate) fn fan_out_button(
    task_id: Ulid,
    group: SharedString,
    cards: &CardContext<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Div {
    let menu = cards.fan_out.filter(|menu| menu.task_id == task_id);
    let button = quiet_button("card-fan-out", "Several agents…", theme, theme.border)
        .when(menu.is_none(), |el| {
            el.opacity(opacity::HIDDEN)
                .group_hover(group, |style| style.opacity(opacity::FULL))
        })
        .tooltip(tooltip(
            theme,
            "Run with several agents: each in its own worktree, to compare (git repos only)",
            "",
        ))
        .on_click(cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            shell.open_fan_out(task_id, window, cx);
        }));
    let Some(menu) = menu else {
        return div().child(button);
    };
    let choices = match &menu.agents {
        Agents::Loaded(profiles) => {
            workspace_menu::agent_choices(profiles, SessionKind::Agent, None)
        }
        _ => Vec::new(),
    };
    let rows = if choices.is_empty() {
        // Loading, the error or the empty hint; there is nothing to tick.
        agent_rows(
            &menu.agents,
            SessionKind::Agent,
            None,
            "fan-out",
            theme,
            cx,
            |_, _, _, _| {},
        )
    } else if choices.len() < TaskFanoutParams::MIN_PROFILES {
        vec![
            div()
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child("Needs at least 2 agent profiles. Add one to the profiles folder.")
                .into_any_element(),
        ]
    } else {
        choices
            .into_iter()
            .map(|choice| {
                let picked = menu.is_picked(&choice.id);
                let blocked = !picked && menu.is_full();
                let id = SharedString::from(format!("fan-out-{}", choice.id));
                let agent = choice.id;
                menu_item(id, theme)
                    .child(
                        div()
                            .w(px(space::M))
                            .text_color(rgb(theme.accent))
                            .child(if picked { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .when(picked, |el| {
                                el.text_color(rgb(theme.text))
                                    .font_weight(FontWeight(weight::MEDIUM))
                            })
                            .child(choice.label),
                    )
                    .when(blocked, |el| el.opacity(opacity::DISABLED).cursor_default())
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        // Not a click on the card under the menu (which would open its terminal).
                        cx.stop_propagation();
                        if let Some(menu) = shell.fan_out.as_mut() {
                            menu.toggle(&agent);
                            cx.notify();
                        }
                    }))
                    .into_any_element()
            })
            .collect()
    };
    let label = menu.button_label();
    let run = div()
        .id("fan-out-run")
        .flex()
        .items_center()
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .text_size(font_size::BODY)
        .bg(rgb(theme.accent))
        .text_color(rgb(theme.on_accent))
        .child(label);
    let run = if menu.request().is_some() {
        run.cursor_pointer()
            .hover(|style| style.opacity(opacity::HOVER))
            .active(|style| style.opacity(opacity::ACTIVE))
            .on_click(cx.listener(|shell, _, window, cx| {
                cx.stop_propagation();
                shell.run_fan_out(window, cx);
            }))
    } else {
        run.opacity(opacity::DISABLED)
    };
    let panel = menu_panel("fan-out-menu", theme)
        .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
            shell.fan_out = None;
            cx.notify();
        }))
        .child(
            div()
                .px(px(space::S))
                .pt(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child("Run with several agents"),
        )
        .children(rows)
        .child(
            div()
                .flex()
                .justify_end()
                .px(px(space::S))
                .pt(px(space::S))
                .pb(px(space::XS))
                .child(run),
        );
    div().child(button).child(deferred(
        anchored()
            .snap_to_window_with_margin(px(space::S))
            .child(div().pt(px(space::XS)).child(panel)),
    ))
}

/// One child on a group card (SPEC §20E.2): its status icon, agent and state.
pub(crate) fn fan_out_child_row(child: fanout::ChildLine, now: i64, theme: &Theme) -> gpui::Div {
    let state = child.state_text(now);
    let icon = child
        .session
        .as_ref()
        .map(|session| status_mark(session.state, now, theme));
    div()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .child(
            div()
                .flex_none()
                .w(theme::rems_px(theme::STATUS_MARK_WIDTH))
                .children(icon),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(theme.text))
                .child(child.agent),
        )
        .child(div().flex_none().child(state))
}

/// The button of a card's column (Run / Approve); Run is disabled while its run starts.
pub(crate) fn card_action(
    action: CardAction,
    task_id: Ulid,
    selected: bool,
    cards: &CardContext<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    // Ctrl+Enter runs only the selected card.
    let run_keys = if selected {
        cards.run_keys.clone()
    } else {
        String::new()
    };
    let (id, label, tip, keys) = match action {
        CardAction::Run => ("card-run", "Run", "Run this task", run_keys),
        CardAction::Approve => (
            "card-approve",
            "Approve",
            "Accept the result and move the task to Done",
            String::new(),
        ),
    };
    // Accent only on the selected card: a full Todo column stays calm.
    let primary = action == CardAction::Run && selected;
    if cards.starting.contains(&task_id) {
        return disabled_button(id, "Starting…", theme, primary);
    }
    // A quiet button's surface hover would vanish on the raised card: one step above it.
    let action_button = if primary {
        button(id, label, theme, true)
    } else {
        quiet_button(id, label, theme, theme.border)
    };
    action_button
        .tooltip(tooltip(theme, tip, keys))
        .on_click(cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            shell.selected_card = Some(task_id);
            match action {
                CardAction::Run => shell.run_task(task_id, window, cx),
                CardAction::Approve => shell.approve_task(task_id, window, cx),
            }
        }))
}

/// The follow-up input of a running / needs-input card: the multi-line text box that takes the
/// keyboard when clicked; Ctrl+Enter sends it to the card's session (SPEC §10, §20B.1).
pub(crate) fn follow_up_field(
    task_id: Ulid,
    session_id: Ulid,
    cards: &CardContext<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Div {
    let draft = cards
        .follow_up
        .filter(|draft| draft.task_id == task_id && draft.session_id == session_id);
    let text = draft.map_or("", |draft| draft.text.text());
    let focused = draft.is_some() && cards.follow_up_focused;
    let field = text_field(
        "card-follow-up",
        text,
        "Follow up… (Ctrl+Enter sends)",
        focused,
        draft.is_some_and(|draft| draft.text.all_selected()),
        theme,
    )
    .bg(rgb(theme.surface))
    .max_h(px(theme::PROMPT_FIELD_HEIGHT))
    .overflow_y_scroll()
    .on_click(cx.listener(move |shell, _, window, cx| {
        cx.stop_propagation();
        shell.open_follow_up(task_id, session_id, window, cx);
    }))
    .when(draft.is_some(), |el| {
        el.track_focus(cards.follow_up_focus)
            .track_scroll(cards.follow_up_scroll)
            .on_key_down(cx.listener(Shell::follow_up_key_down))
            .relative()
            .child(typing_input(
                cards.follow_up_focus,
                Shell::follow_up_key_down,
                cards.ime_marked,
                cards.typing_target,
                cx,
            ))
    });
    let status = draft.and_then(|draft| {
        if draft.busy {
            Some(("↑", theme.text_muted, "Sending…".to_owned()))
        } else {
            draft
                .error
                .clone()
                .map(|error| ("✕", theme.status_exited, error))
        }
    });
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .child(field)
        .children(status.map(|(icon, color, text)| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(color)).child(icon))
                .child(div().text_color(rgb(theme.text)).child(text))
        }))
}

/// What follows the pointer while a card is dragged: a lifted copy of its title.
pub(crate) struct CardPreview {
    pub(crate) style: PopupStyle,
    pub(crate) title: SharedString,
}

impl Render for CardPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = &self.style;
        div()
            .w(px(theme::BOARD_COLUMN_MIN_WIDTH))
            .p(px(space::M))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .shadow_sm()
            .opacity(opacity::DRAGGING)
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::CARD_TITLE)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .child(self.title.clone())
    }
}
