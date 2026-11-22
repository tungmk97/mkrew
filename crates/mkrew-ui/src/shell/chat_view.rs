//! The Chat mode of an agent pane (ADR 0104): the transcript's turns as messages, tool rows,
//! thinking, a plan card, a question card and a subagent tray. The pane's docked composer sits
//! under it (`pane_frame`). The model is `mkrew_ui::chat`; this file only draws it.

use mkrew_core::thread::ToolCall;
use mkrew_ui::chat;
use mkrew_ui::thread_view::{Page, PageItem};

use crate::*;

/// A keyboard stop of the chat page (Tab reaches it, Enter / Space press it).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Stop {
    /// The "earlier turns" link.
    Earlier,
    /// The toggle of a tool or thinking row.
    Row(chat::RowKey),
    /// "Answer in terminal" of the question card.
    Answer,
}

/// The stops drawn last frame in page order, and their focus handles (kept while drawn).
#[derive(Default)]
pub(crate) struct Stops {
    handles: HashMap<Stop, FocusHandle>,
    order: Vec<Stop>,
}

impl Stops {
    fn begin(&mut self) {
        self.order.clear();
    }

    /// The handle of `stop`, which is drawn now (the next in page order).
    fn stop(&mut self, stop: Stop, cx: &mut App) -> FocusHandle {
        self.order.push(stop);
        (self.handles.entry(stop))
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }

    /// Drops the handles of stops that were not drawn.
    fn end(&mut self) {
        let order = &self.order;
        self.handles.retain(|stop, _| order.contains(stop));
    }

    /// Where Tab (`forward`) or Shift+Tab goes from the stop that has the keyboard, if any.
    fn go(
        &self,
        window: &Window,
        forward: bool,
        list: &FocusHandle,
        composer: &FocusHandle,
    ) -> FocusHandle {
        let handle = |stop: &Stop| self.handles.get(stop);
        let at = self
            .order
            .iter()
            .position(|stop| handle(stop).is_some_and(|h| h.is_focused(window)));
        let to = match chat::tab_to(self.order.len(), at, forward) {
            chat::TabTo::Stop(i) => self.order.get(i).and_then(handle),
            chat::TabTo::List => Some(list),
            chat::TabTo::Composer => None,
        };
        to.unwrap_or(composer).clone()
    }
}

/// What one pane in Chat mode remembers.
pub(crate) struct ChatUi {
    /// Expanded tool and thinking rows.
    pub(crate) open: HashSet<chat::RowKey>,
    pub(crate) scroll: ScrollHandle,
    /// The selected text, inside one message.
    pub(crate) selecting: Option<(chat::MsgKey, mkrew_ui::preview_select::Selection)>,
    /// Where each message's text was painted last frame (for hit-testing a press).
    pub(crate) hits: RefCell<HashMap<chat::MsgKey, preview::Hits>>,
    /// The Tab stops of the page.
    pub(crate) stops: RefCell<Stops>,
}

impl ChatUi {
    pub(crate) fn new() -> Self {
        Self {
            open: HashSet::new(),
            scroll: ScrollHandle::new(),
            selecting: None,
            hits: RefCell::default(),
            stops: RefCell::default(),
        }
    }

