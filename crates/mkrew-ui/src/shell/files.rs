//! Files tree state and the open files' shell side: read, find/replace, editor keys, save.

use crate::*;

impl Shell {
    /// The Files view of `project_id`, created on first use.
    pub(crate) fn files_view(&mut self, project_id: Ulid) -> &mut FilesView {
        self.files
            .entry(project_id)
            .or_insert_with(|| new_files_view(project_id, None))
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

    pub(crate) fn doc(&self, project_id: Ulid, file: &FileKey) -> Option<&documents::Doc> {
        self.documents.get(&project_id)?.get(file)
    }

    pub(crate) fn doc_mut(
        &mut self,
        project_id: Ulid,
        file: &FileKey,
    ) -> Option<&mut documents::Doc> {
        self.documents.get_mut(&project_id)?.get_mut(file)
    }

    pub(crate) fn doc_view(&self, project_id: Ulid, file: &FileKey) -> Option<&DocView> {
        self.doc_views.get(&(project_id, file.clone()))
    }

    /// The doc and view state of a file pane, made when missing. `mode` is the leaf's `md`.
    pub(crate) fn ensure_doc(
        &mut self,
        project_id: Ulid,
        file: &FileKey,
        mode: Option<MdMode>,
        cx: &App,
    ) {
        let mode = mode.unwrap_or(self.md_mode);
        (self.documents.entry(project_id).or_default()).ensure(file, mode);
        (self.doc_views.entry((project_id, file.clone()))).or_insert_with(|| DocView::new(cx));
    }

    pub(crate) fn focus_doc(
        &self,
        project_id: Ulid,
        file: &FileKey,
        which: DocFocus,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(view) = self.doc_view(project_id, file) {
            window.focus(view.handle(which), cx);
        }
    }

    pub(crate) fn doc_focused(
        &self,
        project_id: Ulid,
        file: &FileKey,
        which: DocFocus,
        window: &Window,
    ) -> bool {
        self.doc_view(project_id, file)
            .is_some_and(|view| view.handle(which).is_focused(window))
    }

    /// The chooser picked `target` (SPEC §20E.1): the Files tree starts over on it. Open files
    /// stay as they are: each one knows its own folder (`FileKey::task`).
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
        self.files
            .insert(project_id, new_files_view(project_id, target));
        self.host_view(self.space_host(window))
            .files_scroll
            .scroll_to_item(0, ScrollStrategy::Top);
        self.refresh_files(project_id, window, cx);
    }

