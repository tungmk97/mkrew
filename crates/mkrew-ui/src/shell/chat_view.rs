//! The Chat mode of an agent pane (ADR 0104): the transcript's turns as messages, tool rows,
//! thinking, a plan card, a question card and a subagent tray. The pane's docked composer sits
//! under it (`pane_frame`). The model is `mkrew_ui::chat`; this file only draws it.

use mkrew_core::thread::ToolCall;
use mkrew_ui::chat;
use mkrew_ui::thread_view::{Page, PageItem};

use crate::*;

/// What one pane in Chat mode remembers.
pub(crate) struct ChatUi {
    /// Expanded tool and thinking rows.
    pub(crate) open: HashSet<chat::RowKey>,
    pub(crate) scroll: ScrollHandle,
}

impl ChatUi {
    pub(crate) fn new() -> Self {
        Self {
            open: HashSet::new(),
            scroll: ScrollHandle::new(),
        }
    }
}

impl Shell {
    /// Sessions on screen in any window (the main window's tab and the detached ones').
    fn visible_sessions(&self) -> Vec<Ulid> {
        (self.hosted_sessions(self.main_visible).into_iter())
            .map(|(_, session)| session)
            .collect()
    }

    /// `info`'s pane can show Chat.
    pub(crate) fn chat_available(&self, info: &Session) -> bool {
        let has_transcript = (self.usages.get(info.id)).is_some_and(|u| u.transcript.is_some());
        chat::available(info.kind, &info.agent_profile, has_transcript)
    }

    /// `info`'s pane shows Chat: its saved mode (the layout leaf), else `[ui] chat_default`.
    pub(crate) fn chat_on(&self, info: &Session) -> bool {
        let saved = self.tabs.values().find_map(|tabs| tabs.chat_of(info.id));
        chat::effective(self.chat_available(info), saved, self.chat_default)
    }

    pub(crate) fn chat_session(&self, session: Ulid) -> bool {
        (self.sidebar.sessions().iter())
            .find(|s| s.id == session)
            .is_some_and(|info| self.chat_on(info))
    }

    /// The panes on screen in Chat mode, with their agents' native ids.
    pub(crate) fn chat_panes(&self) -> Vec<(Ulid, Option<String>)> {
        let visible = self.visible_sessions();
        (self.sidebar.sessions().iter())
            .filter(|s| visible.contains(&s.id) && self.chat_on(s))
            .map(|s| (s.id, s.native_session_id.clone()))
            .collect()
    }

    /// Keeps the Chat state of the panes on screen, drops the rest, and shows each Chat
    /// pane's composer (a Chat pane always has it). A pane that starts on a session whose
    /// shared read has a page already gets no first page: it starts at the end. Status tick.
    /// Returns whether anything a pane draws changed (a new state or a newly shown composer),
    /// so the caller redraws at once.
    pub(crate) fn sync_chats(&mut self, panes: &[(Ulid, Option<String>)]) -> bool {
        let before = self.chats.len();
        self.chats
            .retain(|id, _| panes.iter().any(|(pane, _)| pane == id));
        let mut changed = self.chats.len() != before;
        for (id, _) in panes {
            if !self.chats.contains_key(id) {
                let ui = ChatUi::new();
                if self.reads.get(id).is_some_and(|read| read.page.is_some()) {
                    ui.scroll.scroll_to_bottom();
                }
                self.chats.insert(*id, ui);
                changed = true;
            }
            changed |= self.composers.shown(*id).is_none();
            self.composers.open(*id);
        }
        changed
    }

    /// A subagent runs in a visible Chat pane: its loading dots move on the status tick.
    pub(crate) fn chat_animating(&self) -> bool {
        self.chats.keys().any(|id| {
            (self.reads.get(id))
                .and_then(|read| read.page.as_ref())
                .and_then(|page| page.turns.last())
                .is_some_and(|turn| turn.subagents.iter().any(|s| !s.done))
        })
    }

    pub(crate) fn toggle_chat_row(
        &mut self,
        session: Ulid,
        key: chat::RowKey,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(ui) = self.chats.get_mut(&session) {
            if !ui.open.remove(&key) {
                ui.open.insert(key);
            }
            cx.notify();
        }
    }