    fn stop(&self, stop: Stop, cx: &mut App) -> FocusHandle {
        self.stops.borrow_mut().stop(stop, cx)
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
                    .h(theme::rems_px(theme::COMPACT_SEGMENT_HEIGHT))
                    .child("Terminal")
                    .tooltip(tooltip(
                        theme,
                        "Show the terminal",
                        self.keymap.label(Command::ToggleChat),
                    ))
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
                    .h(theme::rems_px(theme::COMPACT_SEGMENT_HEIGHT))
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
        ui.stops.borrow_mut().begin();
        if let Some(page) = page {
            (ui.hits.borrow_mut()).retain(|key, _| key.0 >= page.first_turn);
        }
        let view = chat::view(page, &ui.open);
        if view.turns.is_empty() {
            ui.stops.borrow_mut().end();
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
            let stop = ui.stop(Stop::Earlier, cx);
            keyboard_stop(
                meta_line(theme, "Earlier turns: Open transcript".into())
                    .id(SharedString::from(format!("chat-earlier-{session}"))),
                &stop,
                theme,
            )
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
        let question = view.question.as_ref().map(|question| {
            let stop = ui.stop(Stop::Answer, cx);
            chat_question(session, question, &stop, theme, cx)
        });
        ui.stops.borrow_mut().end();
        div()
            .size_full()
            .flex()
            .flex_col()
            .when(is_focused, |el| el.track_focus(&self.chat_focus))
            .on_key_down(cx.listener(Self::chat_key_down))
            .child(
                div()
                    .id(SharedString::from(format!("chat-list-{session}")))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&ui.scroll)
                    // A press on a message stops here; any other press ends the selection, so a
                    // drag cannot extend an old one.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |shell, _: &MouseDownEvent, _, cx| {
                            shell.chat_clear_selection(session, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(move |shell, event: &MouseMoveEvent, _, cx| {
                        if event.dragging() {
                            shell.chat_drag(session, event.position, cx);
                        }
                    }))
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
            .children(question)
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
        if matches!(editor::edit_key(&event.keystroke), Some(EditKey::Copy)) {
            cx.stop_propagation();
            let text = self.chats.get(&session).and_then(|ui| {
                let page = self.reads.get(&session)?.page.clone()?;
                chat::copy_text(&page, ui.selecting)
            });
            if let Some(text) = text {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            return;
        }
        if !event.keystroke.modifiers.modified()
            && event.keystroke.key == "escape"
            && let Some(ui) = self.chats.get_mut(&session)
            && ui
                .selecting
                .take_if(|(_, selection)| !selection.is_empty())
                .is_some()
        {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let m = event.keystroke.modifiers;
        if event.keystroke.key == "tab" && !(m.control || m.alt || m.platform) {
            if let Some(ui) = self.chats.get(&session) {
                let to = (ui.stops.borrow()).go(
                    window,
                    !m.shift,
                    &self.chat_focus,
                    &self.composer_focus,
                );
                cx.stop_propagation();
                window.focus(&to, cx);
                cx.notify();
            }
            return;
        }
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
            // A stop that has the keyboard presses itself on Enter (gpui clicks it on release).
            "enter" if !self.chat_focus.is_focused(window) => return,
            "escape" | "enter" => {
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

    /// A press on a message: the pane and the list get the keyboard (Ctrl+C, Esc), the caret
    /// goes under the pointer (Shift extends, a double click takes the word).
    fn chat_press(
        &mut self,
        session: Ulid,
        key: chat::MsgKey,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        self.focus_session_pane(session);
        window.focus(&self.chat_focus, cx);
        let at = self.chats.get(&session).and_then(|ui| {
            ui.hits
                .borrow()
                .get(&key)
                .and_then(|hits| hits.hit(event.position))
        });
        let page = self.reads.get(&session).and_then(|read| read.page.clone());
        let current = self.chats.get(&session).and_then(|ui| ui.selecting);
        let pressed = at.zip(page).and_then(|(at, page)| {
            let shown = chat::text_of(&page, key)?;
            Some(chat::press(
                current,
                key,
                shown,
                at,
                event.modifiers.shift,
                event.click_count,
            ))
        });
        if let Some(ui) = self.chats.get_mut(&session) {
            ui.selecting = pressed;
        }
        cx.notify();
    }

    /// A press in the list that is not on a message: no selection to drag from.
    fn chat_clear_selection(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        if let Some(ui) = self.chats.get_mut(&session)
            && ui.selecting.take().is_some()
        {
            cx.notify();
        }
    }

    /// A drag over the list extends the selection to the text nearest the pointer, in the
    /// selection's own message (so it reaches the message's start or end past its edge).
    fn chat_drag(&mut self, session: Ulid, position: Point<Pixels>, cx: &mut Context<'_, Self>) {
        let Some(ui) = self.chats.get_mut(&session) else {
            return;
        };
        let Some((key, selection)) = ui.selecting.as_mut() else {
            return;
        };
        let at = (ui.hits.borrow())
            .get(&*key)
            .and_then(|hits| hits.hit(position));
        if let Some(at) = at {
            selection.head = at;
            cx.notify();
        }
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
        let Some(chat_ui) = self.chats.get(&session) else {
            return div().into_any_element();
        };
        let rows: Vec<_> = turn
            .rows
            .iter()
            .map(|row| match row {
                chat::Row::Text { item } => {
                    match page
                        .and_then(|page| page.turns.get(index))
                        .and_then(|turn| turn.items.get(*item))
                    {
                        Some(PageItem::Text(shown)) => {
                            let key = (turn.key, *item);
                            let ui = self.chats.get(&session);
                            let hits = ui
                                .map(|ui| ui.hits.borrow_mut().entry(key).or_default().clone())
                                .unwrap_or_default();
                            let selection = ui
                                .and_then(|ui| ui.selecting)
                                .filter(|(selected, _)| *selected == key)
                                .map(|(_, selection)| selection);
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(space::XS))
                                .children(preview::message(shown, selection, &hits, theme))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |shell, event: &MouseDownEvent, window, cx| {
                                            shell.chat_press(session, key, event, window, cx);
                                        },
                                    ),
                                )
                                .into_any_element()
                        }
                        _ => div().into_any_element(),
                    }
                }
                chat::Row::Thinking { key, text, open } => chat_disclosure(
                    (session, *key),
                    *open,
                    div().italic().child("Thinking").into_any_element(),
                    || mono_block((*text).to_owned(), theme.text_muted, theme).into_any_element(),
                    &chat_ui.stop(Stop::Row(*key), cx),
                    theme,
                    cx,
                ),
                chat::Row::Tool { key, call, open } => chat_disclosure(
                    (session, *key),
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
                    &chat_ui.stop(Stop::Row(*key), cx),
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
    (session, key): (Ulid, chat::RowKey),
    open: bool,
    head: gpui::AnyElement,
    body: impl FnOnce() -> gpui::AnyElement,
    stop: &FocusHandle,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .child(
            keyboard_stop(
                div().id(SharedString::from(format!(
                    "chat-row-{session}-{}-{}-{}",
                    key.0, key.1, key.2
                ))),
                stop,
                theme,
            )
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
    stop: &FocusHandle,
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
                keyboard_stop(
                    div().id(SharedString::from(format!("chat-answer-{session}"))),
                    stop,
                    theme,
                )
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

/// `el` as a Tab stop on `handle` with the focus ring: a 1 px border, transparent at rest, so
/// the ring shifts nothing (like `DialogControl::focus_stop`, with a handle the page owns).
fn keyboard_stop(
    el: gpui::Stateful<gpui::Div>,
    handle: &FocusHandle,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let ring = rgb(theme.focus_ring);
    el.border(px(theme::BORDER_WIDTH))
        .border_color(gpui::transparent_black())
        .rounded(px(radius::SMALL))
        .track_focus(handle)
        .focus_visible(move |style| style.border_color(ring))
}
