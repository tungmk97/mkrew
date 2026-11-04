//! The `/` command picker over a prompt box (SPEC §20B.7, ADR 0094): opening it, asking the
//! daemon, keys and rendering. The model is [`slash_picker::SlashPicker`].

use crate::*;

impl Shell {
    /// What the picker for `target` lists: the composer's session agent in its project (its task
    /// worktree when the task has one), or the dialog's agent (§4.4 of the design spec).
    fn slash_key(&self, target: composer::Target) -> Option<SlashKey> {
        match target {
            composer::Target::Composer(session) => {
                let session = self.sidebar.sessions().iter().find(|s| s.id == session)?;
                let worktree = (session.task_id)
                    .and_then(|id| self.board.task(id))
                    .filter(|task| task.worktree_path.is_some())
                    .map(|task| task.id);
                Some((session.agent_profile.clone(), session.project_id, worktree))
            }
            composer::Target::TaskDialog => {
                let dialog = self.task_dialog.as_ref()?;
                let default_agent = (self.sidebar.projects().iter())
                    .find(|project| project.id == dialog.project_id())
                    .and_then(|project| project.default_agent.as_deref());
                dialog.slash_key(default_agent)
            }
        }
    }

    /// `/` in a prompt box: the picker over `target`, the cached rows at once and a fresh
    /// `slash.list`. With no agent to ask about, or no daemon, the `/` is typed as is.
    pub(crate) fn open_slash_picker(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(scope) = self.slash_key(target).filter(|_| self.connection.is_some()) else {
            self.prompt_typed(target, "/", window, cx);
            cx.notify();
            return;
        };
        let cached = self.slash_cache.get(&scope).cloned();
        self.workspace_menu = None;
        self.slash_picker = Some(SlashPicker::new(target, scope, cached));
        self.slash_picker_scroll.scroll_to_item(0);
        window.focus(&self.dialog_focus, cx);
        self.request_slash(window, cx);
        cx.notify();
    }