    /// Lists the root and open folders of the Files view of `project_id` again.
    pub(crate) fn refresh_files(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.sync_watches(cx);
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
            let _ = Shell::answer(&this, cx, |shell, window, cx| {
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

    /// `fs.read` of a doc's file (of the folder its key names), highlighted on the background
    /// executor; while one is in flight, the file is read once more after it answers.
    pub(crate) fn read_file(
        &mut self,
        project_id: Ulid,
        file: FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let docs = self.documents.entry(project_id).or_default();
        let opening = docs.get(&file).map(|doc| doc.opening);
        if !docs.begin_read(&file) {
            return;
        }
        let read = connection
            .call::<FsContent>(Request::FsRead(documents::read_params(project_id, &file)));
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
            let _ = Shell::answer(&this, cx, |shell, window, cx| {
                if let Err(error) = &read {
                    tracing::warn!(%project_id, path = %file.path, task = ?file.task, "fs.read failed: {error}");
                }
                // Dropped when its doc was closed (and maybe opened again) meanwhile.
                let again = (shell.documents.get_mut(&project_id))
                    .filter(|docs| docs.get(&file).map(|doc| doc.opening) == opening)
                    .and_then(|docs| docs.read(&file, read));
                if let Some(again) = again {
                    shell.read_file(project_id, again, window, cx);
                }
                shell.show_pending_line(project_id, &file);
                shell.build_preview(project_id, &file, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// The Retry button of a pane whose read failed.
    pub(crate) fn retry_read(
        &mut self,
        project_id: Ulid,
        file: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self
            .doc_mut(project_id, file)
            .is_some_and(documents::Doc::retry)
        {
            self.read_file(project_id, file.clone(), window, cx);
        }
        cx.notify();
    }

    /// Parses the `.md` file `file` of `project_id` for its preview when it is shown as one and
    /// its text moved on, off the UI thread (highlighting and Mermaid are slow). Asks again
    /// when the text changed while it ran.
    pub(crate) fn build_preview(
        &mut self,
        project_id: Ulid,
        file: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((stamp, text)) =
            (self.doc_mut(project_id, file)).and_then(documents::Doc::preview_request)
        else {
            return;
        };
        let file = file.clone();
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let page = background
                .spawn(async move { Shown::from_text(&text) })
                .await;
            let _ = Shell::answer(&this, cx, |shell, window, cx| {
                // The stamp names the text: a page of a text that was replaced is dropped.
                let dropped = (shell.documents.get_mut(&project_id))
                    .is_some_and(|docs| docs.preview_ready(stamp, page));
                if dropped {
                    shell.toast_info("The file changed; your block edit was closed", cx);
                }
                shell.build_preview(project_id, &file, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Puts the caret on the line a link asked for and scrolls there, once the file is shown
    /// (after a request when it already was, after the read otherwise).
    pub(crate) fn show_pending_line(&mut self, project_id: Ulid, file: &FileKey) {
        if let Some(line) =
            (self.doc_mut(project_id, file)).and_then(documents::Doc::apply_pending_line)
            && let Some(view) = self.doc_view(project_id, file)
        {
            view.code_scroll
                .scroll_to_item(line, ScrollStrategy::Center);
        }
    }

    /// Follows `git.changed`: the open files on screen are read again (the others when drawn),
    /// and the tree on screen lists its open folders again.
    pub(crate) fn files_event(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Event::GitChanged(changed) = event else {
            return;
        };
        let project_id = changed.project_id;
        // The open files of the changed folder (the project's or that worktree's).
        let shown: Vec<FileKey> = (self.visible_files().into_iter())
            .filter(|(project, file, _)| {
                *project == project_id && file.task == changed.worktree_task_id
            })
            .map(|(_, file, _)| file)
            .collect();
        let reads = (self.documents.get_mut(&project_id)).map_or_else(Vec::new, |docs| {
            docs.refresh(changed.worktree_task_id, &shown)
        });
        for file in reads {
            self.read_file(project_id, file, window, cx);
        }
        let target = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        if changed.worktree_task_id == target
            && (self.space_of(project_id))
                .is_some_and(|key| self.file_tree_shown(HostId::Space(key)))
            && let Some(view) = self.files.get_mut(&project_id)
        {
            let requests = view.apply(event);
            self.files_requests(project_id, requests, window, cx);
        }
    }

    /// A click on a tree row: opens or closes a folder; a file opens as the preview tab, or
    /// as a kept one on a double click (SPEC §6.1).
    pub(crate) fn click_file(
        &mut self,
        project_id: Ulid,
        path: &str,
        double: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let view = self.files_view(project_id);
        let target = view.worktree_task_id;
        let is_file = view.is_file(path);
        let requests = view.click(path);
        self.files_requests(project_id, requests, window, cx);
        if is_file {
            let place = if double {
                documents::Place::NewTab
            } else {
                documents::Place::Preview
            };
            let file = FileKey::new(path, target);
            self.open_document(project_id, file.clone(), None, place, window, cx);
            if double {
                self.keep_leaf(project_id, &file, window, cx);
            }
        }
        // The keyboard stays in the tree.
        window.focus(&self.files_focus, cx);
    }

    /// Arrow keys move the tree cursor (kept in view), ← / → close / open folders, Enter opens.
    pub(crate) fn files_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(key), Some(project_id)) =
            (files_view::key(&event.keystroke), self.workspace_in(window))
        else {
            return;
        };
        cx.stop_propagation();
        if key == files_view::Key::Enter
            && let Some(cursor) = self.files_view(project_id).enter_target()
        {
            return self.click_file(project_id, &cursor, false, window, cx);
        }
        let view = self.files_view(project_id);
        let requests = view.key(key);
        let cursor = view.cursor.clone();
        let at = view.rows().iter().position(|row| {
            matches!(row, files_view::Row::Entry { entry, .. } if Some(&entry.path) == cursor.as_ref())
        });
        if let Some(at) = at {
            self.host_view(self.space_host(window))
                .files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
        self.files_requests(project_id, requests, window, cx);
    }

    /// Ctrl+F / "Find" (or Ctrl+H for `replace`) in a file pane: opens the find bar over the
    /// focused file and gives its field the keyboard.
    pub(crate) fn open_file_find(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((project_id, path)) = self.focused_doc(window) else {
            return;
        };
        if !(self.doc_mut(project_id, &path)).is_some_and(|doc| doc.open_find(replace)) {
            return;
        }
        let field = if replace {
            DocFocus::Replace
        } else {
            DocFocus::Find
        };
        self.focus_doc(project_id, &path, field, window, cx);
        self.reveal_file_match(project_id, &path, cx);
    }

    /// Typing edits the query; Enter / Shift+Enter go to the next / previous match; Tab goes
    /// to the replace field when it is shown; Ctrl+Alt+Enter (⌥⌘↩ on macOS) replaces all; Esc
    /// closes the bar.
    pub(crate) fn file_find_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some((project_id, path)) = self.focused_doc(window) else {
            return;
        };
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        match keystroke.key.as_str() {
            "enter" if m.secondary() && m.alt => {
                return self.replace_all_in_file(project_id, &path, window, cx);
            }
            "tab" if !m.control && !m.alt => {
                // Tab / Shift+Tab: to the replace field when it is shown.
                let replacing = (self
                    .doc(project_id, &path)
                    .and_then(|doc| doc.find.as_ref()))
                .is_some_and(|find| find.replace.is_some());
                if replacing {
                    self.focus_doc(project_id, &path, DocFocus::Replace, window, cx);
                    cx.notify();
                }
                return;
            }
            _ => {}
        }
        if let Some(edit) = field_edit(keystroke, cx) {
            self.edit_file_find(project_id, &path, |query, cx| edit.apply(query, cx), cx);
            return;
        }
        match find_key(keystroke) {
            Some(FindKey::Close) => self.close_file_find(project_id, &path, window, cx),
            // The terminal's names: Enter = "older" = the next match here.
            Some(FindKey::Older) => self.step_file_find(project_id, &path, true, cx),
            Some(FindKey::Newer) => self.step_file_find(project_id, &path, false, cx),
            _ => {}
        }
    }

    pub(crate) fn edit_file_find(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        edit: impl FnOnce(&mut Input, &mut gpui::App),
        cx: &mut Context<'_, Self>,
    ) {
        let Some(doc) = self.doc_mut(project_id, path) else {
            return;
        };
        let Some(find) = &mut doc.find else {
            return;
        };
        edit(&mut find.query, cx);
        doc.search();
        self.reveal_file_match(project_id, path, cx);
    }

    pub(crate) fn step_file_find(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        forward: bool,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(find) = (self.doc_mut(project_id, path)).and_then(|doc| doc.find.as_mut()) {
            find.step(forward);
        }
        self.reveal_file_match(project_id, path, cx);
    }

    /// Scrolls to the current match and selects it in the editor.
    pub(crate) fn reveal_file_match(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        cx: &mut Context<'_, Self>,
    ) {
        let found = (self.doc(project_id, path))
            .and_then(|doc| doc.find.as_ref())
            .and_then(TextFind::current)
            .cloned();
        if let Some(found) = found
            && let Some(editor) =
                (self.doc_mut(project_id, path)).and_then(documents::Doc::editor_mut)
        {
            let (start, end) = (found.range.start, found.range.end);
            editor.select(Pos::new(found.line, start)..Pos::new(found.line, end));
            if let Some(view) = self.doc_view(project_id, path) {
                view.code_scroll
                    .scroll_to_item(found.line, ScrollStrategy::Center);
            }
        }
        cx.notify();
    }

    pub(crate) fn close_file_find(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(doc) = self.doc_mut(project_id, path) {
            doc.find = None;
        }
        self.focus_doc(project_id, path, DocFocus::Editor, window, cx);
        cx.notify();
    }

    /// Keys of the replace field:
    /// - typing edits the replacement;
    /// - Enter replaces the current match and goes to the next;
    /// - Ctrl+Alt+Enter (⌥⌘↩ on macOS) replaces all;
    /// - Tab goes back to the find field;
    /// - Esc closes the bar.
    pub(crate) fn file_replace_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some((project_id, path)) = self.focused_doc(window) else {
            return;
        };
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        match keystroke.key.as_str() {
            "enter" if m.secondary() && m.alt => {
                return self.replace_all_in_file(project_id, &path, window, cx);
            }
            "enter" if !m.shift && !m.control => {
                return self.replace_in_file(project_id, &path, window, cx);
            }
            "tab" if !m.control && !m.alt => {
                self.focus_doc(project_id, &path, DocFocus::Find, window, cx);
                cx.notify();
                return;
            }
            _ => {}
        }
        if let Some(edit) = field_edit(keystroke, cx) {
            self.edit_file_replace(project_id, &path, |with, cx| edit.apply(with, cx), cx);
        } else if find_key(keystroke) == Some(FindKey::Close) {
            self.close_file_find(project_id, &path, window, cx);
        }
    }

    pub(crate) fn edit_file_replace(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        edit: impl FnOnce(&mut Input, &mut gpui::App),
        cx: &mut Context<'_, Self>,
    ) {
        let find = (self.doc_mut(project_id, path)).and_then(|doc| doc.find.as_mut());
        if let Some(with) = find.and_then(|find| find.replace.as_mut()) {
            edit(with, cx);
            cx.notify();
        }
    }

    pub(crate) fn replace_in_file(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if (self.doc_mut(project_id, path)).is_some_and(documents::Doc::replace_one) {
            self.file_edited(project_id, path, window, cx);
            self.reveal_file_match(project_id, path, cx);
        }
    }

    pub(crate) fn replace_all_in_file(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if (self.doc_mut(project_id, path)).is_some_and(|doc| doc.replace_all() > 0) {
            self.file_edited(project_id, path, window, cx);
        }
        cx.notify();
    }

    /// Ctrl+S / "Save" (§12A): `fs.write` of file `path` (in the folder its key names) over the
    /// version it was read as. `then` runs after a successful save; while one is in flight
    /// already, after that one.
    pub(crate) fn save_file(
        &mut self,
        project_id: Ulid,
        path: FileKey,
        then: Option<AfterSave>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // Without a connection the save says so (the text stays; it is kept over a reconnect).
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let Some(docs) = self.documents.get_mut(&project_id) else {
            return;
        };
        let opening = docs.get(&path).map(|doc| doc.opening);
        let Some(save) = docs.begin_save(&path) else {
            if let Some(then) = then
                && docs.get(&path).is_some_and(documents::Doc::is_saving)
            {
                self.files_after_save.push((project_id, path, then));
            }
            return;
        };
        let saved_file = FileKey::new(save.path.clone(), save.task);
        let point = save.point;
        let written = connection
            .call::<FsWritten>(Request::FsWrite(documents::write_params(project_id, save)));
        cx.spawn_in(window, async move |this, cx| {
            let written = written.await.map_err(|error| format!("{error:#}"));
            let _ = Shell::answer(&this, cx, |shell, window, cx| {
                if let Err(error) = &written {
                    tracing::warn!(%project_id, path = %saved_file.path, task = ?saved_file.task, "fs.write failed: {error}");
                }
                let done = matches!(&written, Ok(written) if written.saved);
                let unshown = match shell.documents.get_mut(&project_id) {
                    // Its doc is still the one that asked (not closed and opened again).
                    Some(docs) if docs.get(&saved_file).map(|doc| doc.opening) == opening => {
                        docs.saved(&saved_file, point, written)
                    }
                    // The doc is gone.
                    _ => documents::unshown_save(written),
                };
                if let Some(message) = unshown {
                    shell.toast_error(message, cx);
                }
                let (waiting, others): (Vec<_>, Vec<_>) =
                    std::mem::take(&mut shell.files_after_save)
                        .into_iter()
                        .partition(|(id, file, _)| *id == project_id && *file == saved_file);
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

    /// Runs `then` once the unsaved changes of `files` (docs of `project_id`) are dealt with,
    /// asking about one file at a time (its pane is shown while asked):
    /// - at once when none is unsaved;
    /// - after "Save" and a successful save, or "Don't save", for each of them.
    ///
    /// "Cancel" stops: `then` does not run (§2.5).
    pub(crate) fn leave_documents(
        &mut self,
        project_id: Ulid,
        files: Vec<FileKey>,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // A doc saved meanwhile is skipped; one typed into during its save is asked again.
        let next = (self.documents.get(&project_id)).and_then(|docs| docs.first_unsaved(&files));
        let Some(path) = next else {
            return then(self, window, cx);
        };
        // Its leaf is shown (in whatever tab or window holds it; a doc whose leaf is gone
        // gets a new tab): the question is about what the user sees.
        self.open_document(
            project_id,
            path.clone(),
            None,
            documents::Place::NewTab,
            window,
            cx,
        );
        let name = path
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&path.path)
            .to_owned();
        let workspace = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map(|project| project.name.as_str());
        let worktree = worktree_target::label(
            path.task,
            project_id,
            self.board.tasks(),
            self.board.is_loaded(),
        );
        let detail = worktree_target::leave_detail(&path.path, worktree.as_deref(), workspace);
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to {name}?"),
            Some(detail.as_str()),
            &documents::LEAVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = documents::leave_choice(answer.await.ok());
            let _ = Shell::answer(&this, cx, |shell, window, cx| {
                let rest: AfterSave = Box::new(move |shell, window, cx| {
                    shell.leave_documents(project_id, files, then, window, cx);
                });
                match choice {
                    Some(documents::Leave::Save) => {
                        shell.save_file(project_id, path, Some(rest), window, cx);
                    }
                    Some(documents::Leave::DontSave) => {
                        // The doc shows the file on disk again (it may stay open on Cancel).
                        let read = (shell.documents.get_mut(&project_id))
                            .and_then(|docs| docs.drop_changes(&path));
                        if let Some(path) = read {
                            shell.read_file(project_id, path, window, cx);
                        }
                        rest(shell, window, cx);
                    }
                    None => {}
                }
            });
        })
        .detach();
    }

    /// "Reload" of the changed-on-disk banner: the unsaved changes go and the file is read.
    pub(crate) fn reload_file(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = (self.documents.get_mut(&project_id)).and_then(|docs| docs.drop_changes(path));
        if let Some(path) = read {
            self.read_file(project_id, path, window, cx);
        }
    }

    /// The window's close button (§2.5, §20J, §20L): with `[ui] close_to_tray` on and something
    /// to bring it back (the tray icon on Windows, the Dock icon on macOS) the window only
    /// hides (unsaved file changes stay in memory); otherwise unsaved file changes ask first,
    /// one file at a time, and the window closes once none are left. The workspace that holds
    /// them is shown first, so a refused save is visible.
    pub(crate) fn last_window_should_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let can_hide = mkrew_ui::desktop::can_hide_on_close(Os::CURRENT, self.desktop.has_tray());
        if attention::hide_on_close(self.close_to_tray, can_hide) {
            self.desktop.hide_window();
            self.set_detached_shown(false, cx);
            // macOS: gpui cannot hide one window, so the app hides like Cmd+H. Off this update:
            // `hide` makes AppKit call back into the windows while gpui still holds the app.
            if Os::CURRENT == Os::Mac {
                cx.spawn(async move |this, cx| {
                    let _ = this.update(cx, |_, cx| cx.hide());
                })
                .detach();
            }
            return false;
        }
        let again: AfterSave = Box::new(|shell, window, cx| shell.close_when_saved(window, cx));
        if self.ask_unsaved(again, window, cx) {
            return false;
        }
        // Not hiding (Linux, a machine window, or `close_to_tray` off): closing this window ends
        // Mkrew, so the machine windows are asked to quit here, not left to the drop of the
        // `Shell`.
        self.save_before_exit(cx);
        true
    }

    /// Closes the window once no file change is unsaved, asking about the rest first.
    fn close_when_saved(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let again: AfterSave = Box::new(|shell, window, cx| shell.close_when_saved(window, cx));
        if !self.ask_unsaved(again, window, cx) {
            self.save_before_exit(cx);
            window.remove_window();
        }
    }

    /// Asks about the first unsaved file change (`then` runs after it was saved or dropped).
    /// `true` while a question is open or was just opened, `false` when nothing is unsaved.
    pub(crate) fn ask_unsaved(
        &mut self,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        // An unsaved prompt draft of Settings → Prompts asks first (spec 7.2, like files). The
        // dialog is dropped before `then` runs, so "Don't save" does not ask again.
        if (self.settings_dialog.as_ref()).is_some_and(|dialog| dialog.prompts.is_dirty()) {
            if window.has_active_prompt() {
                return true;
            }
            let then: AfterSave = Box::new(move |shell, window, cx| {
                shell.close_settings_dialog(window, cx);
                then(shell, window, cx);
            });
            self.leave_prompt_draft(then, window, cx);
            return true;
        }
        let unsaved = (self.documents.iter()).find_map(|(project_id, docs)| {
            let files = docs.unsaved_files();
            (!files.is_empty()).then_some((*project_id, files))
        });
        let Some((project_id, files)) = unsaved else {
            return false;
        };
        // A confirmation is open (Alt+F4 reaches here): answer it first; asking now would
        // replace it.
        if window.has_active_prompt() {
            return true;
        }
        if self.workspace_in(window) != Some(project_id) {
            self.activate_workspace(self.space_host(window), project_id, window, cx);
        }
        // `leave_documents` shows the file's pane, so a refused save is visible.
        self.leave_documents(project_id, files, then, window, cx);
        true
    }

    /// Keys of a file pane's editor (§12A, VS Code style; not in the keymap, ADR 0028). Keys
    /// the editor does not take (window shortcuts) bubble on to the keymap.
    pub(crate) fn file_editor_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(key), Some((project_id, path))) =
            (editor::edit_key(&event.keystroke), self.focused_doc(window))
        else {
            return;
        };
        cx.stop_propagation();
        let page = self.file_page_lines(project_id, &path, cx);
        // An empty clipboard pastes nothing (no empty undo step).
        let pasted = (key == EditKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten()
            .filter(|text| !text.is_empty());
        let Some(editor) = (self.doc_mut(project_id, &path)).and_then(documents::Doc::editor_mut)
        else {
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
        if let Some(view) = self.doc_view(project_id, &path) {
            view.code_scroll
                .scroll_to_item(line, ScrollStrategy::Nearest);
        }
        if edited {
            self.file_edited(project_id, &path, window, cx);
        }
        cx.notify();
    }

    /// A click (or a drag, `extend`) at window x `x` on line `line` of the editor of `path`.
    #[allow(clippy::too_many_arguments)] // The pane's file, the line, the pointer and the window.
    pub(crate) fn click_file_text(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        line: usize,
        x: Pixels,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.focus_doc(project_id, path, DocFocus::Editor, window, cx);
        let Some(text_left) = self
            .doc_view(project_id, path)
            .map(|view| view.text_left.get())
        else {
            return;
        };
        let cell_width = mkrew_ui::terminal::view::cell_size(window, cx.theme()).0;
        let cells = ((x - text_left) / cell_width).round().max(0.0) as usize;
        let Some(editor) = (self.doc_mut(project_id, path)).and_then(documents::Doc::editor_mut)
        else {
            return;
        };
        let Some(text) = editor.lines().get(line) else {
            return;
        };
        let col = editor::byte_for_display_col(text, cells);
        editor.set_cursor(Pos { line, col }, extend);
        cx.notify();
    }

    /// After an edit, the leaf is kept (no longer the preview), find follows the new text, the
    /// Split view's page is built again, and the highlight follows once typing pauses.
    pub(crate) fn file_edited(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.keep_leaf(project_id, path, window, cx);
        let Some(doc) = self.doc_mut(project_id, path) else {
            return;
        };
        doc.search();
        let Some(version) = doc.shown().map(|shown| shown.editor.version()) else {
            return;
        };
        // Only a page on screen is built; one build runs at a time and the last text wins.
        self.build_preview(project_id, path, window, cx);
        let path = path.clone();
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            background.timer(theme::REHIGHLIGHT_DELAY).await;
            let Ok(Some(content)) = this.update(cx, |shell, _| {
                (shell.doc(project_id, &path)).and_then(|doc| doc.highlight_request(version))
            }) else {
                return;
            };
            let code = background.spawn(async move { Code::new(&content) }).await;
            let _ = this.update(cx, |shell, cx| {
                let doc = shell.doc_mut(project_id, &path);
                if doc.is_some_and(|doc| doc.highlighted(version, code)) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Lines the editor of `path` shows at once (PageUp / PageDown).
    pub(crate) fn file_page_lines(&self, project_id: Ulid, path: &FileKey, cx: &App) -> usize {
        let theme = cx.theme();
        let row = px(theme.terminal_font_size * theme.code_line_height);
        let shown = (self.doc_view(project_id, path))
            .and_then(|view| view.code_scroll.0.borrow().last_item_size);
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

    /// [`Self::target_file`] as the text of a path on the other computer (a machine window,
    /// where `/` or `\` follows its project's path and nothing is read from this computer's
    /// disk). A task worktree gives its root: the folder of a project inside a repository is
    /// not worked out here.
    pub(crate) fn remote_file(
        &self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
    ) -> Option<String> {
        let project = (self.sidebar.projects().iter()).find(|p| p.id == project_id)?;
        let base = match target {
            None => project.path.as_str(),
            Some(task_id) => self.board.task(task_id)?.worktree_path.as_deref()?,
        };
        Some(worktree_target::join_remote(base, path))
    }

    /// Opens file `path` in Zed, at the caret's line; a failure shows in the pane header.
    pub(crate) fn open_file_in_zed(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        cx: &mut Context<'_, Self>,
    ) {
        if self.local_only(cx) {
            return;
        }
        let line = self.doc(project_id, path).and_then(documents::Doc::line);
        let Some(full) = self.target_file(project_id, path.task, &path.path) else {
            return;
        };
        let error = zed::open(&full, line).err().map(|error| {
            tracing::warn!(%project_id, path = %path.path, "open in Zed failed: {error}");
            error.to_string()
        });
        if let Some(view) = self.doc_views.get_mut(&(project_id, path.clone())) {
            view.zed_error = error;
        }
        cx.notify();
    }

    /// Files and Git views on a worktree that was removed (or whose task was deleted) go back
    /// to the project folder. Open files keep their folder: a read of one whose worktree is
    /// gone fails and the pane says so.
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
        for project_id in files {
            self.files
                .insert(project_id, new_files_view(project_id, None));
            // The worktree is gone (as before); the tree starts over.
            if (self.space_of(project_id))
                .is_some_and(|key| self.files_on_screen(HostId::Space(key)))
            {
                self.refresh_files(project_id, window, cx);
            }
        }
        for project_id in git {
            self.retarget_git(project_id, None, None);
            if (self.space_of(project_id)).is_some_and(|key| self.git_shown(HostId::Space(key))) {
                self.refresh_git(project_id, window, cx);
            }
        }
    }
}

/// A Files view of `project_id` on `target`.
fn new_files_view(project_id: Ulid, target: Option<Ulid>) -> FilesView {
    FilesView::new(project_id, target)
}
