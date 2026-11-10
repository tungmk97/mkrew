//! The composer (agent input) and the prompt picker.

use crate::*;

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
    /// sends, Esc hides it.
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
        if self.prompt_key_down(composer::Target::Composer(session), event, window, cx) {
            return;
        }
        match task_dialog::dialog_key(&event.keystroke) {
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

    /// The lines picked in `project_id`'s Editor view (one line, §12A) or Git diff (new side):
    /// the file, first and last line, and their text (§20B.4).
    pub(crate) fn picked_lines(
        &self,
        project_id: Ulid,
        view: View,
    ) -> Option<(String, u32, u32, String)> {
        match view {
            View::Editor => {
                let files = self.files.get(&project_id)?;
                let (Some(path), Some(shown)) = (files.file(), files.shown()) else {
                    return None;
                };
                // The selected lines, else the caret's line.
                let editor = &shown.editor;
                let (first, last) = match editor.selection() {
                    // A selection ending at a line's start does not include that line.
                    Some(range) if range.end.col == 0 => (range.start.line, range.end.line - 1),
                    Some(range) => (range.start.line, range.end.line),
                    None => (editor.cursor().line, editor.cursor().line),
                };
                let text = editor.lines().get(first..=last)?.join("\n");
                let line = |index: usize| u32::try_from(index + 1).ok();
                Some((path.to_owned(), line(first)?, line(last)?, text))
            }
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
        let Some(key) = quick_open::key(&event.keystroke) else {
            return;
        };
        let Some(picker) = self.prompt_picker.as_mut() else {
            return;
        };
        match key {
            quick_open::Key::Close => self.close_prompt_picker(None, window, cx),
            quick_open::Key::Open => self.insert_prompt_pick(window, cx),
            quick_open::Key::Move(by) => {
                picker.step(by);
                self.prompt_picker_scroll.scroll_to_item(picker.selected);
                cx.notify();
            }
            quick_open::Key::Paste | quick_open::Key::Insert(_) | quick_open::Key::Edit(_) => {}
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
        let hover = theme.surface_raised;
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
                .border_color(rgb(if picked {
                    theme.pane_focus_border
                } else {
                    theme.surface
                }))
                .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                .cursor_pointer()
                .active(|style| style.opacity(opacity::ACTIVE))
        };
        let mut rows: Vec<gpui::AnyElement> = picker
            .library
            .prompts
            .iter()
            .enumerate()
            .map(|(index, prompt)| {
                let picked = index == picker.selected;
                frame(("prompt-entry", index).into(), picked)
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        if let Some(picker) = shell.prompt_picker.as_mut() {
                            picker.pick(index);
                            shell.insert_prompt_pick(window, cx);
                        }
                    }))
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
                .on_click(cx.listener(|shell, _, window, cx| {
                    if let Some(picker) = shell.prompt_picker.as_mut() {
                        let manage = picker.manage_row();
                        picker.pick(manage);
                        shell.insert_prompt_pick(window, cx);
                    }
                }))
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
        let panel = div()
            .id("prompt-picker")
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
                    .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
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

    /// The composer docked under `session`'s pane (§20B.1): the draft, then why the last send
    /// failed or the keys. Only the focused pane's composer takes the keyboard (`focused`: it
    /// has it now); a click on another pane's composer focuses that pane and its composer.
    pub(crate) fn composer_panel(
        &self,
        session: Ulid,
        draft: &Draft,
        is_focused: bool,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
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
        // The keys only while it has the keyboard (they do nothing on other panes).
        let status = if draft.busy {
            Some(("↑", theme.text_muted, "Sending…".to_owned()))
        } else if rewriting {
            Some(("✦", theme.text_muted, "Improving the prompt…".to_owned()))
        } else if let Some(error) = &draft.error {
            Some(("✕", theme.status_exited, error.clone()))
        } else {
            focused.then(|| {
                (
                    "",
                    theme.text_muted,
                    "Ctrl+Enter to send · @ for a file · Esc to hide".to_owned(),
                )
            })
        };
        // The texts waiting for the agent (§20B.2), first line each, ✕ removes one.
        let hover = theme.border;
        let queued = self
            .composers
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
                            .cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .tooltip(tooltip(theme, "Remove from the queue (not sent)", ""))
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                cx.stop_propagation();
                                shell.unqueue(session, send_id, cx);
                            }))
                            .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted)),
                    )
            })
            .collect::<Vec<_>>();
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
                                    .gap(px(space::XS))
                                    .text_size(font_size::META)
                                    .when(!icon.is_empty(), |el| {
                                        el.child(div().text_color(rgb(color)).child(icon))
                                    })
                                    .child(
                                        div()
                                            .text_color(rgb(if draft.error.is_some() {
                                                theme.text
                                            } else {
                                                theme.text_muted
                                            }))
                                            .child(text),
                                    )
                            })),
                    )
                    // ✦ (§20B.1): the agent rewrites the draft.
                    .when(
                        is_focused && !draft.busy && !draft.prompt.text.is_blank(),
                        |el| {
                            let sparkles =
                                icon_svg(Icon::Sparkles, theme::ICON_SIZE_SMALL, theme.text_muted);
                            let improve = if rewriting {
                                disabled_button("composer-enhance", sparkles, theme, false)
                            } else {
                                quiet_button(
                                    "composer-enhance",
                                    sparkles,
                                    theme,
                                    theme.surface_raised,
                                )
                                .on_click(cx.listener(
                                    move |shell, _, window, cx| {
                                        shell.enhance_prompt(
                                            composer::Target::Composer(session),
                                            window,
                                            cx,
                                        )
                                    },
                                ))
                            };
                            el.child(improve.flex_none().tooltip(tooltip(
                                theme,
                                "Improve this prompt with the agent (it may read the code)",
                                "",
                            )))
                        },
                    )
                    // The prompt library (§20B.3), typed into this draft.
                    .when(is_focused && !draft.busy, |el| {
                        el.child(
                            quiet_button(
                                "composer-prompts",
                                icon_text(Icon::Library, "Prompts…", theme.text_muted),
                                theme,
                                theme.surface_raised,
                            )
                            .flex_none()
                            .tooltip(tooltip(
                                theme,
                                "Insert a prompt from the library (type / in the box for the agent's commands)",
                                "",
                            ))
                            .on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    shell.open_prompt_picker(
                                        composer::Target::Composer(session),
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    }),
            )
    }
}
