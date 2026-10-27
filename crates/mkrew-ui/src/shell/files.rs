//! Files view state: tree, read, find/replace, editor keys, save.

use crate::*;

impl Shell {
    /// The Files view of `project_id`, created on first use.
    pub(crate) fn files_view(&mut self, project_id: Ulid) -> &mut FilesView {
        self.files
            .entry(project_id)
            .or_insert_with(|| FilesView::new(project_id, None))
    }

    /// The Files view of `project_id` while it still shows `target` (an answer sent for
    /// another target is dropped).
    pub(crate) fn files_at(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
    ) -> Option<&mut FilesView> {
        self.files
            .get_mut(&project_id)
            .filter(|view| view.worktree_task_id == target)
    }

    /// The chooser picked `target` (SPEC §20E.1): the Files view starts over on it.
    pub(crate) fn set_files_target(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).worktree_task_id == target {
            return;
        }
        let unsaved = (self.files.get(&project_id)).map_or_else(Vec::new, FilesView::unsaved_paths);
        if !unsaved.is_empty() {
            let again: AfterSave = Box::new(move |shell, window, cx| {
                shell.set_files_target(project_id, target, window, cx);
            });
            return self.leave_files(project_id, unsaved, again, window, cx);
        }
        self.files
            .insert(project_id, FilesView::new(project_id, target));
        self.files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.refresh_files(project_id, window, cx);
    }

    /// Lists the root and open folders of the Files view of `project_id` again and re-reads
    /// its active tab.
    pub(crate) fn refresh_files(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let requests = self.files_view(project_id).refresh();
        self.files_requests(project_id, requests, window, cx);
    }

    pub(crate) fn files_requests(
        &mut self,
        project_id: Ulid,
        requests: Requests,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        for dir in requests.list {
            self.list_dir(project_id, dir, window, cx);
        }
        if let Some(path) = requests.read {
            self.read_file(project_id, path, window, cx);
        }
        cx.notify();
    }

    /// `fs.list` of one folder of the Files view of `project_id` (also keeps the daemon
    /// watching the workspace, so `git.changed` follows file changes).
    pub(crate) fn list_dir(
        &mut self,
        project_id: Ulid,
        dir: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.files_view(project_id);
        if !view.begin_list(&dir) {
            return;
        }
        let target = view.worktree_task_id;
        let listed = connection.call::<Vec<FsEntry>>(Request::FsList(FsListParams {
            project_id,
            worktree_task_id: target,
            dir: dir.clone(),
            show_ignored: view.show_ignored,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &listed {
                    tracing::warn!(%project_id, dir, "fs.list failed: {error}");
                }
                let Some(view) = shell.files_at(project_id, target) else {
                    return;
                };
                for dir in view.listed(&dir, listed) {
                    shell.list_dir(project_id, dir, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `fs.read` of a tab's file, highlighted on the background executor; while one is in
    /// flight, the file is read once more after it answers.
    pub(crate) fn read_file(
        &mut self,
        project_id: Ulid,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.files_view(project_id);
        let opening = view.tab_for(&path).map(|tab| tab.opening);
        if !view.begin_read(&path) {
            return;
        }
        let target = view.worktree_task_id;
        let read = connection.call::<FsContent>(Request::FsRead(FsReadParams {
            project_id,
            worktree_task_id: target,
            path: path.clone(),
            index_text: true,
        }));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let read = match read.await {
                Ok(content) => Ok(background
                    .spawn(async move {
                        let code = Code::new(&content);
                        (content, code)
                    })
                    .await),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &read {
                    tracing::warn!(%project_id, path, "fs.read failed: {error}");
                }
                // Dropped when its tab was closed (and maybe opened again) meanwhile.
                let again = (shell.files_at(project_id, target))
                    .filter(|view| view.tab_for(&path).map(|tab| tab.opening) == opening)
                    .and_then(|view| view.read(&path, read));
                if let Some(path) = again {
                    shell.read_file(project_id, path, window, cx);
                }
                shell.show_pending_line(project_id, target);
                shell.build_preview(project_id, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Parses the `.md` file of the Files view of `project_id` for its preview when it is
    /// shown as one and its text moved on, off the UI thread (highlighting and Mermaid are
    /// slow). Asks again when the text changed while it ran.
    pub(crate) fn build_preview(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let target = view.worktree_task_id;
        let Some((stamp, text)) = view.tab_mut().and_then(FileTab::preview_request) else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let page = background
                .spawn(async move { Shown::from_text(&text) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let dropped = (shell.files_at(project_id, target))
                    .is_some_and(|view| view.preview_ready(stamp, page));
                if dropped {
                    shell.toast_info("The file changed; your block edit was closed", cx);
                }
                shell.build_preview(project_id, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Follows `git.changed` in the Files view on screen (hidden ones refresh when shown).
    pub(crate) fn files_event(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace.filter(|_| self.view() == View::Files) else {
            return;
        };
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let requests = view.apply(event);
        self.files_requests(project_id, requests, window, cx);
    }

    /// A click on a tree row: opens or closes a folder, opens a file (`Kept` for a double
    /// click).
    pub(crate) fn click_file(
        &mut self,
        project_id: Ulid,
        path: &str,
        how: Open,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.files_focus, cx);
        // The shown path is compared, not the index: a file opened over the transient tab
        // keeps the same index.
        let view = self.files_view(project_id);
        let before = view.file().map(str::to_owned);
        let requests = view.click(path, how);
        if view.file() != before.as_deref() {
            // A file click lists nothing; only the read matters.
            return self.file_tab_shown(project_id, requests.read, window, cx);
        }
        self.files_requests(project_id, requests, window, cx);
    }

    /// Arrow keys move the tree cursor (kept in view), ← / → close / open folders, Enter opens.
    pub(crate) fn files_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(key), Some(project_id)) = (files_view::key(&event.keystroke), self.workspace)
        else {
            return;
        };
        cx.stop_propagation();
        if key == files_view::Key::Enter
            && let Some(cursor) = self.files_view(project_id).enter_target()
        {
            return self.click_file(project_id, &cursor, Open::Transient, window, cx);
        }
        let view = self.files_view(project_id);
        let requests = view.key(key);
        let cursor = view.cursor.clone();
        let at = view.rows().iter().position(|row| {
            matches!(row, files_view::Row::Entry { entry, .. } if Some(&entry.path) == cursor.as_ref())
        });
        if let Some(at) = at {
            self.files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
        self.files_requests(project_id, requests, window, cx);
    }

    /// Ctrl+F / Ctrl+Shift+F / "Find" (or Ctrl+H for `replace`) in the Files view: opens the
    /// find bar over the opened file and gives its field the keyboard.
    pub(crate) fn open_file_find(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if !self.files_view(project_id).open_find(replace) {
            return;
        }
        let field = if replace {
            &self.file_replace_focus
        } else {
            &self.file_find_focus
        };
        window.focus(field, cx);
        self.reveal_file_match(project_id, cx);
    }

    /// Typing edits the query; Enter / Shift+Enter go to the next / previous match; Tab goes
    /// to the replace field when it is shown; Ctrl+Alt+Enter replaces all; Esc closes the bar.
    pub(crate) fn file_find_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(project_id) = self.workspace else {
            return;
        };
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        match keystroke.key.as_str() {
            "enter" if m.control && m.alt => {
                return self.replace_all_in_file(project_id, window, cx);
            }
            "tab" if !m.control && !m.alt => {
                // Tab / Shift+Tab: to the replace field when it is shown.
                let replacing = (self.files_view(project_id).find.as_ref())
                    .is_some_and(|find| find.replace.is_some());
                if replacing {
                    window.focus(&self.file_replace_focus, cx);
                    cx.notify();
                }
                return;
            }
            _ => {}
        }
        if let Some(edit) = field_edit(keystroke, cx) {
            self.edit_file_find(project_id, |query, cx| edit.apply(query, cx), cx);
            return;
        }
        match find_key(keystroke) {
            Some(FindKey::Close) => self.close_file_find(project_id, window, cx),
            // The terminal's names: Enter = "older" = the next match here.
            Some(FindKey::Older) => self.step_file_find(project_id, true, cx),
            Some(FindKey::Newer) => self.step_file_find(project_id, false, cx),
            _ => {}
        }
    }

    pub(crate) fn edit_file_find(
        &mut self,
        project_id: Ulid,
        edit: impl FnOnce(&mut Input, &mut gpui::App),
        cx: &mut Context<'_, Self>,
    ) {
        let view = self.files_view(project_id);
        let Some(find) = &mut view.find else {
            return;
        };
        edit(&mut find.query, cx);
        view.search();
        self.reveal_file_match(project_id, cx);
    }

    pub(crate) fn step_file_find(
        &mut self,
        project_id: Ulid,
        forward: bool,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(find) = &mut self.files_view(project_id).find {
            find.step(forward);
        }
        self.reveal_file_match(project_id, cx);
    }

    /// Scrolls to the current match and selects it in the editor.
    pub(crate) fn reveal_file_match(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        let view = self.files_view(project_id);
        let found = view.find.as_ref().and_then(TextFind::current).cloned();
        if let (Some(found), Some(editor)) = (found, view.editor_mut()) {
            let (start, end) = (found.range.start, found.range.end);
            editor.select(Pos::new(found.line, start)..Pos::new(found.line, end));
            self.file_code_scroll
                .scroll_to_item(found.line, ScrollStrategy::Center);
        }
        cx.notify();
    }

    pub(crate) fn close_file_find(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.files_view(project_id).find = None;
        window.focus(&self.file_editor_focus, cx);
        cx.notify();
    }

    /// Keys of the replace field:
    /// - typing edits the replacement;
    /// - Enter replaces the current match and goes to the next;
    /// - Ctrl+Alt+Enter replaces all;
    /// - Tab goes back to the find field;
    /// - Esc closes the bar.
    pub(crate) fn file_replace_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(project_id) = self.workspace else {
            return;
        };
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        match keystroke.key.as_str() {
            "enter" if m.control && m.alt => {
                return self.replace_all_in_file(project_id, window, cx);
            }
            "enter" if !m.shift && !m.control => {
                return self.replace_in_file(project_id, window, cx);
            }
            "tab" if !m.control && !m.alt => {
                window.focus(&self.file_find_focus, cx);
                cx.notify();
                return;
            }
            _ => {}
        }
        if let Some(edit) = field_edit(keystroke, cx) {
            self.edit_file_replace(project_id, |with, cx| edit.apply(with, cx), cx);
        } else if find_key(keystroke) == Some(FindKey::Close) {
            self.close_file_find(project_id, window, cx);
        }
    }

    pub(crate) fn edit_file_replace(
        &mut self,
        project_id: Ulid,
        edit: impl FnOnce(&mut Input, &mut gpui::App),
        cx: &mut Context<'_, Self>,
    ) {
        let find = self.files_view(project_id).find.as_mut();
        if let Some(with) = find.and_then(|find| find.replace.as_mut()) {
            edit(with, cx);
            cx.notify();
        }
    }

    pub(crate) fn replace_in_file(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).replace_one() {
            self.file_edited(project_id, window, cx);
            self.reveal_file_match(project_id, cx);
        }
    }

    pub(crate) fn replace_all_in_file(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).replace_all() > 0 {
            self.file_edited(project_id, window, cx);
        }
        cx.notify();
    }

    /// Ctrl+S / "Save" (§12A): `fs.write` of tab `path` over the version it was read as.
    /// `then` runs after a successful save; while one is in flight already, after that one.
    pub(crate) fn save_file(
        &mut self,
        project_id: Ulid,
        path: String,
        then: Option<AfterSave>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let target = view.worktree_task_id;
        let opening = view.tab_for(&path).map(|tab| tab.opening);
        let Some(save) = view.begin_save(&path) else {
            if let Some(then) = then
                && view.tab_for(&path).is_some_and(FileTab::is_saving)
            {
                self.files_after_save.push((project_id, path, then));
            }
            return;
        };
        let written = connection.call::<FsWritten>(Request::FsWrite(FsWriteParams {
            project_id,
            worktree_task_id: target,
            path: save.path.clone(),
            content: save.text,
            expected_hash: save.expected_hash,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let written = written.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &written {
                    tracing::warn!(%project_id, path = %save.path, "fs.write failed: {error}");
                }
                let done = matches!(&written, Ok(written) if written.saved);
                let unshown = match shell.files_at(project_id, target) {
                    // Its tab is still the one that asked (not closed and opened again).
                    Some(view) if view.tab_for(&save.path).map(|tab| tab.opening) == opening => {
                        view.saved(&save.path, save.point, written)
                    }
                    // The tab or the workspace's Files view is gone.
                    _ => files_view::unshown_save(written),
                };
                if let Some(message) = unshown {
                    shell.toast_error(message, cx);
                }
                let (waiting, others): (Vec<_>, Vec<_>) =
                    std::mem::take(&mut shell.files_after_save)
                        .into_iter()
                        .partition(|(id, path, _)| *id == project_id && *path == save.path);
                shell.files_after_save = others;
                if done {
                    for then in then.into_iter().chain(waiting.into_iter().map(|w| w.2)) {
                        then(shell, window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Runs `then` once the unsaved changes of `paths` (tabs of `project_id`'s Files view)
    /// are dealt with, asking about one tab at a time (shown while asked):
    /// - at once when none is unsaved;
    /// - after "Save" and a successful save, or "Don't save", for each of them.
    ///
    /// "Cancel" stops: `then` does not run (§2.5).
    pub(crate) fn leave_files(
        &mut self,
        project_id: Ulid,
        paths: Vec<String>,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // A tab saved meanwhile is skipped; one typed into during its save is asked again.
        let next = (self.files.get(&project_id)).and_then(|view| view.first_unsaved(&paths));
        let Some(path) = next else {
            return then(self, window, cx);
        };
        // Its tab exists (it is unsaved): `open` only shows it.
        self.open_file_tab(project_id, &path, Open::Transient, window, cx);
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        let workspace = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map_or_else(String::new, |project| format!(" in {}", project.name));
        let detail = format!("{path}{workspace} — your changes are lost if you don't save them.");
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to {name}?"),
            Some(detail.as_str()),
            &files_view::LEAVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = files_view::leave_choice(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| {
                let rest: AfterSave = Box::new(move |shell, window, cx| {
                    shell.leave_files(project_id, paths, then, window, cx);
                });
                match choice {
                    Some(files_view::Leave::Save) => {
                        shell.save_file(project_id, path, Some(rest), window, cx);
                    }
                    Some(files_view::Leave::DontSave) => {
                        // The tab shows the file on disk again (it may stay open on Cancel).
                        let read = shell.files_view(project_id).drop_changes(&path);
                        let requests = Requests { list: vec![], read };
                        shell.files_requests(project_id, requests, window, cx);
                        rest(shell, window, cx);
                    }
                    None => {}
                }
            });
        })
        .detach();
    }

    /// The window's close button (§2.5, §20J): with `[ui] close_to_tray` on and a tray icon to
    /// bring it back, the window only hides (Files changes stay in memory); otherwise unsaved
    /// Files changes ask first, one file at a time, and the window closes once none are left.
    /// The workspace that holds them is shown first, so a refused save is visible.
    pub(crate) fn window_should_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if attention::hide_on_close(self.close_to_tray, self.desktop.has_tray()) {
            self.desktop.hide_window();
            return false;
        }
        let again: AfterSave = Box::new(|shell, window, cx| shell.close_when_saved(window, cx));
        !self.ask_unsaved(again, window, cx)
    }

    /// Closes the window once no Files change is unsaved, asking about the rest first.
    fn close_when_saved(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let again: AfterSave = Box::new(|shell, window, cx| shell.close_when_saved(window, cx));
        if !self.ask_unsaved(again, window, cx) {
            window.remove_window();
        }
    }

    /// Asks about the first unsaved Files change (`then` runs after it was saved or dropped).
    /// `true` while a question is open or was just opened, `false` when nothing is unsaved.
    pub(crate) fn ask_unsaved(
        &mut self,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let unsaved = (self.files.values()).find_map(|view| {
            let paths = view.unsaved_paths();
            (!paths.is_empty()).then_some((view.project_id, paths))
        });
        let Some((project_id, paths)) = unsaved else {
            return false;
        };
        // A confirmation is open (Alt+F4 reaches here): answer it first; asking now would
        // replace it.
        if window.has_active_prompt() {
            return true;
        }
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.show_view(View::Files, window, cx);
        self.leave_files(project_id, paths, then, window, cx);
        true
    }

    /// Keys of the Files editor (§12A, VS Code style; not in the keymap, ADR 0028). Keys the
    /// editor does not take (window shortcuts) bubble on to the keymap.
    pub(crate) fn file_editor_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(key), Some(project_id)) = (editor::edit_key(&event.keystroke), self.workspace)
        else {
            return;
        };
        cx.stop_propagation();
        let page = self.file_page_lines(cx);
        // An empty clipboard pastes nothing (no empty undo step).
        let pasted = (key == EditKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten()
            .filter(|text| !text.is_empty());
        let Some(editor) = self.files_view(project_id).editor_mut() else {
            return;
        };
        let version = editor.version();
        let copied = match key {
            EditKey::Copy => editor.selected_text(),
            EditKey::Cut => editor.cut(),
            EditKey::Paste => {
                if let Some(text) = pasted {
                    editor.insert(&text);
                }
                None
            }
            key => {
                editor.key(key, page);
                None
            }
        };
        let (line, edited) = (editor.cursor().line, editor.version() != version);
        if let Some(text) = copied {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.file_code_scroll
            .scroll_to_item(line, ScrollStrategy::Nearest);
        if edited {
            self.file_edited(project_id, window, cx);
        }
        cx.notify();
    }

    /// A click (or a drag, `extend`) at window x `x` on line `line` of the editor.
    pub(crate) fn click_file_text(
        &mut self,
        project_id: Ulid,
        line: usize,
        x: Pixels,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.file_editor_focus, cx);
        let cell_width = mkrew_ui::terminal::view::cell_size(window, cx.theme()).0;
        let cells = ((x - self.file_text_left.get()) / cell_width)
            .round()
            .max(0.0) as usize;
        let Some(editor) = self.files_view(project_id).editor_mut() else {
            return;
        };
        let Some(text) = editor.lines().get(line) else {
            return;
        };
        let col = editor::byte_for_display_col(text, cells);
        editor.set_cursor(Pos { line, col }, extend);
        cx.notify();
    }

    /// After an edit, the tab is kept (no longer transient), find follows the new text, and
    /// the highlight follows once typing pauses (in that tab, whichever is shown by then).
    pub(crate) fn file_edited(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let view = self.files_view(project_id);
        view.keep();
        view.search();
        let Some((path, version)) = view
            .tab()
            .and_then(|tab| Some((tab.path.clone(), tab.shown()?.editor.version())))
        else {
            return;
        };
        let target = view.worktree_task_id;
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            background.timer(theme::REHIGHLIGHT_DELAY).await;
            let Ok(Some(content)) = this.update(cx, |shell, _| {
                (shell.files_at(project_id, target))
                    .and_then(|view| view.tab_for(&path))
                    .and_then(|tab| tab.highlight_request(version))
            }) else {
                return;
            };
            let code = background.spawn(async move { Code::new(&content) }).await;
            let _ = this.update(cx, |shell, cx| {
                let tab =
                    (shell.files_at(project_id, target)).and_then(|view| view.tab_for_mut(&path));
                if tab.is_some_and(|tab| tab.highlighted(version, code)) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Lines the editor shows at once (PageUp / PageDown).
    pub(crate) fn file_page_lines(&self, cx: &App) -> usize {
        let theme = cx.theme();
        let row = px(theme.terminal_font_size * theme.code_line_height);
        let shown = self.file_code_scroll.0.borrow().last_item_size;
        shown
            .map_or(1, |size| (size.item.height / row).floor() as usize)
            .max(1)
    }

    /// The absolute path of `path` (relative, `/`-separated) in workspace `project_id`, with
    /// native separators (Zed gets the path verbatim).
    pub(crate) fn workspace_file(&self, project_id: Ulid, path: &str) -> Option<PathBuf> {
        let project = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)?;
        Some(worktree_target::join_relative(
            PathBuf::from(&project.path),
            path,
        ))
    }

    /// [`Self::workspace_file`] in task worktree `target` when there is one (SPEC §20E.1).
    pub(crate) fn target_file(
        &self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
    ) -> Option<PathBuf> {
        let Some(task_id) = target else {
            return self.workspace_file(project_id, path);
        };
        let project = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)?;
        let root = self.board.task(task_id)?.worktree_path.as_deref()?;
        let dir = worktree_target::project_dir(Path::new(&project.path), Path::new(root));
        Some(worktree_target::join_relative(dir, path))
    }

    /// Opens the active tab's file in Zed, at the selected line if any; a failure shows in
    /// the viewer header.
    pub(crate) fn open_file_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        let Some(view) = self.files.get(&project_id) else {
            return;
        };
        let (Some(path), line) = (
            view.file().map(str::to_owned),
            view.tab().and_then(FileTab::line),
        ) else {
            return;
        };
        let Some(full) = self.target_file(project_id, view.worktree_task_id, &path) else {
            return;
        };
        let error = zed::open(&full, line).err().map(|error| {
            tracing::warn!(%project_id, path, "open in Zed failed: {error}");
            error.to_string()
        });
        self.files_view(project_id).zed_error = error;
        cx.notify();
    }

    /// Files and Git views on a worktree that was removed (or whose task was deleted) go back
    /// to the project folder.
    pub(crate) fn drop_gone_targets(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let tasks = self.board.tasks();
        let gone = |project_id, target| !worktree_target::exists(project_id, target, tasks);
        let files: Vec<Ulid> = (self.files.values())
            .filter(|view| gone(view.project_id, view.worktree_task_id))
            .map(|view| view.project_id)
            .collect();
        let git: Vec<Ulid> = (self.git.values())
            .filter(|view| gone(view.project_id, view.worktree_task_id))
            .map(|view| view.project_id)
            .collect();
        // Hidden views read again when shown.
        let shown = self.workspace;
        for project_id in files {
            self.files
                .insert(project_id, FilesView::new(project_id, None));
            if shown == Some(project_id) && self.view() == View::Files {
                self.refresh_files(project_id, window, cx);
            }
        }
        for project_id in git {
            self.retarget_git(project_id, None, None);
            if shown == Some(project_id) && self.view() == View::Git {
                self.refresh_git(project_id, window, cx);
            }
        }
    }
}