    /// Saves `on` as `session`'s mode in its tab layout and gives the pane the keyboard.
    pub(crate) fn set_chat(
        &mut self,
        session: Ulid,
        on: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((project, _)) = self.tab_of_session(session) else {
            return;
        };
        let write = (self.tabs.get_mut(&project)).and_then(|tabs| tabs.set_chat(session, Some(on)));
        if on {
            self.composers.open(session);
        } else {
            self.composers.hide(session);
        }
        if let Some(write) = write {
            self.save_layout(project, write, window, cx);
        }
        self.focus_session_pane(session);
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Chat <-> Terminal for `session`'s pane, when it can show Chat.
    pub(crate) fn toggle_chat(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let on = (self.sidebar.sessions().iter())
            .find(|s| s.id == session && self.chat_available(s))
            .map(|info| self.chat_on(info));
        if let Some(on) = on {
            self.set_chat(session, !on, window, cx);
        }
    }

    /// The `Terminal | Chat` control of an agent pane's header (`None` when it cannot chat).
    pub(crate) fn pane_mode_switch(
        &self,
        info: &Session,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.chat_available(info) {
            return None;
        }
        let (session, chat) = (info.id, self.chat_on(info));
        Some(
            segmented(theme)
                .child(
                    segment(
                        SharedString::from(format!("pane-mode-terminal-{session}")),
                        !chat,
                        theme,
                    )
                    .child("Terminal")
                    .tooltip(tooltip(theme, "Show the terminal", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        if chat {
                            shell.set_chat(session, false, window, cx);
                        }
                    })),
                )
                .child(
                    segment(
                        SharedString::from(format!("pane-mode-chat-{session}")),
                        chat,
                        theme,
                    )
                    .child("Chat")
                    .tooltip(tooltip(
                        theme,
                        "Show the conversation as a chat",
                        self.keymap.label(Command::ToggleChat),
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        if !chat {
                            shell.set_chat(session, true, window, cx);
                        }
                    })),
                )
                .into_any_element(),
        )
    }

    /// The Chat page of `session`'s pane: the turns (scrolling, following the end), the subagent
    /// tray and the question card. `is_focused`: this pane has the keyboard, so its list takes
    /// `chat_focus`. The composer is not part of it: `pane_frame` docks it below.
    pub(crate) fn render_chat(
        &self,
        session: Ulid,
        is_focused: bool,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(ui) = self.chats.get(&session) else {
            return div().size_full().into_any_element();
        };
        let page = self
            .reads
            .get(&session)
            .and_then(|read| read.page.as_deref());
        let view = chat::view(page, &ui.open);
        if view.turns.is_empty() {
            return message(
                theme,
                None,
                "No messages yet. Type below to start.".into(),
                None,
            )
            .size_full()
            .into_any_element();
        }
        let earlier = view.earlier.then(|| {
            meta_line(theme, "Earlier turns: Open transcript".into())
                .id(SharedString::from(format!("chat-earlier-{session}")))
                .pressable(Look::Text, true, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| shell.open_transcript(session, cx)),
                )
        });
        let turns: Vec<_> = view
            .turns
            .iter()
            .enumerate()
            .map(|(index, turn)| self.chat_turn(session, page, index, turn, theme, cx))
            .collect();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(SharedString::from(format!("chat-list-{session}")))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&ui.scroll)
                    .when(is_focused, |el| el.track_focus(&self.chat_focus))
                    .on_key_down(cx.listener(Self::chat_key_down))
                    .flex()
                    .flex_col()
                    .gap(px(space::M))
                    .p(px(space::M))
                    .font_family(theme.ui_font_family.clone())
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .children(earlier)
                    .children(turns),
            )
            .when(!view.tray.is_empty(), |el| {
                el.child(chat_tray(&view.tray, now, theme))
            })
            .children(
                view.question
                    .as_ref()
                    .map(|question| chat_question(session, question, theme, cx)),
            )
            .into_any_element()
    }

    /// Keys of the chat list: arrows, PageUp / PageDown, Home and End scroll; Esc, Enter and Tab
    /// go back to the composer. Other keys (shortcuts) go on to the keymap.
    pub(crate) fn chat_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(session) = self.focused_session(window) else {
            return;
        };
        if event.keystroke.modifiers.modified() {
            return;
        }
        let Some(ui) = self.chats.get(&session) else {
            return;
        };
        let page = f32::from(ui.scroll.bounds().size.height);
        let step = theme::CHAT_SCROLL_STEP;
        let delta = match event.keystroke.key.as_str() {
            "up" => -step,
            "down" => step,
            "pageup" => -page,
            "pagedown" => page,
            "home" => f32::MIN,
            "end" => f32::MAX,
            "escape" | "enter" | "tab" => {
                cx.stop_propagation();
                window.focus(&self.composer_focus, cx);
                cx.notify();
                return;
            }
            _ => return,
        };
        cx.stop_propagation();
        let (offset, max) = (ui.scroll.offset(), ui.scroll.max_offset());
        let y = chat::scrolled(f32::from(offset.y), f32::from(max.y), delta);
        ui.scroll.set_offset(point(offset.x, px(y)));
        cx.notify();
    }

    /// One turn: the prompt card (right), then its rows, then the plan card if it has one.
    fn chat_turn(
        &self,
        session: Ulid,
        page: Option<&Page>,
        index: usize,
        turn: &chat::ChatTurn<'_>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let rows: Vec<_> = turn
            .rows
            .iter()
            .map(|row| match row {
                chat::Row::Text { item } => {
                    match page
                        .and_then(|page| page.turns.get(index))
                        .and_then(|turn| turn.items.get(*item))
                    {
                        Some(PageItem::Text(shown)) => div()
                            .flex()
                            .flex_col()
                            .gap(px(space::XS))
                            .children(preview::blocks(&shown.blocks, shown, theme))
                            .into_any_element(),
                        _ => div().into_any_element(),
                    }
                }
                chat::Row::Thinking { key, text, open } => chat_disclosure(
                    session,
                    *key,
                    *open,
                    div().italic().child("Thinking").into_any_element(),
                    || mono_block((*text).to_owned(), theme.text_muted, theme).into_any_element(),
                    theme,
                    cx,
                ),
                chat::Row::Tool { key, call, open } => chat_disclosure(
                    session,
                    *key,
                    *open,
                    div()
                        .flex()
                        .gap(px(space::S))
                        .min_w_0()
                        .child(
                            div()
                                .flex_none()
                                .font_weight(FontWeight(weight::MEDIUM))
                                .child(call.name.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(theme.terminal_font_family.clone())
                                .child(call.target.clone()),
                        )
                        .into_any_element(),
                    || tool_body(call, theme).into_any_element(),
                    theme,
                    cx,
                ),
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .child(
                div().flex().justify_end().child(
                    div()
                        .max_w(relative(theme::CHAT_PROMPT_MAX_FRACTION))
                        .p(px(space::S))
                        .rounded(px(radius::MEDIUM))
                        .bg(rgb(theme.card))
                        .child(turn.prompt.to_owned()),
                ),
            )
            .children(rows)
            .children(turn.plan.as_deref().map(|steps| chat_plan(steps, theme)))
            .into_any_element()
    }
}

/// A row that opens and closes: `head` always, `body` (built only then) while open; a click
/// on the head toggles.
fn chat_disclosure(
    session: Ulid,
    key: chat::RowKey,
    open: bool,
    head: gpui::AnyElement,
    body: impl FnOnce() -> gpui::AnyElement,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .child(
            div()
                .id(SharedString::from(format!(
                    "chat-row-{session}-{}-{}-{}",
                    key.0, key.1, key.2
                )))
                .flex()
                .items_center()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .pressable(Look::Text, true, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| shell.toggle_chat_row(session, key, cx)),
                )
                .child(icon_svg(
                    if open {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    theme::ICON_SIZE_SMALL,
                    theme.text_muted,
                ))
                .child(head),
        )
        .when(open, |el| el.child(body()))
        .into_any_element()
}

/// Monospace text on a faint block (arguments, results, thinking).
fn mono_block(text: String, color: u32, theme: &Theme) -> gpui::Div {
    div()
        .p(px(space::S))
        .rounded(px(radius::SMALL))
        .bg(rgb(theme.surface))
        .font_family(theme.terminal_font_family.clone())
        .text_size(font_size::META)
        .text_color(rgb(color))
        .child(text)
}

/// An open tool row: the arguments, then the result (`danger_text` when it is an error).
fn tool_body(call: &ToolCall, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .pl(px(space::L))
        .when(!call.input.is_empty(), |el| {
            el.child(mono_block(call.input.clone(), theme.text_muted, theme))
        })
        .child(match &call.result {
            Some(result) => mono_block(
                if result.text.is_empty() {
                    "(no output)".to_owned()
                } else {
                    result.text.clone()
                },
                if result.is_error {
                    theme.danger_text
                } else {
                    theme.text
                },
                theme,
            ),
            None => div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child("No result yet."),
        })
}

/// The plan card: a mark per step (ring, half ring, check); the step in progress in the text
/// color, the others muted.
fn chat_plan(steps: &[chat::PlanRow<'_>], theme: &Theme) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .p(px(space::M))
        .rounded(px(radius::CARD))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.divider))
        .bg(rgb(theme.card))
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child("Plan"),
        )
        .children(steps.iter().map(|step| {
            let (icon, color) = match step.mark {
                chat::PlanMark::Todo => (Icon::Circle, theme.text_muted),
                chat::PlanMark::Doing => (Icon::CircleHalf, theme.text),
                chat::PlanMark::Done => (Icon::Check, theme.text_muted),
            };
            div()
                .flex()
                .items_start()
                .gap(px(space::S))
                .child(icon_svg(icon, theme::ICON_SIZE_SMALL, color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(rgb(color))
                        .child(step.text.to_owned()),
                )
        }))
        .into_any_element()
}

/// The subagent tray: a chip per subagent with its description and loading dots or a check.
fn chat_tray(chips: &[chat::Chip<'_>], now: i64, theme: &Theme) -> gpui::AnyElement {
    div()
        .flex_none()
        .flex()
        .flex_wrap()
        .gap(px(space::S))
        .px(px(space::M))
        .py(px(space::S))
        .border_t(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.divider))
        .font_family(theme.ui_font_family.clone())
        .text_size(font_size::META)
        .children(chips.iter().map(|chip| {
            div()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded_full()
                .bg(rgb(theme.card))
                .child(if chip.done {
                    icon_svg(Icon::Check, theme::ICON_SIZE_SMALL, theme.success_text)
                        .into_any_element()
                } else {
                    loading_dots(now, rgb(theme.status_working)).into_any_element()
                })
                .child(
                    div()
                        .text_color(rgb(theme.text))
                        .child(chip.description.to_owned()),
                )
                .when(!chip.subagent_type.is_empty(), |el| {
                    el.child(
                        div()
                            .text_color(rgb(theme.text_muted))
                            .child(chip.subagent_type.to_owned()),
                    )
                })
        }))
        .into_any_element()
}

/// The pending question: its text, its options as a list, and a button that switches the pane
/// to Terminal, where the answer is typed (the card never types for the user).
fn chat_question(
    session: Ulid,
    question: &chat::QuestionCard<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(space::S))
        .mx(px(space::M))
        .mb(px(space::M))
        .p(px(space::M))
        .rounded(px(radius::CARD))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.status_needs_input))
        .bg(rgb(theme.card))
        .font_family(theme.ui_font_family.clone())
        .text_size(font_size::BODY)
        .child(
            div()
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(question.text.to_owned()),
        )
        .children(question.options.iter().map(|option| {
            div()
                .text_color(rgb(theme.text_muted))
                .child(format!("• {option}"))
        }))
        .child(
            div().flex().child(
                div()
                    .id(SharedString::from(format!("chat-answer-{session}")))
                    .px(px(space::M))
                    .py(px(space::XS))
                    .rounded(px(radius::CHIP))
                    .bg(rgb(theme.surface_raised))
                    .text_color(rgb(theme.text))
                    .pressable(Look::Raised, true, theme)
                    .on_press(
                        true,
                        cx.listener(move |shell, _, window, cx| {
                            shell.set_chat(session, false, window, cx);
                        }),
                    )
                    .child("Answer in terminal"),
            ),
        )
        .into_any_element()
}
