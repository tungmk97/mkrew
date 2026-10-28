//! Sidebar inbox navigation and the peek popover.

use crate::*;

impl Shell {
    pub(crate) fn inbox_entries(&self) -> Vec<Entry> {
        inbox::entries(
            self.sidebar.projects(),
            self.sidebar.sessions(),
            &self.viewed,
        )
    }

    /// Hiding also closes what only the sidebar draws (the peek, the workspace menu) and takes
    /// the keyboard out of the filter box, so it never stays in something invisible (the `◧`
    /// click does not pass `Shell::run`'s filter guard). Showing brings back the last width.
    pub(crate) fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.sidebar_layout.toggle_hidden();
        if self.sidebar_layout.hidden() {
            if self.filter_focus.is_focused(window) {
                self.focus_pane(window, cx);
            }
            self.close_peek(window, cx);
            self.workspace_menu = None;
        }
        cx.notify();
        self.save_sidebar_layout(cx);
    }

    /// Saves the layout as `[ui] sidebar` (a file that does not parse keeps it for this run).
    fn save_sidebar_layout(&mut self, cx: &mut Context<'_, Self>) {
        let value = format!("\"{}\"", self.sidebar_layout.mode().name());
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "sidebar", &value)
        {
            self.settings_write_failed("the sidebar layout", error, cx);
        }
    }

    /// Full width ↔ compact rail; a hidden sidebar comes back as the rail. The rail draws no
    /// filter box or chips, so entering it clears them (nothing stays narrowed unseen) and the
    /// keyboard leaves the filter box. A peek or menu hung on a full-width row closes.
    pub(crate) fn toggle_compact_sidebar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.sidebar_layout.toggle_compact();
        if self.sidebar_layout.compact() {
            if self.filter_focus.is_focused(window) {
                self.focus_pane(window, cx);
            }
            self.sidebar.clear_filter();
            self.close_peek(window, cx);
            self.workspace_menu = None;
        }
        cx.notify();
        self.save_sidebar_layout(cx);
    }

    /// Alt+0: the sidebar's filter takes the keyboard with the active row highlighted. A
    /// hidden sidebar is shown, and the compact rail (no filter box) widens first.
    pub(crate) fn focus_sidebar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.sidebar.is_empty() {
            return;
        }
        if self.sidebar_layout.hidden() {
            self.toggle_sidebar(window, cx);
        }
        if self.sidebar_layout.compact() {
            self.toggle_compact_sidebar(window, cx);
        }
        let rows = self.sidebar.view(&self.viewed).rows;
        self.sidebar
            .browse_from(&rows, self.focused_session(), self.workspace);
        window.focus(&self.filter_focus, cx);
        cx.notify();
    }

    /// Opens the Inbox entry after the focused pane's session (wrapping); nothing while the
    /// Inbox is empty. The key and the sidebar footer's Inbox state both land here.
    pub(crate) fn jump_next_inbox(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let entries = self.inbox_entries();
        let focused = self.focused_session().and_then(|id| {
            self.sidebar
                .sessions()
                .iter()
                .find(|session| session.id == id)
        });
        if let Some(session) = inbox::next(&entries, focused) {
            self.open_session(session, window, cx);
        }
    }

    /// Opens the Inbox entry before the focused pane's session (wrapping); nothing while the
    /// Inbox is empty.
    pub(crate) fn jump_previous_inbox(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let entries = self.inbox_entries();
        let focused = self.focused_session().and_then(|id| {
            self.sidebar
                .sessions()
                .iter()
                .find(|session| session.id == id)
        });
        if let Some(session) = inbox::previous(&entries, focused) {
            self.open_session(session, window, cx);
        }
    }

    /// Whether `session_id` can be peeked: listed and live (`session.read` needs a live one).
    pub(crate) fn peekable(&self, session_id: Ulid) -> bool {
        self.sidebar
            .sessions()
            .iter()
            .any(|session| session.id == session_id && session.state != SessionState::Exited)
    }

    /// Opens the peek of `session_id` next to its row (§20A.1). With `focused` the keyboard
    /// moves into it and its row is revealed; a peek that already had the keyboard keeps it.
    pub(crate) fn open_peek(
        &mut self,
        session_id: Ulid,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() || !self.peekable(session_id) {
            return;
        }
        // A keyboard peek hangs on the session's row: open its workspace, drop what hides it.
        if focused && let Some(project_id) = self.project_of(session_id) {
            self.sidebar.reveal(project_id);
        }
        let had_keyboard = self.peek.as_ref().is_some_and(|peek| peek.focused);
        self.sidebar_layout.show();
        self.peek = Some(Peek::new(session_id, focused || had_keyboard));
        if focused && !had_keyboard {
            self.take_peek_focus(window, cx);
        }
        self.read_peek(window, cx);
        cx.notify();
    }

    /// Moves the keyboard into the open peek, remembering where it was.
    pub(crate) fn take_peek_focus(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let terminal = self.focused_session().filter(|session| {
            self.panes
                .get(session)
                .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
        });
        let focused = window.focused(cx);
        if focused.as_ref() != Some(&self.peek_focus) {
            self.peek_return = focused.map(|focus| (focus, terminal));
        }
        if let Some(peek) = self.peek.as_mut() {
            peek.focused = true;
        }
        window.focus(&self.peek_focus, cx);
        cx.notify();
    }

    /// Closes the peek; if it had the keyboard, the keyboard goes back to where it was
    /// (unless that pane went away meanwhile).
    pub(crate) fn close_peek(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(peek) = self.peek.take() else {
            return;
        };
        if peek.focused {
            match self.peek_return.take() {
                Some((focus, session))
                    if session.is_none_or(|session| self.panes.contains_key(&session)) =>
                {
                    window.focus(&focus, cx);
                }
                _ => self.focus_pane(window, cx),
            }
        }
        cx.notify();
    }

    /// Reads the peeked session's last screen lines (`session.read`, never attaching).
    pub(crate) fn read_peek(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(peek) = self.peek.as_mut() else {
            return;
        };
        let session_id = peek.session_id;
        let (serial, params) = peek.read_params();
        let read = connection.call::<SessionReadResult>(Request::SessionRead(params));
        cx.spawn_in(window, async move |this, cx| {
            let read = read
                .await
                .map(|read| read.lines)
                .map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Some(peek) = shell
                    .peek
                    .as_mut()
                    .filter(|peek| peek.session_id == session_id)
                {
                    peek.read(serial, read);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// After sessions changed: the peek closes once its session ended or its row is no longer
    /// shown (a chip or filter now hides it), and reads the screen again when its session's
    /// state `changed`.
    pub(crate) fn update_peek(
        &mut self,
        changed: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(peek) = self.peek.as_ref() else {
            return;
        };
        let session_id = peek.session_id;
        // The popover only draws on its row: without the row it would be invisible but keep
        // the keyboard.
        let row_shown = self
            .sidebar
            .view(&self.viewed)
            .rows
            .iter()
            .any(|row| matches!(row, Row::Session(row) if row.session_id == session_id));
        if !self.peekable(session_id) || !row_shown {
            return self.close_peek(window, cx);
        }
        if changed {
            self.read_peek(window, cx);
        }
    }

    /// The mouse entered or left a session row: resting on it for the hover delay peeks it
    /// (unless the open peek is in use); leaving lets an unused hover peek close.
    pub(crate) fn hover_row(
        &mut self,
        session_id: Ulid,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !hovered {
            self.peek_hover.leave(session_id);
            self.close_unused_peek(window, cx);
            return;
        }
        let serial = self.peek_hover.enter(session_id);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::PEEK_HOVER_DELAY)
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(session_id) = shell.peek_hover.fire(serial) else {
                    return;
                };
                let shown = shell
                    .peek
                    .as_ref()
                    .is_some_and(|peek| peek.session_id == session_id);
                if !shown && shell.peek.as_ref().is_none_or(Peek::yields_to_hover) {
                    shell.open_peek(session_id, false, window, cx);
                }
            });
        })
        .detach();
    }

    /// The mouse entered or left the peek popover.
    pub(crate) fn hover_peek(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(peek) = self.peek.as_mut() {
            peek.hovered = hovered;
            if !hovered {
                self.close_unused_peek(window, cx);
            }
        }
    }

    /// Closes a peek nobody uses once the mouse is neither on its row nor on it, after a short
    /// grace so the mouse can travel from the row to the popover.
    pub(crate) fn close_unused_peek(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.peek.is_none() {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::PEEK_LEAVE_GRACE)
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(peek) = shell.peek.as_ref() else {
                    return;
                };
                if !peek.stays(shell.peek_hover.is_over(peek.session_id)) {
                    shell.close_peek(window, cx);
                }
            });
        })
        .detach();
    }

    /// Typing edits the reply, Enter sends it, Alt+Y / Alt+N answer, Ctrl+O opens the pane, ↑ / ↓
    /// peek the other sessions that need you, Esc closes.
    pub(crate) fn peek_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(peek) = self.peek.as_mut() else {
            return;
        };
        if let Some(key) = task_dialog::dialog_key(&event.keystroke) {
            copy_selection(&key, &peek.text, cx);
        }
        match peek.key(&event.keystroke) {
            peek::Outcome::Edited => cx.notify(),
            peek::Outcome::Close => self.close_peek(window, cx),
            peek::Outcome::Send => {
                if let Some(params) = peek.send_params() {
                    self.send_peek(params, true, window, cx);
                }
            }
            peek::Outcome::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    peek.insert(&text);
                    cx.notify();
                }
            }
            peek::Outcome::Cycle(by) => {
                let current = peek.session_id;
                if let Some(next) = peek::cycle(&self.inbox_entries(), current, by)
                    && next != current
                {
                    self.open_peek(next, true, window, cx);
                }
            }
            peek::Outcome::Answer(letter) => self.answer_peek(letter, window, cx),
            peek::Outcome::Open => {
                let session = peek.session_id;
                self.close_peek(window, cx);
                self.open_session(session, window, cx);
            }
        }
    }

    /// A `y` / `n` button: sends that letter with submit while the session needs input.
    pub(crate) fn answer_peek(
        &mut self,
        letter: char,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(peek) = self.peek.as_ref() else {
            return;
        };
        let state = self
            .sidebar
            .sessions()
            .iter()
            .find(|session| session.id == peek.session_id)
            .map(|session| session.state);
        if let Some(params) = state.and_then(|state| peek.answer(letter, state)) {
            self.send_peek(params, false, window, cx);
        }
    }

    /// `session.send` from the peek; a sent `draft` empties the reply field. Once sent, a peek
    /// with the keyboard moves on to the next session that needs you (closing when none is left).
    pub(crate) fn send_peek(
        &mut self,
        params: SessionSendParams,
        draft: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(peek) = self.peek.as_mut() else {
            return;
        };
        let session_id = peek.session_id;
        peek.busy = true;
        peek.error = None;
        cx.notify();
        let sent = connection.request(Request::SessionSend(params));
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%session_id, "session.send failed: {error}");
                }
                // The peek was closed or moved to another session meanwhile.
                let Some(peek) = shell
                    .peek
                    .as_mut()
                    .filter(|peek| peek.session_id == session_id)
                else {
                    return;
                };
                let advance = result.is_ok() && peek.focused;
                peek.sent(result, draft);
                cx.notify();
                if advance {
                    match peek::cycle(&shell.inbox_entries(), session_id, 1) {
                        Some(next) if next != session_id => {
                            shell.open_peek(next, true, window, cx);
                        }
                        _ => shell.close_peek(window, cx),
                    }
                }
            });
        })
        .detach();
    }

    /// `row` with the peek popover hung at its top right when the peek shows `session_id`.
    pub(crate) fn with_peek(
        &self,
        row: gpui::AnyElement,
        session_id: Ulid,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let popover = self
            .peek
            .as_ref()
            .filter(|peek| peek.session_id == session_id && !self.modal_open())
            .and_then(|peek| self.render_peek(peek, now, theme, cx));
        match popover {
            Some(popover) => div()
                .relative()
                .child(row)
                .child(div().absolute().top_0().right_0().child(popover))
                .into_any_element(),
            None => row,
        }
    }

    /// The peek popover (§20A.1): the session's name, state and its age, its last screen
    /// lines, a one-line reply and `y` / `n` while it needs input. Floats right of its row; a
    /// click outside closes it.
    pub(crate) fn render_peek(
        &self,
        peek: &Peek,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let session = self
            .sidebar
            .sessions()
            .iter()
            .find(|session| session.id == peek.session_id)?;
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == session.project_id)
            .map_or("", |project| project.name.as_str());
        let name = match history::title(session) {
            Some(title) => format!("{workspace}: {} · {title}", session.agent_profile),
            None => format!("{workspace}: {}", session.agent_profile),
        };
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let screen = match &peek.screen {
            peek::Screen::Reading => muted("Reading the screen…".into()),
            peek::Screen::Failed(error) => muted(format!("Cannot read the screen: {error}")),
            peek::Screen::Read(lines) if lines.is_empty() => muted("The screen is empty.".into()),
            peek::Screen::Read(lines) => div()
                .flex()
                .flex_col()
                .font_family(theme.terminal_font_family.clone())
                .text_size(font_size::META)
                .text_color(rgb(theme.text))
                .children(lines.iter().map(|line| {
                    div()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(line.clone())
                })),
        };
        let needs_input = session.state == SessionState::NeedsInput;
        let answer = |id: &'static str, letter: char| {
            let (label, tip) = match letter {
                'y' => ("y", "Send y + Enter"),
                _ => ("n", "Send n + Enter"),
            };
            if peek.busy {
                disabled_button(id, label, theme, false)
            } else {
                button(id, label, theme, false)
                    .tooltip(tooltip(
                        theme,
                        tip,
                        if letter == 'y' { "Alt+Y" } else { "Alt+N" },
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.answer_peek(letter, window, cx);
                    }))
            }
        };
        let reply = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(
                div().flex_1().min_w_0().child(
                    caret_field(
                        "peek-reply",
                        &peek.text,
                        "Reply to the agent",
                        peek.focused,
                        theme,
                        cx,
                        |shell, caret| {
                            if let Some(peek) = shell.peek.as_mut() {
                                peek.text.click(caret.at, caret.extend, caret.clicks);
                            }
                        },
                    )
                    .when(peek.busy, |el| el.opacity(opacity::DISABLED))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        if shell.peek.as_ref().is_some_and(|peek| !peek.focused) {
                            shell.take_peek_focus(window, cx);
                        }
                    })),
                ),
            )
            .when(needs_input, |el| {
                el.child(answer("peek-yes", 'y'))
                    .child(answer("peek-no", 'n'))
            });
        let hint = if peek.busy {
            "Sending…".to_owned()
        } else if !peek.focused {
            format!(
                "Click the field to reply · {} peeks the first session that needs you",
                self.keymap.label(Command::Peek)
            )
        } else if needs_input {
            "Enter sends · Alt+Y / Alt+N answer · Ctrl+O opens · ↑ ↓ others · Esc closes".to_owned()
        } else {
            "Enter sends · Ctrl+O opens the pane · ↑ ↓ others that need you · Esc closes".to_owned()
        };
        let panel = div()
            .id("peek")
            .w(px(theme::PEEK_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            // Only once it has the keyboard: a tracked element takes focus on any mouse down,
            // so a click on a hover peek would steal the keyboard before `take_peek_focus`.
            .when(peek.focused, |el| el.track_focus(&self.peek_focus))
            .on_key_down(cx.listener(Self::peek_key_down))
            .when(peek.focused, |el| {
                el.relative()
                    .child(self.typing(&self.peek_focus, Self::peek_key_down, cx))
            })
            .on_mouse_down_out(cx.listener(|shell, _, window, cx| {
                shell.close_peek(window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::BODY)
                            .child(status_mark(session.state, theme)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(font_size::BODY)
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(status::text(
                                session.state,
                                session.exit_code,
                                session.state_changed_at,
                                now,
                            )),
                    ),
            )
            .child(
                div()
                    .p(px(space::S))
                    .rounded(px(radius::SMALL))
                    .bg(rgb(theme.bg))
                    .child(screen),
            )
            .child(reply)
            .children(peek.error.clone().map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_exited))
                    .child(error)
            }))
            .child(muted(hint));
        // The padding bridges the row's edge, so the mouse can travel onto the popover.
        let area = div()
            .id("peek-area")
            .pl(px(space::S))
            .on_hover(cx.listener(|shell, hovered: &bool, window, cx| {
                shell.hover_peek(*hovered, window, cx);
            }))
            .child(panel);
        Some(
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(space::S))
                    .child(area),
            )
            .into_any_element(),
        )
    }
}
