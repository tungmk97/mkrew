//! The composer (agent input) and the prompt picker.

use crate::*;
use mkrew_ui::chat;

impl Shell {
    /// The focused pane's session while its composer is shown.
    pub(crate) fn composer_session(&self, window: &Window) -> Option<Ulid> {
        self.focused_session(window)
            .filter(|&session| self.composers.shown(session).is_some())
    }

    /// The focused pane's session when it runs an agent.
    pub(crate) fn focused_agent(&self, window: &Window) -> Option<Ulid> {
        self.focused_session(window).filter(|&id| {
            self.sidebar
                .sessions()
                .iter()
                .any(|s| s.id == id && s.kind == SessionKind::Agent)
        })
    }

    /// Shows the focused agent pane's composer (keeping its draft) and gives it the keyboard.
    pub(crate) fn open_composer(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(session) = self.focused_agent(window) else {
            return;
        };
        self.composers.open(session);
        self.composer_scroll.scroll_to_bottom();
        window.focus(&self.composer_focus, cx);
        cx.notify();
    }

    /// Keys typed in the composer ([`Shell::prompt_key_down`]): Enter is a new line, Ctrl+Enter
    /// sends, Esc hides it; in Chat Enter sends, Shift+Enter is a new line, Esc goes to the chat
    /// list.
    pub(crate) fn composer_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(session) = self.composer_session(window) else {
            return;
        };
        // In Chat a plain Enter sends (the chat convention); Shift+Enter falls through to the
        // new line. A picker that is open has the keys itself, so this never sees its Enter.
        let chat = self.chat_session(session);
        if chat && composer::sends_on_enter(&event.keystroke) {
            self.send_composer(session, window, cx);
            return;
        }
        if self.prompt_key_down(composer::Target::Composer(session), event, window, cx) {
            return;
        }
        match task_dialog::dialog_key(&event.keystroke) {
            // Chat: Esc moves to the chat list (the composer stays); Terminal: Esc hides it.
            Some(DialogKey::Cancel) if chat => {
                window.focus(&self.chat_focus, cx);
                cx.notify();
            }
            Some(DialogKey::Cancel) => {
                self.composers.hide(session);
                self.focus_pane(window, cx);
                cx.notify();
            }
            // Ctrl+Enter; ignored while sending or rewriting (`begin_send`).
            _ => self.send_composer(session, window, cx),
        }
    }

    /// An image pasted into a prompt box (§20B.5): saved through `attachment.save` (a BMP as
    /// PNG), then its path is typed into the box. A composer's image belongs to its session,
    /// the task dialog's to its project. A failure is a toast, like a pane's paste.
    pub(crate) fn prompt_paste_image(
        &mut self,
        target: composer::Target,
        item: gpui::ClipboardItem,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let owner = match target {
            composer::Target::Composer(session) => {
                Some(mkrew_protocol::AttachmentOwner::Session(session))
            }
            composer::Target::TaskDialog => self
                .task_dialog
                .as_ref()
                .and_then(TaskDialog::attachment_owner),
        };
        let Some(owner) = owner else {
            return;
        };
        let Some((ext, data)) = item.into_entries().find_map(|entry| match entry {
            gpui::ClipboardEntry::Image(image) => composer::attachment(image),
            _ => None,
        }) else {
            return;
        };
        if let Some(message) = composer::attachment_refusal(data.len()) {
            self.toast_error(message, cx);
            return;
        }
        let serial = self.dialog_serial;
        if target == composer::Target::TaskDialog
            && let Some(dialog) = self.task_dialog.as_mut()
        {
            dialog.images_saving += 1;
        }
        let saved = connection.call::<mkrew_protocol::AttachmentSaveResult>(
            Request::AttachmentSave(mkrew_protocol::AttachmentSaveParams {
                owner,
                ext: ext.to_owned(),
                data,
            }),
        );
        cx.spawn_in(window, async move |this, cx| {
            let saved = saved.await;
            let _ = this.update(cx, |shell, cx| {
                // The dialog it was pasted into may be gone, or another one open.
                let dialog_open = target == composer::Target::TaskDialog;
                if dialog_open {
                    let Some(dialog) = shell.open_dialog(serial) else {
                        return;
                    };
                    dialog.images_saving = dialog.images_saving.saturating_sub(1);
                }
                match saved {
                    Ok(saved) => {
                        // Typed where the caret is, without taking the keyboard from wherever
                        // the user went meanwhile.
                        let text = composer::typed_paths([saved.path.as_str()]);
                        match target {
                            composer::Target::Composer(session) => {
                                shell.composers.insert(session, &text);
                                shell.composer_scroll.scroll_to_bottom();
                            }
                            composer::Target::TaskDialog => {
                                if let Some(dialog) = shell.task_dialog.as_mut() {
                                    dialog.prompt.insert(&text);
                                    if dialog.field == Field::Prompt {
                                        shell.prompt_scroll.scroll_to_bottom();
                                    }
                                }
                            }
                        }
                        cx.notify();
                    }
                    Err(error) => {
                        tracing::warn!(?target, "attachment.save failed: {error:#}");
                        shell.toast_error(format!("Cannot attach the image: {error:#}"), cx);
                    }
                }
            });
        })
        .detach();
    }

    /// The prompt `target` while it is on screen: the shown composer's draft or the open task
    /// dialog's prompt.
    pub(crate) fn prompt(&self, target: composer::Target) -> Option<&composer::Prompt> {
        match target {
            composer::Target::Composer(session) => {
                self.composers.shown(session).map(|draft| &draft.prompt)
            }
            composer::Target::TaskDialog => self.task_dialog.as_ref().map(|dialog| &dialog.prompt),
        }
    }

    /// Types `text` (a pick, a drop, an `@`) into the prompt `target` and gives it the
    /// keyboard back.
    pub(crate) fn prompt_typed(
        &mut self,
        target: composer::Target,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match target {
            composer::Target::Composer(session) => {
                self.composers.insert(session, text);
                self.composer_scroll.scroll_to_bottom();
                if self.composer_session(window) == Some(session) {
                    window.focus(&self.composer_focus, cx);
                } else {
                    self.focus_pane(window, cx);
                }
            }
            composer::Target::TaskDialog => {
                if let Some(dialog) = self.task_dialog.as_mut() {
                    dialog.insert_prompt(text);
                    self.prompt_scroll.scroll_to_bottom();
                }
                // The open dialog takes the keyboard.
                self.focus_pane(window, cx);
            }
        }
    }

    /// ✦ (SPEC §20B.1) in a composer or the task dialog: the agent rewrites the prompt,
    /// reading the code in the project folder if it needs to; the answer arrives as
    /// `ai.generated`. A composer uses its session's agent, the task dialog the task's agent
    /// (the workspace default when none is picked).
    pub(crate) fn enhance_prompt(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let generation_id = Ulid::new();
        // ponytail: the project folder even for a worktree session or task; same code, other
        // branch.
        let (project_id, agent_profile, input) = match target {
            composer::Target::Composer(session) => {
                let Some(info) = self.sidebar.sessions().iter().find(|s| s.id == session) else {
                    return;
                };
                let (project_id, agent_profile) = (info.project_id, info.agent_profile.clone());
                let Some(input) = self.composers.begin_enhance(session, generation_id) else {
                    return;
                };
                (project_id, Some(agent_profile), input)
            }
            composer::Target::TaskDialog => {
                let Some(dialog) = self.task_dialog.as_mut() else {
                    return;
                };
                let Some(input) = dialog.begin_enhance(generation_id) else {
                    return;
                };
                (dialog.project_id(), dialog.agent.clone(), input)
            }
        };
        cx.notify();
        let started = connection.call::<()>(Request::AiGenerate(AiGenerateParams {
            generation_id,
            project_id: Some(project_id),
            worktree_task_id: None,
            repo: None,
            agent_profile,
            kind: AiKind::EnhancePrompt,
            input,
            web: false,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let Err(error) = started.await else {
                return;
            };
            let _ = this.update_in(cx, |shell, _, cx| {
                shell.prompt_enhanced(generation_id, Err(format!("{error:#}")));
                cx.notify();
            });
        })
        .detach();
    }

    /// The ✦ answer of generation `generation_id` goes to the prompt that asked for it (a
    /// composer or the task dialog). `false` if none did.
    pub(crate) fn prompt_enhanced(
        &mut self,
        generation_id: Ulid,
        result: Result<String, String>,
    ) -> bool {
        if let Some(dialog) = self.task_dialog.as_mut()
            && dialog.prompt.enhancing == Some(generation_id)
        {
            return dialog.enhanced(generation_id, result);
        }
        self.composers.enhanced(generation_id, result)
    }

    /// The connection closed: the `ai.generated` of a ✦ in flight will never arrive, so the
    /// prompts waiting for one (composers, the kept task dialog) take keys again and can be
    /// saved; their text stays.
    pub(crate) fn abandon_rewrites(&mut self) {
        const LOST: &str = "the connection to the daemon closed";
        self.composers.abandon_rewrites(LOST);
        if let Some(dialog) = self.task_dialog.as_mut()
            && let Some(generation_id) = dialog.prompt.enhancing
        {
            dialog.enhanced(generation_id, Err(LOST.to_owned()));
        }
    }

    /// `session.send` of `session`'s draft with Enter (one bracketed paste when multi-line,
    /// §7); the draft empties once it is written or queued.
    pub(crate) fn send_composer(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.composers.begin_send(session) else {
            return;
        };
        cx.notify();
        let sent = connection.request(Request::SessionSend(params));
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%session, "session.send failed: {error}");
                }
                shell.composers.sent(session, result);
                cx.notify();
            });
        })
        .detach();
    }

    /// The prompt variables known for `project_id` (§20B.3): its name, `task` (a task title)
    /// and the file open in its Files view, else the one selected in its Git view.
    pub(crate) fn prompt_vars(&self, project_id: Ulid, task: Option<String>) -> PromptVars {
        let project = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.name.clone());
        let file = self
            .files
            .get(&project_id)
            .and_then(|view| view.file().map(str::to_owned))
            .or_else(|| {
                self.git
                    .get(&project_id)
                    .and_then(|view| view.selected.as_ref())
                    .map(|target| target.path.clone())
            });
        // The shown view's pick first (one picked earlier elsewhere may linger).
        let views = if self.view() == View::Diff {
            [View::Diff, View::Editor]
        } else {
            [View::Editor, View::Diff]
        };
        let selection = views
            .into_iter()
            .find_map(|view| self.picked_lines(project_id, view))
            .map(|(_, _, _, text)| text);
        PromptVars {
            file,
            selection,
            task,
            project,
        }
    }

    /// The lines picked in `project_id`'s Editor view (a Markdown preview's selection, or the
    /// editor's lines, §12A), Docs view (its comment selection) or Git diff (new side): the
    /// file, first and last line, and their text (§20B.4).
    pub(crate) fn picked_lines(
        &self,
        project_id: Ulid,
        view: View,
    ) -> Option<(String, u32, u32, String)> {
        match view {
            View::Editor => self.files.get(&project_id)?.tab()?.picked_lines(),
            View::Diff => {
                let git = self.git.get(&project_id)?;
                let (Some(target), Some(selection), git_view::Diff::Shown(shown)) =
                    (&git.selected, git.comments.selection, &git.diff)
                else {
                    return None;
                };
                let text = shown.new_side_text(selection)?;
                Some((target.path.clone(), selection.start, selection.end, text))
            }
            View::Docs => self.docs.get(&project_id)?.picked_lines(),
            _ => None,
        }
    }

    /// Opens the prompt picker typing into `target`; the library is read (seeded the first
    /// time) now, so edited prompt files show without a restart.
    pub(crate) fn open_prompt_picker(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let vars = match target {
            composer::Target::Composer(session) => {
                let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == session) else {
                    return;
                };
                let task = session
                    .task_id
                    .and_then(|id| self.board.task(id))
                    .map(|task| task.title.clone());
                self.prompt_vars(session.project_id, task)
            }
            composer::Target::TaskDialog => {
                let Some(dialog) = &self.task_dialog else {
                    return;
                };
                let title = dialog.title.trim();
                let task = (!title.is_empty()).then(|| title.to_owned());
                self.prompt_vars(dialog.project_id(), task)
            }
        };
        let library = mkrew_core::prompt::load_or_seed(&self.prompts_dir);
        for error in &library.errors {
            tracing::warn!(file = %error.file.display(), "cannot read prompt: {}", error.message);
        }
        self.workspace_menu = None;
        self.prompt_picker = Some(PromptPicker::new(target, library, vars));
        self.picker_host = self.host_of(window);
        self.prompt_picker_scroll.scroll_to_item(0);
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// Closes the prompt picker: `text` (the rendered pick) is typed into its target, which
    /// gets the keyboard back.
    pub(crate) fn close_prompt_picker(
        &mut self,
        text: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(picker) = self.prompt_picker.take() else {
            return;
        };
        self.picker_host = HostId::Main;
        match (picker.target, text) {
            (target, Some(text)) => {
                let before = self.prompt(target).map_or("", |prompt| prompt.text.text());
                let text = prompt_picker::on_new_line(before, &text);
                self.prompt_typed(target, &text, window, cx);
            }
            (composer::Target::Composer(session), None)
                if self.composer_session(window) == Some(session) =>
            {
                window.focus(&self.composer_focus, cx);
            }
            _ => self.focus_pane(window, cx),
        }
        cx.notify();
    }

    /// ↑ / ↓ move the pick (kept in view), Enter types it, Esc closes.
    pub(crate) fn prompt_picker_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = palette::key(&event.keystroke) else {
            return;
        };
        let Some(picker) = self.prompt_picker.as_mut() else {
            return;
        };
        match key {
            palette::Key::Close => self.close_prompt_picker(None, window, cx),
            palette::Key::Open => self.insert_prompt_pick(window, cx),
            palette::Key::Move(by) => {
                picker.step(by);
                self.prompt_picker_scroll.scroll_to_item(picker.selected);
                cx.notify();
            }
            palette::Key::Paste | palette::Key::Insert(_) | palette::Key::Edit(_) => {}
        }
    }

    /// Enter or a click on the pick: types the picked prompt, rendered, into the picker's
    /// target; on the last row it opens Settings → Prompts instead.
    pub(crate) fn insert_prompt_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(picker) = self.prompt_picker.as_ref() else {
            return;
        };
        let (manage, rendered) = (picker.on_manage(), picker.preview());
        if manage {
            self.close_prompt_picker(None, window, cx);
            self.open_prompts_settings(window, cx);
        } else if let Some(rendered) = rendered {
            self.close_prompt_picker(Some(rendered.text), window, cx);
        }
    }

    /// `session.unqueue`: the queued text is not typed; the `session.queue` event that follows
    /// updates the list (a text written meanwhile fails with `not_found` and leaves it too).
    pub(crate) fn unqueue(&mut self, session: Ulid, send_id: Ulid, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let removed = connection.request(Request::SessionUnqueue(SessionUnqueueParams {
            session_id: session,
            send_id,
        }));
        cx.spawn(async move |this, cx| {
            let failure = match removed.await {
                Ok(Outcome::Result(_)) => return,
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%session, "session.unqueue failed: {}", body.message);
                    composer::unqueue_failure(&body)
                }
                Err(error) => {
                    tracing::warn!(%session, "session.unqueue failed: {error:#}");
                    Some(format!("Cannot remove the queued text: {error:#}"))
                }
            };
            if let Some(message) = failure {
                let _ = this.update(cx, |shell, cx| shell.toast_error(message, cx));
            }
        })
        .detach();
    }

    /// The prompt picker (§20B.3): the library's prompts (the pick ringed), the pick rendered
    /// with the known variables, a muted note naming the ones left empty; a click or Enter
    /// types it.
    pub(crate) fn render_prompt_picker(
        &self,
        picker: &PromptPicker,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        // One frame for every row: the pick has a focus-colored ring, so it stays visible under
        // hover.
        let frame = |id: gpui::ElementId, picked: bool| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::M))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .border(px(theme::BORDER_WIDTH))
                .border_color(if picked {
                    gpui::Hsla::from(rgb(theme.pane_focus_border))
                } else {
                    gpui::transparent_black()
                })
                .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                .pressable_unless(picked, Look::Row, theme)
        };
        let mut rows: Vec<gpui::AnyElement> = picker
            .library
            .prompts
            .iter()
            .enumerate()
            .map(|(index, prompt)| {
                let picked = index == picker.selected;
                frame(("prompt-entry", index).into(), picked)
                    .on_press(
                        true,
                        cx.listener(move |shell, _, window, cx| {
                            if let Some(picker) = shell.prompt_picker.as_mut() {
                                picker.pick(index);
                                shell.insert_prompt_pick(window, cx);
                            }
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(prompt.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(prompt.name.clone()),
                    )
                    .into_any_element()
            })
            .collect();
        let empty = rows.is_empty();
        rows.push(
            frame("prompt-manage".into(), picker.on_manage())
                .on_press(
                    true,
                    cx.listener(|shell, _, window, cx| {
                        if let Some(picker) = shell.prompt_picker.as_mut() {
                            let manage = picker.manage_row();
                            picker.pick(manage);
                            shell.insert_prompt_pick(window, cx);
                        }
                    }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(rgb(theme.accent_text))
                        .child("Manage prompts…"),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("Settings → Prompts"),
                )
                .into_any_element(),
        );
        let preview = picker.preview();
        let note = preview
            .as_ref()
            .and_then(|rendered| prompt_picker::missing_note(&rendered.missing));
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let unreadable = picker.library.errors.len();
        let into = if picker.on_manage() {
            "Enter opens Settings → Prompts · Esc closes"
        } else {
            match picker.target {
                composer::Target::Composer(_) => {
                    "↑ ↓ to pick · Enter types it in the composer · Esc closes"
                }
                composer::Target::TaskDialog => {
                    "↑ ↓ to pick · Enter adds it to the task prompt · Esc closes"
                }
            }
        };
        let panel = floating_panel("prompt-picker", theme)
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::prompt_picker_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Insert prompt"),
            )
            .when(empty, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .child(div().flex_1().min_w_0().child(muted(
                            "No prompts yet: use Manage prompts… below, or add Markdown files to the prompts folder."
                                .to_owned(),
                        )))
                        .child(
                            button(
                                "prompt-open-dir",
                                icon_text(
                                    Icon::FolderOpen,
                                    "Open prompts folder",
                                    button_fg(theme, true),
                                ),
                                theme,
                                true,
                            )
                            .flex_none()
                            .on_click(cx.listener(
                                |shell, _, window, cx| {
                                    shell.open_prompts_dir(window, cx);
                                },
                            )),
                        ),
                )
            })
            .child(
                div()
                    .id("prompt-entries")
                    .max_h(px(theme::PICKER_LIST_HEIGHT))
                    .overflow_y_scroll()
                    .track_scroll(&self.prompt_picker_scroll)
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .children(rows),
            )
            .children(preview.map(|rendered| {
                // No border: the raised surface sets the preview apart (§14).
                div()
                    .id("prompt-preview")
                    .max_h(px(theme::PROMPT_FIELD_HEIGHT))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .px(px(space::S))
                    .py(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .bg(rgb(theme.surface_raised))
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .children(rendered.text.split('\n').map(|line| {
                        // An empty line still takes a line's height.
                        div().child(if line.is_empty() { " " } else { line }.to_owned())
                    }))
            }))
            .children(note.map(muted))
            .when(unreadable > 0, |el| {
                el.child(
                    div()
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .child(div().text_color(rgb(theme.status_needs_input)).child("⚠"))
                        .child(muted(format!(
                            "{unreadable} prompt file(s) could not be read (see the log)."
                        ))),
                )
            })
            .child(muted(into.to_owned()));
        Self::modal_shade("prompt-picker-shade", panel, cx)
    }

    /// The texts waiting for the agent, first line each, ✕ removes one (§20B.2).
    fn composer_queue_rows(
        &self,
        session: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::Div> {
        self.composers
            .queue(session)
            .iter()
            .map(|item| {
                let send_id = item.send_id;
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.text_muted))
                            .child("⧗"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text))
                            .child(composer::first_line(&item.text).to_owned()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("unqueue-{send_id}")))
                            .flex_none()
                            .flex()
                            .items_center()
                            .px(px(space::S))
                            .rounded(px(radius::SMALL))
                            .text_color(rgb(theme.text_muted))
                            .pressable(Look::Icon, true, theme)
                            .tooltip(tooltip(theme, "Remove from the queue (not sent)", ""))
                            .on_press(
                                true,
                                cx.listener(move |shell, _, _, cx| {
                                    cx.stop_propagation();
                                    shell.unqueue(session, send_id, cx);
                                }),
                            )
                            .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted)),
                    )
            })
            .collect()
    }

    /// The line that replaces the stats: sending, improving, an error, else (Terminal only) the
    /// key hint while the composer has the keyboard.
    fn composer_status(
        &self,
        draft: &Draft,
        focused: bool,
        with_hint: bool,
        theme: &Theme,
    ) -> Option<(&'static str, u32, String)> {
        if draft.busy {
            Some(("↑", theme.text_muted, "Sending…".to_owned()))
        } else if draft.prompt.rewriting() {
            Some(("✦", theme.text_muted, "Improving the prompt…".to_owned()))
        } else if let Some(error) = &draft.error {
            Some(("✕", theme.status_exited, error.clone()))
        } else {
            (focused && with_hint).then(|| ("", theme.text_muted, composer::hint().to_owned()))
        }
    }

    /// The ✦ and "Prompts…" buttons of the chip or key row (§20B.1, §20B.3). `compact`
    /// (the Chat card) shows Prompts as an icon only, to leave room for the chips.
    #[allow(clippy::too_many_arguments)]
    fn composer_tools(
        &self,
        session: Ulid,
        draft: &Draft,
        is_focused: bool,
        hover: u32,
        compact: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let rewriting = draft.prompt.rewriting();
        let mut tools = Vec::new();
        if is_focused && !draft.busy && !draft.prompt.text.is_blank() {
            let sparkles = icon_svg(Icon::Sparkles, theme::ICON_SIZE_SMALL, theme.text_muted);
            let improve = if rewriting {
                disabled_button("composer-enhance", sparkles, theme, false)
            } else {
                quiet_button("composer-enhance", sparkles, theme, hover).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        shell.enhance_prompt(composer::Target::Composer(session), window, cx)
                    },
                ))
            };
            tools.push(
                improve
                    .flex_none()
                    .tooltip(tooltip(
                        theme,
                        "Improve this prompt with the agent (it may read the code)",
                        "",
                    ))
                    .into_any_element(),
            );
        }
        if is_focused && !draft.busy {
            let prompts = quiet_button(
                "composer-prompts",
                if compact {
                    icon_svg(Icon::Library, theme::ICON_SIZE_SMALL, theme.text_muted)
                        .into_any_element()
                } else {
                    icon_text(Icon::Library, "Prompts…", theme.text_muted).into_any_element()
                },
                theme,
                hover,
            )
            .flex_none()
            .tooltip(tooltip(
                theme,
                "Insert a prompt from the library (type / in the box for the agent's commands)",
                "",
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_prompt_picker(composer::Target::Composer(session), window, cx);
            }));
            tools.push(prompts.into_any_element());
        }
        tools
    }

    /// The model chip: the agent's own model picker opens in its terminal. The pane switches to
    /// Terminal and `/model` + Enter is sent (ADR 0105). A working agent queues it (SPEC §8).
    pub(crate) fn open_model_picker(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.set_chat(session, false, window, cx);
        self.send_to_agent(session, "/model", window, cx);
    }

    /// `session.send` of `text` with Enter, outside the composer's draft; a failure is a toast.
    pub(crate) fn send_to_agent(
        &mut self,
        session: Ulid,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let sent = connection.request(Request::SessionSend(SessionSendParams {
            session_id: session,
            text: text.to_owned(),
            submit: true,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            if let Err(error) = result {
                tracing::warn!(%session, "session.send failed: {error}");
                let _ = this.update(cx, |shell, cx| {
                    shell.toast_error(format!("Cannot send: {error}"), cx);
                });
            }
        })
        .detach();
    }

    /// The composer docked under `session`'s pane (§20B.1). In Chat mode it is the floating
    /// card (`composer_card`, ADR 0105); in Terminal mode the docked box: the draft,
    /// then why the last send failed or the keys. Only the focused pane's composer takes the
    /// keyboard (`focused`: it has it now); a click on another pane's composer focuses that
    /// pane and its composer. `width`: the pane's width in px, `now`: epoch ms.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn composer_panel(
        &self,
        session: Ulid,
        draft: &Draft,
        is_focused: bool,
        focused: bool,
        width: f32,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        if self.chat_session(session) {
            return self.composer_card(session, draft, is_focused, focused, width, now, theme, cx);
        }
        let rewriting = draft.prompt.rewriting();
        let field = caret_field(
            "composer",
            draft.prompt.text.input(),
            "Message the agent…",
            focused && !draft.busy && !rewriting,
            theme,
            cx,
            move |shell, caret| {
                shell
                    .composers
                    .click(session, caret.at, caret.extend, caret.clicks);
            },
        )
        .max_h(px(theme::COMPOSER_MAX_HEIGHT))
        .overflow_y_scroll()
        .when(draft.busy || rewriting, |el| el.opacity(opacity::DISABLED))
        .when(is_focused, |el| {
            el.track_focus(&self.composer_focus)
                .track_scroll(&self.composer_scroll)
                .on_key_down(cx.listener(Self::composer_key_down))
                .relative()
                .child(self.typing(&self.composer_focus, Self::composer_key_down, cx))
        });
        let status = self.composer_status(draft, focused, true, theme);
        let queued = self.composer_queue_rows(session, theme, cx);
        let tools = self.composer_tools(
            session,
            draft,
            is_focused,
            theme.surface_raised,
            false,
            theme,
            cx,
        );
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::S))
            .bg(rgb(theme.surface))
            .font_family(theme.ui_font_family.clone())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.focus_session_pane(session);
                    window.focus(&shell.composer_focus, cx);
                    cx.notify();
                }),
            )
            .when(!queued.is_empty(), |el| {
                el.child(
                    div()
                        .id("composer-queue")
                        .flex_none()
                        .max_h(px(theme::COMPOSER_QUEUE_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(queued),
                )
            })
            .child(field)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .children(status.map(|(icon, color, text)| {
                                div()
                                    .flex()
                                    .min_w_0()
                                    .gap(px(space::XS))
                                    .text_size(font_size::META)
                                    .when(!icon.is_empty(), |el| {
                                        el.child(
                                            div().flex_none().text_color(rgb(color)).child(icon),
                                        )
                                    })
                                    .child(
                                        // A narrow pane cuts the key hint with an ellipsis
                                        // instead of drawing it under the buttons; an error
                                        // wraps, so it is read in full.
                                        div()
                                            .min_w_0()
                                            .when(draft.error.is_none(), |el| el.truncate())
                                            .text_color(rgb(if draft.error.is_some() {
                                                theme.text
                                            } else {
                                                theme.text_muted
                                            }))
                                            .child(text),
                                    )
                            })),
                    )
                    .children(tools),
            )
            .into_any_element()
    }

    /// The Chat composer (ADR 0105): in the page's centered column, `space::M` above the pane's
    /// bottom edge, a thin context meter over a floating card (the queue, the field, then the
    /// chip row: model, effort, permission, branch, stats or the status line, ✦, Prompts…, and
    /// the round send button). Collapsed to a one-line pill while the pane has no keyboard, the
    /// draft is empty and nothing is queued or failed.
    #[allow(clippy::too_many_arguments)]
    fn composer_card(
        &self,
        session: Ulid,
        draft: &Draft,
        is_focused: bool,
        focused: bool,
        width: f32,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let info = self.sidebar.sessions().iter().find(|s| s.id == session);
        let page = self
            .reads
            .get(&session)
            .and_then(|read| read.page.as_deref());
        let placeholder = chat::composer_placeholder(
            page.and_then(|page| page.meta.title.as_deref()),
            info.and_then(mkrew_ui::history::title),
        );
        let blank = draft.prompt.text.is_blank();
        let queue = self.composers.queue(session);
        if !is_focused && blank && queue.is_empty() && !draft.busy && draft.error.is_none() {
            let pill = div()
                .id(SharedString::from(format!("composer-pill-{session}")))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::L))
                .py(px(space::S))
                .rounded(px(radius::PILL))
                .bg(rgb(theme.surface_raised))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::rgba(theme.glass_border))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .font_family(theme.ui_font_family.clone())
                .pressable(Look::Raised, true, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, window, cx| {
                        shell.focus_session_pane(session);
                        window.focus(&shell.composer_focus, cx);
                        cx.notify();
                    }),
                )
                .child(icon_svg(
                    Icon::Sparkles,
                    theme::ICON_SIZE_SMALL,
                    theme.text_subtle,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .italic()
                        .child(placeholder),
                )
                .child(icon_svg(
                    Icon::ArrowUp,
                    theme::ICON_SIZE_SMALL,
                    theme.text_muted,
                ));
            return super::chat_view::chat_column(pill)
                .pb(px(space::M))
                .into_any_element();
        }
        let working = info.is_some_and(|s| s.state == SessionState::Working);
        let at_root = self
            .link_scope(session)
            .is_some_and(|(_, target, _)| target.is_none());
        let stats = chat::composer_stats(
            page,
            self.usages.get(session),
            self.git_summary(),
            at_root,
            working,
            now,
            &super::widget_bar::local_offset,
        );
        let rewriting = draft.prompt.rewriting();
        let can_send = !draft.busy && !rewriting && !blank;
        let input = draft.prompt.text.input();
        let caret = focused && !draft.busy && !rewriting;
        let (text, layout) = mkrew_ui::field::caret_text(
            input.as_str(),
            input.cursor(),
            input.selection(),
            placeholder_row(placeholder, theme),
            caret,
            theme,
        );
        let field = mkrew_ui::field::caret_mouse(
            div()
                .id("composer")
                .relative()
                .cursor(CursorStyle::IBeam)
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .child(text),
            &cx.entity(),
            layout,
            move |shell: &mut Shell, caret| {
                shell
                    .composers
                    .click(session, caret.at, caret.extend, caret.clicks);
            },
        )
        .max_h(px(theme::COMPOSER_MAX_HEIGHT))
        .overflow_y_scroll()
        .when(draft.busy || rewriting, |el| el.opacity(opacity::DISABLED))
        .when(is_focused, |el| {
            el.track_focus(&self.composer_focus)
                .track_scroll(&self.composer_scroll)
                .on_key_down(cx.listener(Self::composer_key_down))
                .child(self.typing(&self.composer_focus, Self::composer_key_down, cx))
        });
        let meter = stats
            .context
            .and_then(|(used, window)| chat::context_meter(used, window?))
            .map(|meter| {
                let fill = if meter.warning {
                    theme.status_needs_input
                } else {
                    theme.accent
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .pb(px(space::XS))
                    .child(
                        div()
                            .h(px(theme::CONTEXT_METER_HEIGHT))
                            .w_full()
                            .rounded(px(radius::PILL))
                            .bg(rgb(theme.divider))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(meter.fraction))
                                    .rounded(px(radius::PILL))
                                    .bg(rgb(fill)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{}%", meter.percent))
                            .child(meter.window_label),
                    )
            });
        // Room for the chips: the pane's width, capped by the column, less the page padding
        // and the card's own padding. The chip widths are in rems (`theme::rems_px`, designed
        // at 13 px), so the room is measured in the same unit.
        let room = (width.min(theme::CHAT_COLUMN_WIDTH) - 2.0 * space::XL - 2.0 * space::M)
            / (cx.global::<theme::Fonts>().ui_size / 13.0);
        let shown = chat::fit_chips(chat::Chips::wanted(&stats), room);
        let mono = theme.terminal_font_family.clone();
        let glyph = |icon: Icon, color: u32| icon_svg(icon, theme::ICON_SIZE_SMALL, color);
        let quiet = |el: gpui::Div| {
            el.flex()
                .flex_none()
                .items_center()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
        };
        let exited = self.is_exited(session);
        let model_chip = stats.model.clone().filter(|_| shown.model).map(|model| {
            div()
                .id("composer-model")
                .flex()
                .flex_none()
                .items_center()
                .gap(px(space::XS))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::PILL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme::raised_border(theme)))
                .text_size(font_size::META)
                .text_color(rgb(theme.accent_text))
                .pressable(Look::Chip, !exited, theme)
                .tooltip(tooltip(
                    theme,
                    "Change the model (types /model in the terminal)",
                    "",
                ))
                .on_press(
                    !exited,
                    cx.listener(move |shell, _, window, cx| {
                        shell.open_model_picker(session, window, cx);
                    }),
                )
                .when(working, |el| {
                    el.child(loading_dots(now, rgb(theme.accent_text)))
                })
                .child(model)
                .child(glyph(Icon::ChevronDown, theme.text_muted))
        });
        let permission_chip =
            stats
                .permission
                .clone()
                .filter(|_| shown.permission)
                .map(|permission| {
                    quiet(div())
                        .text_color(rgb(theme.status_needs_input))
                        .child(glyph(Icon::Zap, theme.status_needs_input))
                        .child(permission)
                });
        let effort_chip = stats
            .effort
            .clone()
            .filter(|_| shown.effort)
            .map(|effort| quiet(div()).child(effort));
        let branch_chip = stats.branch.clone().filter(|_| shown.branch).map(|branch| {
            let name = match branch.changed {
                0 => branch.name,
                changed => format!("{} *{changed}", branch.name),
            };
            quiet(div())
                .font_family(mono.clone())
                .child(glyph(Icon::GitBranch, theme.text_muted))
                .child(div().text_color(rgb(theme.syntax_string)).child(name))
        });
        // The status line replaces the stats and the end clock while it shows.
        let status = self.composer_status(draft, focused, false, theme);
        let middle = match status {
            Some((icon, color, text)) => div()
                .flex()
                .min_w_0()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .when(!icon.is_empty(), |el| {
                    el.child(div().flex_none().text_color(rgb(color)).child(icon))
                })
                .child(
                    div()
                        .min_w_0()
                        .when(draft.error.is_none(), |el| el.truncate())
                        .text_color(rgb(if draft.error.is_some() {
                            theme.text
                        } else {
                            theme.text_muted
                        }))
                        .child(text),
                )
                .into_any_element(),
            None => {
                let context = stats.context.map(|(used, _)| {
                    quiet(div())
                        .child(glyph(Icon::Database, theme.text_muted))
                        .child(
                            div()
                                .text_color(rgb(theme.syntax_keyword))
                                .child(chat::format_k(used)),
                        )
                });
                let rate = stats.tok_s.map(|rate| {
                    quiet(div())
                        .text_color(rgb(theme.text))
                        .child(format!("{rate} tok/s"))
                });
                let elapsed = stats.elapsed_ms.map(|ms| {
                    quiet(div())
                        .child(glyph(Icon::Clock, theme.text_muted))
                        .child(chat::format_elapsed(ms))
                });
                let ended = stats.ended_clock.clone().filter(|_| shown.clock);
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .overflow_hidden()
                    .gap(px(space::S))
                    .font_family(mono.clone())
                    .when(shown.stats, |el| {
                        el.children(context).children(rate).children(elapsed)
                    })
                    .children(ended.map(|time| {
                        quiet(div())
                            .text_color(rgb(theme.text_subtle))
                            .child(glyph(Icon::Clock, theme.text_subtle))
                            .child(time)
                    }))
                    .into_any_element()
            }
        };
        let send = round_button("composer-send", Icon::CornerDownLeft, can_send, theme)
            .tooltip(tooltip(theme, composer::send_tip(), ""))
            .on_press(
                can_send,
                cx.listener(move |shell, _, window, cx| {
                    shell.send_composer(session, window, cx);
                }),
            );
        let card = div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .rounded(px(radius::LARGE))
            .bg(rgb(theme.surface_raised))
            .border(px(theme::BORDER_WIDTH))
            .border_color(gpui::rgba(theme.glass_border))
            .shadow(theme::shadow_float(theme))
            .font_family(theme.ui_font_family.clone())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.focus_session_pane(session);
                    window.focus(&shell.composer_focus, cx);
                    cx.notify();
                }),
            )
            .when(!queue.is_empty(), |el| {
                el.child(
                    div()
                        .id("composer-queue")
                        .flex_none()
                        .max_h(px(theme::COMPOSER_QUEUE_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(self.composer_queue_rows(session, theme, cx)),
                )
            })
            .child(field)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .children(model_chip)
                    .children(permission_chip)
                    .children(effort_chip)
                    .children(branch_chip)
                    .child(div().flex_1().min_w_0().flex().child(middle))
                    .children(self.composer_tools(
                        session,
                        draft,
                        is_focused,
                        theme.border,
                        true,
                        theme,
                        cx,
                    ))
                    .child(send),
            );
        super::chat_view::chat_column(div().flex().flex_col().children(meter).child(card))
            .pb(px(space::M))
            .into_any_element()
    }
}

/// The Chat field's placeholder: a small sparkle icon, then `text` in italic muted (Tern look).
/// `caret_text` draws the caret before it.
fn placeholder_row(text: String, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::S))
        .child(icon_svg(
            Icon::Sparkles,
            theme::ICON_SIZE_SMALL,
            theme.text_subtle,
        ))
        .child(
            div()
                .min_w_0()
                .italic()
                .text_color(rgb(theme.text_muted))
                .child(text),
        )
}