    /// `slash.list` for the open picker's scope. The answer fills the cache of that scope
    /// whichever picker is open by then; the picker takes it only if it is still the latest
    /// request (a stale answer is dropped).
    pub(crate) fn request_slash(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(picker) = self.slash_picker.as_mut() else {
            return;
        };
        let scope = picker.scope().clone();
        let (number, params) = picker.request();
        let listed = connection.call::<mkrew_protocol::SlashListResult>(Request::SlashList(params));
        cx.spawn_in(window, async move |this, cx| {
            let answer = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                match &answer {
                    Ok(result) => {
                        shell.slash_cache.insert(scope, result.items.clone());
                    }
                    Err(error) => {
                        tracing::warn!(profile = %scope.0, "slash.list failed: {error}");
                    }
                }
                if let Some(picker) = shell.slash_picker.as_mut()
                    && picker.answered(number, answer)
                {
                    shell.slash_picker_scroll.scroll_to_item(picker.selected);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The dialog's Agent changed while the picker is open: the new agent's cached rows replace
    /// the old ones (the filter stays) and its list is asked for.
    pub(crate) fn retarget_slash_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(picker) = self
            .slash_picker
            .as_ref()
            .filter(|picker| picker.target == composer::Target::TaskDialog)
        else {
            return;
        };
        let Some(scope) = self.slash_key(composer::Target::TaskDialog) else {
            return;
        };
        if &scope == picker.scope() {
            return;
        }
        let cached = self.slash_cache.get(&scope).cloned();
        if let Some(picker) = self.slash_picker.as_mut() {
            picker.retarget(scope, cached);
        }
        self.request_slash(window, cx);
        cx.notify();
    }

    /// Closes the picker: `typed` (a pick, or the `/` and filter Esc gives back) goes into its
    /// target, which gets the keyboard; `None` closes without typing.
    pub(crate) fn close_slash_picker(
        &mut self,
        typed: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(picker) = self.slash_picker.take() else {
            return;
        };
        match (typed, picker.target) {
            (Some(text), target) => self.prompt_typed(target, &text, window, cx),
            (None, composer::Target::Composer(session))
                if self.composer_session() == Some(session) =>
            {
                window.focus(&self.composer_focus, cx);
            }
            (None, _) => self.focus_pane(window, cx),
        }
        cx.notify();
    }

    /// Enter, Tab or a click: types the pick; a group row only expands (its skills are listed).
    pub(crate) fn accept_slash_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(picker) = self.slash_picker.as_mut() else {
            return;
        };
        match picker.accept() {
            Some(text) => self.close_slash_picker(Some(text), window, cx),
            None => {
                self.slash_picker_scroll.scroll_to_item(0);
                cx.notify();
            }
        }
    }

    /// "Open profile": the agent's profile file in Zed, to set `cli` there. A machine window's
    /// profile is on the other computer, so it is refused there.
    pub(crate) fn open_slash_profile(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(profile) = self
            .slash_picker
            .as_ref()
            .map(|picker| picker.scope().0.clone())
        else {
            return;
        };
        if self.local_only(cx) {
            return;
        }
        let path = self
            .config_dir
            .join("agents")
            .join(format!("{profile}.toml"));
        if let Err(error) = zed::open(&path, None) {
            tracing::warn!(path = %path.display(), "open agent profile failed: {error}");
            self.toast_error(error.to_string(), cx);
        }
        self.close_slash_picker(None, window, cx);
    }

    /// Typing filters; ↑ / ↓ move the pick (kept in view); Enter or Tab types it; Esc types the
    /// `/` and what was typed after it.
    pub(crate) fn slash_picker_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        // Tab picks like Enter: it is the completion key of the agents' own `/` popups.
        let key = if task_dialog::dialog_key(&event.keystroke) == Some(DialogKey::SwitchField) {
            Some(quick_open::Key::Open)
        } else {
            quick_open::key(&event.keystroke)
        };
        let Some(key) = key else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(picker) = self.slash_picker.as_mut() else {
            return;
        };
        match key {
            quick_open::Key::Close => {
                let typed = picker.escaped();
                return self.close_slash_picker(Some(typed), window, cx);
            }
            quick_open::Key::Open => return self.accept_slash_pick(window, cx),
            quick_open::Key::Move(_) => {
                picker.apply(key);
                self.slash_picker_scroll.scroll_to_item(picker.selected);
            }
            quick_open::Key::Paste => {
                if let Some(text) = paste {
                    picker.insert(&text);
                }
            }
            quick_open::Key::Edit(_) | quick_open::Key::Insert(_) => {
                if let quick_open::Key::Edit(edit) = &key {
                    copy_selection(edit, picker.query(), cx);
                }
                picker.apply(key);
            }
        }
        cx.notify();
    }
}

impl Shell {
    /// The picker: the filter field over the rows (`/name` in the mono font with its matched
    /// chars in the accent color, the argument hint and the one-line description muted, a source
    /// chip on the right, the pick ringed), then the note that says why there are none, the
    /// error line, and the footer. A click or Enter types the pick.
    pub(crate) fn render_slash_picker(
        &self,
        picker: &SlashPicker,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let hover = theme.surface_raised;
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let rows: Vec<gpui::AnyElement> = picker
            .rows()
            .iter()
            .enumerate()
            .filter_map(|(position, row)| {
                let item = picker.items().get(row.index)?;
                let picked = position == picker.selected;
                let name = format!("/{}", item.name);
                // The `/` comes first in the text, so the matched chars move one place.
                let shifted: Vec<u32> = row.matched.iter().map(|at| at + 1).collect();
                let highlights =
                    quick_open::match_ranges(&name, &shifted)
                        .into_iter()
                        .map(|range| {
                            let style = gpui::HighlightStyle {
                                color: Some(accent),
                                font_weight: Some(FontWeight(weight::SEMIBOLD)),
                                ..gpui::HighlightStyle::default()
                            };
                            (range, style)
                        });
                Some(
                    div()
                        .id(("slash-row", position))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        // The pick has a focus-colored ring, so it stays visible under hover.
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(if picked {
                            theme.pane_focus_border
                        } else {
                            theme.surface
                        }))
                        .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                        .cursor_pointer()
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            if let Some(picker) = shell.slash_picker.as_mut() {
                                picker.pick(position);
                            }
                            shell.accept_slash_pick(window, cx);
                        }))
                        .child(
                            div()
                                .flex_none()
                                .font_family(theme.terminal_font_family.clone())
                                .child(gpui::StyledText::new(name).with_highlights(highlights)),
                        )
                        .children(item.argument_hint.clone().map(|hint| {
                            div()
                                .flex_none()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(hint)
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(item.description.clone()),
                        )
                        // The group row (omp's `skill:`) stands for skills of many sources: no chip.
                        .children(item.group.is_none().then(|| {
                            div()
                                .flex_none()
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .border(px(theme::BORDER_WIDTH))
                                .border_color(rgb(theme.border))
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(slash_picker::source_label(&item.source))
                        }))
                        .into_any_element(),
                )
            })
            .collect();
        let note = picker.empty().map(|empty| match empty {
            slash_picker::Empty::Loading => {
                muted("Loading commands…".to_owned()).into_any_element()
            }
            slash_picker::Empty::UnknownCli => div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .child(
                    div().flex_1().min_w_0().child(muted(
                        "Mkrew doesn't know which CLI this agent runs. Set `cli` in its profile."
                            .to_owned(),
                    )),
                )
                .child(
                    button(
                        "slash-open-profile",
                        icon_text(Icon::FileText, "Open profile", button_fg(theme, true)),
                        theme,
                        true,
                    )
                    .flex_none()
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.open_slash_profile(window, cx);
                    })),
                )
                .into_any_element(),
            slash_picker::Empty::NoCommands => {
                muted("This agent lists no commands.".to_owned()).into_any_element()
            }
            slash_picker::Empty::NoMatch => muted(format!(
                "No commands match \"/{}\". Esc types it as typed.",
                picker.query().as_str().trim()
            ))
            .into_any_element(),
        });
        let failure = picker.error().map(|error| {
            div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                        .child(div().text_color(rgb(theme.text)).child(error.to_owned())),
                )
                .child(
                    button(
                        "slash-retry",
                        icon_text(Icon::RotateCw, "Try again", button_fg(theme, true)),
                        theme,
                        true,
                    )
                    .flex_none()
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.request_slash(window, cx);
                        cx.notify();
                    })),
                )
        });
        let keys = match picker.target {
            composer::Target::Composer(_) => {
                "↑ ↓ to pick · Enter or Tab types it in the composer · Esc types /"
            }
            composer::Target::TaskDialog => {
                "↑ ↓ to pick · Enter or Tab types it in the task prompt · Esc types /"
            }
        };
        let unreadable = picker.unreadable();
        let panel = div()
            .id("slash-picker")
            .w(px(theme::DIALOG_WIDTH))
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
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::slash_picker_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::slash_picker_key_down, cx))
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(format!("Insert command · {}", picker.scope().0)),
            )
            .child(caret_field(
                "slash-query",
                picker.query(),
                "Type part of a command name",
                true,
                theme,
                cx,
                |shell, caret| {
                    if let Some(picker) = shell.slash_picker.as_mut() {
                        picker.click_query(caret.at, caret.extend, caret.clicks);
                    }
                },
            ))
            .children(failure)
            .when(!rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("slash-rows")
                        .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.slash_picker_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .children(note)
            .child(muted(keys.to_owned()))
            .when(picker.cli() == Some(mkrew_protocol::Cli::Claude), |el| {
                el.child(muted(
                    "MCP prompts are not listed; type /mcp__server__prompt".to_owned(),
                ))
            })
            .when(!unreadable.is_empty(), |el| {
                el.child(
                    div()
                        .id("slash-unreadable")
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .tooltip(tooltip(theme, &unreadable.join("\n"), ""))
                        .child(div().text_color(rgb(theme.status_needs_input)).child("⚠"))
                        .child(muted(slash_picker::unreadable_note(unreadable.len()))),
                )
            });
        Self::modal_shade("slash-picker-shade", panel, cx)
    }
}
