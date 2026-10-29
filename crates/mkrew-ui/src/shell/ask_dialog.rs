//! The "Ask AI…" dialog (ADR 0061): one question to an agent, outside every project.

use crate::*;

impl Shell {
    /// Opens the dialog and lists the agent profiles; the active workspace's default agent is
    /// picked first.
    pub(crate) fn open_ask(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let default_agent = self
            .sidebar
            .projects()
            .iter()
            .find(|project| Some(project.id) == self.workspace)
            .and_then(|project| project.default_agent.clone());
        self.ask = Some(Ask::new(default_agent, self.ask_web));
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if shell.dialog_serial == serial
                    && let Some(dialog) = shell.ask.as_mut()
                {
                    dialog.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn close_ask(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.ask = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Typing edits the question (Enter = new line), Ctrl+Enter asks, Esc closes.
    pub(crate) fn ask_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        // A control has the keyboard: the question takes nothing but Esc / Ctrl+Enter.
        if self.dialog_control_focused(window)
            && !matches!(key, DialogKey::Cancel | DialogKey::Save)
        {
            return;
        }
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(dialog) = self.ask.as_mut() else {
            return;
        };
        if let Some(text) = paste {
            dialog.question.insert(&text);
        }
        if matches!(key, DialogKey::Copy | DialogKey::Cut)
            && let Some(text) = dialog.question.input().selected_text()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        }
        match dialog.key(key) {
            text_box::Outcome::Edited => {
                let question = dialog.question.input();
                if question.cursor() == question.len() {
                    self.prompt_scroll.scroll_to_bottom();
                }
                cx.notify();
            }
            text_box::Outcome::Cancel => self.close_ask(window, cx),
            text_box::Outcome::Submit => self.send_ask(window, cx),
        }
    }

    /// `ai.generate` `ask` for the typed question; the answer arrives as `ai.generated`.
    pub(crate) fn send_ask(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let Some(params) = self.ask.as_mut().and_then(Ask::begin) else {
            return;
        };
        let generation_id = params.generation_id;
        cx.notify();
        let started = connection.call::<()>(Request::AiGenerate(params));
        cx.spawn_in(window, async move |this, cx| {
            let Err(error) = started.await else {
                return;
            };
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.ask.as_mut() {
                    dialog.answered(generation_id, Err(format!("{error:#}")));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// "Ask AI": the agent and web search choosers, the question, the answer (Markdown), then
    /// Copy answer, Close and Ask (Ctrl+Enter).
    pub(crate) fn render_ask(
        &self,
        dialog: &Ask,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let selected = dialog.selected();
        let agents = match &dialog.agents {
            Agents::Loading => muted("Loading agents…".into()).into_any_element(),
            Agents::Failed(error) => {
                notice(theme.text, format!("✕ Cannot list the agents: {error}")).into_any_element()
            }
            Agents::Loaded(_) if selected.is_none() => muted(
                "No installed agent can answer. Add a line like `ask = \"claude -p\"` to an \
                 agent profile."
                    .into(),
            )
            .into_any_element(),
            Agents::Loaded(_) => {
                let chips = dialog
                    .choices()
                    .into_iter()
                    .map(|profile| Chip {
                        id: SharedString::from(format!("ask-agent-{}", profile.id)),
                        label: profile.name.clone().into(),
                        tip: format!("Ask {}", profile.name),
                        active: selected.is_some_and(|s| s.id == profile.id),
                        pick: profile.id.clone(),
                    })
                    .collect();
                Self::chip_row(
                    "Agent",
                    chips,
                    true,
                    theme,
                    |shell, id, _, cx| {
                        if let Some(dialog) = shell.ask.as_mut() {
                            dialog.agent = Some(id);
                            cx.notify();
                        }
                    },
                    cx,
                )
                .into_any_element()
            }
        };
        let web = selected.map(|profile| {
            if !profile.ask_web {
                return muted(format!(
                    "Web search: add an `ask_web` line to {}.toml",
                    profile.id
                ))
                .into_any_element();
            }
            let chips = [
                (false, "Off", "Answer from what the agent knows"),
                (true, "On", "Let the agent search the web (slower)"),
            ]
            .into_iter()
            .map(|(on, label, tip)| Chip {
                id: SharedString::from(format!("ask-web-{label}")),
                label: label.into(),
                tip: tip.into(),
                active: dialog.web == on,
                pick: on,
            })
            .collect();
            Self::chip_row(
                "Web search",
                chips,
                true,
                theme,
                |shell, on, _, cx| {
                    shell.ask_web = on;
                    if let Some(dialog) = shell.ask.as_mut() {
                        dialog.web = on;
                    }
                    cx.notify();
                },
                cx,
            )
            .into_any_element()
        });
        // A click puts the caret under the pointer, a drag selects (SPEC §13).
        let field = caret_field(
            "ask-question",
            dialog.question.input(),
            "Ask anything…",
            self.dialog_focus.is_focused(window),
            theme,
            cx,
            |shell, caret| {
                if let Some(dialog) = shell.ask.as_mut() {
                    dialog.question.click(caret.at, caret.extend, caret.clicks);
                }
            },
        )
        .h(px(theme::PROMPT_FIELD_HEIGHT))
        .overflow_y_scroll()
        .track_scroll(&self.prompt_scroll);
        let answer = match &dialog.state {
            ask::State::Idle => None,
            ask::State::Waiting(_) => Some(muted("◌ Thinking…".into()).into_any_element()),
            ask::State::Failed(error) => Some(
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                    .child(div().text_color(rgb(theme.text)).child(error.clone()))
                    .into_any_element(),
            ),
            ask::State::Answered { shown, .. } => Some(
                div()
                    .id("ask-answer")
                    .max_h(px(theme::ASK_ANSWER_HEIGHT))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .p(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .bg(rgb(theme.card))
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .children(preview::blocks(&shown.blocks, shown, theme))
                    .into_any_element(),
            ),
        };
        let answer_text = match &dialog.state {
            ask::State::Answered { text, .. } => Some(text.clone()),
            _ => None,
        };
        let can_ask = !dialog.waiting() && !dialog.question.is_blank() && selected.is_some();
        let ask_label = if dialog.waiting() { "Asking…" } else { "Ask" };
        let footer = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(
                div()
                    .flex_1()
                    .child(muted("Enter new line · Ctrl+Enter ask · Esc close".into())),
            )
            .children(answer_text.map(|text| {
                button("ask-copy", "Copy answer", theme, false)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Copy the answer as Markdown", ""))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    })
            }))
            .child(
                button("ask-close", "Close", theme, false)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Close the dialog", "Esc"))
                    .on_click(cx.listener(|shell, _, window, cx| shell.close_ask(window, cx))),
            )
            .child(if can_ask {
                button("ask-send", ask_label, theme, true)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Ask the question", "Ctrl+Enter"))
                    .on_click(cx.listener(|shell, _, window, cx| shell.send_ask(window, cx)))
            } else {
                disabled_button("ask-send", ask_label, theme, true).tooltip(tooltip(
                    theme,
                    "Type a question",
                    "",
                ))
            });
        let panel = div()
            .id("ask-dialog")
            .w(px(theme::ASK_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::ask_key_down),
            )
            .relative()
            .child(self.typing(&self.dialog_focus, Self::ask_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Ask AI"),
            )
            .child(muted(
                "Any question, outside your projects: the agent runs in an empty folder.".into(),
            ))
            .child(agents)
            .children(web)
            .child(field)
            .children(answer)
            .child(footer);
        Self::modal_shade("ask-shade", panel, cx)
    }
}
