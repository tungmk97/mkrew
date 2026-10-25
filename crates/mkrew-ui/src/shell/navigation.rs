//! Quick open, links, file reveal and the command palette.

use crate::*;

impl Shell {
    /// Quick open (Ctrl+Shift+O): a fuzzy finder over the active workspace's files.
    pub(crate) fn open_quick_open(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace.filter(|_| self.connection.is_some()) else {
            return;
        };
        self.workspace_menu = None;
        let target = self
            .files
            .get(&project_id)
            .and_then(|view| view.worktree_task_id);
        self.quick_open = Some(QuickOpen::new(project_id, target));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.find_files(window, cx);
        cx.notify();
    }

    pub(crate) fn close_quick_open(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let open = self.quick_open.take();
        // Esc on an `@` picker types the `@` it replaced (an e-mail, a handle).
        match open.and_then(|open| open.composer) {
            Some(session) => self.composer_typed(session, "@", window, cx),
            None => self.focus_pane(window, cx),
        }
        cx.notify();
    }

    /// `@` in `session`'s composer: quick open over its workspace's files; the pick is typed
    /// as `composer.file_ref` (§20B.1). Without a daemon the `@` is typed as is.
    pub(crate) fn open_file_ref_picker(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session)
            .map(|s| s.project_id)
            .filter(|_| self.connection.is_some())
        else {
            self.composers.insert(session, "@");
            cx.notify();
            return;
        };
        // Project-relative paths for the composer, whatever the Files view shows.
        let mut open = QuickOpen::new(project_id, None);
        open.composer = Some(session);
        self.quick_open = Some(open);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.find_files(window, cx);
        cx.notify();
    }

    /// Types `text` into `session`'s draft and gives its composer the keyboard back.
    pub(crate) fn composer_typed(
        &mut self,
        session: Ulid,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.composers.insert(session, text);
        self.composer_scroll.scroll_to_bottom();
        if self.composer_session() == Some(session) {
            window.focus(&self.composer_focus, cx);
        } else {
            self.focus_pane(window, cx);
        }
    }

    /// Files dropped from the OS on `session`'s pane (§20B.5): into its composer draft while
    /// the composer is shown, else typed into the terminal. No Enter either way. A modal dialog
    /// covers the panes, so a drop on it types nothing.
    pub(crate) fn drop_paths(
        &mut self,
        session: Ulid,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() {
            return;
        }
        let paths: Vec<String> = paths
            .paths()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let text = composer::typed_paths(paths.iter().map(String::as_str));
        if text.is_empty() {
            return;
        }
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.focus(session);
        }
        if self.composers.shown(session).is_some() {
            self.composer_typed(session, &text, window, cx);
        } else if let Some(pane) = self.panes.get(&session).cloned() {
            pane.update(cx, |pane, cx| pane.type_text(&text, cx));
            self.focus_pane(window, cx);
        }
        cx.notify();
    }

    /// `fs.find` for the finder's query; a reply to an older query or finder is dropped.
    pub(crate) fn find_files(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let dialog = self.dialog_serial;
        let Some(open) = self.quick_open.as_mut() else {
            return;
        };
        let (serial, params) = open.search();
        let project_id = params.project_id;
        let found = connection.call::<Vec<FsMatch>>(Request::FsFind(params));
        cx.spawn_in(window, async move |this, cx| {
            let found = found.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &found {
                    tracing::warn!(%project_id, "fs.find failed: {error}");
                }
                if shell.dialog_serial != dialog {
                    return;
                }
                if let Some(open) = shell.quick_open.as_mut()
                    && open.found(serial, found)
                {
                    shell.quick_open_scroll.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Typing searches again, ↑ / ↓ move the pick (kept in view), Enter opens it, Esc closes.
    pub(crate) fn quick_open_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = quick_open::key(&event.keystroke) else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(open) = self.quick_open.as_mut() else {
            return;
        };
        match key {
            quick_open::Key::Close => return self.close_quick_open(window, cx),
            quick_open::Key::Open => return self.open_quick_pick(window, cx),
            quick_open::Key::Move(_) => {
                open.key(key);
                self.quick_open_scroll.scroll_to_item(open.selected);
            }
            quick_open::Key::Paste => {
                if paste.is_some_and(|text| open.insert(&text)) {
                    self.find_files(window, cx);
                }
            }
            quick_open::Key::Edit(_) | quick_open::Key::Insert(_) => {
                if let quick_open::Key::Edit(edit) = &key {
                    copy_selection(edit, &open.query, cx);
                }
                if open.key(key) {
                    self.find_files(window, cx);
                }
            }
        }
        cx.notify();
    }

    /// Opens the picked file in the Files view (its folders open, the cursor lands on it).
    pub(crate) fn open_quick_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(open) = self.quick_open.take() else {
            return;
        };
        let Some(path) = open.picked().map(str::to_owned) else {
            self.quick_open = Some(open);
            return;
        };
        if let Some(session) = open.composer {
            // A missing or broken settings file types the default form (the daemon reports it).
            let template = mkrew_core::settings::load(&self.settings_file)
                .unwrap_or_default()
                .composer
                .file_ref;
            let text = composer::file_ref(&template, &path);
            self.composer_typed(session, &text, window, cx);
            cx.notify();
            return;
        }
        let project_id = open.project_id;
        self.open_revealed(project_id, open.worktree_task_id, path, None, window, cx);
    }

    /// Where links typed in `session`'s terminal point: its project, the task worktree its
    /// agent runs in (`None` = the project folder) and that folder.
    pub(crate) fn link_scope(&self, session: Ulid) -> Option<(Ulid, Option<Ulid>, PathBuf)> {
        let session = self.sidebar.sessions().iter().find(|s| s.id == session)?;
        let project = (self.sidebar.projects().iter()).find(|p| p.id == session.project_id)?;
        let project_dir = PathBuf::from(&project.path);
        let worktree = session
            .task_id
            .and_then(|id| self.board.task(id))
            .and_then(|task| Some((task.id, task.worktree_path.as_deref()?)));
        Some(match worktree {
            Some((task, root)) => (
                project.id,
                Some(task),
                worktree_target::project_dir(&project_dir, Path::new(root)),
            ),
            None => (project.id, None, project_dir),
        })
    }

    /// Ctrl+Click on a terminal link: a URL opens in the browser, a file in the Files view.
    pub(crate) fn open_terminal_link(
        &mut self,
        session: Ulid,
        link: &OpenLink,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match link {
            OpenLink::Url(url) => cx.open_url(url),
            OpenLink::File { path, line } => {
                let Some((project_id, target, _)) = self.link_scope(session) else {
                    return;
                };
                self.open_file_at(project_id, target, path.clone(), *line, window, cx);
            }
        }
    }

    /// Shows `path` of the Files view on `target` (the project, or a task worktree) with the
    /// caret on `line`. Another target starts the view over, so unsaved text is asked about
    /// first (as the chooser does).
    pub(crate) fn open_file_at(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).worktree_task_id != target {
            if self.files_view(project_id).unsaved().is_some() {
                let again: AfterSave = Box::new(move |shell, window, cx| {
                    shell.open_file_at(project_id, target, path, line, window, cx);
                });
                return self.leave_file(project_id, again, window, cx);
            }
            self.set_files_target(project_id, target, window, cx);
        }
        self.open_revealed(project_id, target, path, line, window, cx);
    }

    /// Puts the caret on the line a link asked for and scrolls there, once the file is shown
    /// (after `reveal` when it already was, after the read otherwise).
    pub(crate) fn show_pending_line(&mut self, project_id: Ulid, target: Option<Ulid>) {
        if let Some(line) =
            (self.files_at(project_id, target)).and_then(FilesView::apply_pending_line)
        {
            self.file_code_scroll
                .scroll_to_item(line, ScrollStrategy::Center);
        }
    }

    /// Quick open's file: asks about unsaved changes first (before the workspace or view
    /// switches, so Cancel leaves everything as it was). The answer's continuation comes back
    /// here, because text typed while a save was in flight is unsaved again.
    pub(crate) fn open_revealed(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let leaves =
            (self.files_at(project_id, target)).is_some_and(|view| view.leaves_unsaved(&path));
        if leaves {
            let reveal: AfterSave = Box::new(move |shell, window, cx| {
                shell.open_revealed(project_id, target, path, line, window, cx);
            });
            return self.leave_file(project_id, reveal, window, cx);
        }
        self.reveal_file(project_id, target, path, line, window, cx);
    }

    /// Shows `path` in the Files view of `project_id` (its folders open, the cursor lands on it).
    pub(crate) fn reveal_file(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.show_view(View::Files, window, cx);
        // Revealed only while the Files view still shows the folder that was searched.
        let Some(view) = self.files_at(project_id, target) else {
            return;
        };
        let requests = view.reveal(&path, line);
        let at = view.rows().iter().position(
            |row| matches!(row, files_view::Row::Entry { entry, .. } if entry.path == path),
        );
        if let Some(at) = at {
            self.files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
        self.files_requests(project_id, requests, window, cx);
        self.show_pending_line(project_id, target);
    }

    /// The command palette (Ctrl+Shift+P): the commands that can act where the keyboard is,
    /// palette-only actions and "Go to" items.
    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let workspace = self.workspace;
        let terminal = self.focused_session().filter(|session| {
            self.panes
                .get(session)
                .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
        });
        let discard = workspace
            .and_then(|id| self.git.get(&id))
            .is_some_and(|view| {
                view.selected
                    .as_ref()
                    .is_some_and(|target| view.discard(target).is_some())
            });
        let task_changes = self
            .selected_card
            .and_then(|id| self.board.task(id))
            .is_some_and(|task| {
                Some(task.project_id) == workspace
                    && task.status == TaskStatus::Review
                    && task.checkpoint.is_some()
            });
        let fan_out = self.selected_card.is_some_and(|id| {
            self.board
                .task(id)
                .is_some_and(|task| Some(task.project_id) == workspace)
                && self.board.can_fan_out(id, self.sidebar.sessions())
                && !self.starting.contains(&id)
        });
        let run_state = workspace.map(|id| self.run_summary(id)).unwrap_or_default();
        let context = palette::Context {
            workspace: workspace.is_some(),
            view: self.view(),
            workspaces: self.sidebar.projects().len(),
            tabs: workspace
                .and_then(|id| self.tabs.get(&id))
                .map_or(0, |tabs| tabs.tabs().len()),
            terminal: terminal.is_some(),
            discard,
            task_changes,
            fan_out,
            agents: workspace.is_some_and(|id| broadcast::has_targets(id, self.sidebar.sessions())),
            agent_pane: self.focused_agent().is_some(),
            markable: self
                .focused_session()
                .is_some_and(|id| self.can_mark_unread(id)),
            run_stopped: workspace.is_some()
                && (run_state.rows.is_empty() || run_state.live() < run_state.rows.len()),
            run_live: run_state.any_live(),
            features: self.features,
        };
        let mut entries = palette::settings_error(&self.keymap, self.settings_error.as_deref());
        entries.extend(palette::keymap_errors(&self.keymap, &self.keymap_errors));
        entries.extend(palette::entries(
            &self.keymap,
            &context,
            self.sidebar.projects(),
            self.sidebar.sessions(),
            self.board.tasks(),
        ));
        self.restore_theme_pick(cx);
        self.workspace_menu = None;
        self.palette = Some(Palette::new(entries));
        self.palette_return = window.focused(cx).map(|focus| (focus, terminal));
        self.palette_scroll.scroll_to_item(0);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// The theme picker (ADR 0078): the palette listing `theme::THEMES`, on the current one.
    pub(crate) fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.restore_theme_pick(cx);
        let current = cx.theme().clone();
        let mut palette = Palette::new(palette::theme_entries(current.name));
        if let Some(index) = theme::THEMES.iter().position(|e| e.name == current.name) {
            palette.pick_entry(index);
        }
        self.workspace_menu = None;
        self.palette_scroll.scroll_to_item(palette.selected);
        self.palette = Some(palette);
        self.palette_return = window.focused(cx).map(|focus| (focus, None));
        self.theme_pick = Some(current);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// Back to the theme the picker opened on (Esc, click outside, or anything replacing it).
    pub(crate) fn restore_theme_pick(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(from) = self.theme_pick.take() {
            cx.set_global(from);
            cx.refresh_windows();
        }
    }

    /// Shows the picked theme without saving it.
    fn preview_theme_pick(&mut self, cx: &mut Context<'_, Self>) {
        let Some(mode) = self.theme_pick.as_ref().map(|from| from.mode) else {
            return;
        };
        let Some(palette::Action::Theme(index)) = self.palette.as_ref().and_then(Palette::picked)
        else {
            return;
        };
        let fonts = cx.global::<theme::Fonts>().clone();
        let shown = Theme {
            mode,
            ..theme::THEMES[index].build()
        };
        cx.set_global(shown.with_fonts(&fonts));
        cx.refresh_windows();
    }

    /// Saves theme `index` to the slot shown and shows it (Enter or a click in the picker). A
    /// failed write keeps it for this session and says so, like the title bar toggle.
    fn choose_theme(&mut self, index: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        let name = theme::THEMES[index].name;
        let mode = cx.theme().mode;
        let dark = theme::slot_is_dark(mode, theme::os_dark(window));
        let mut names = cx.global::<theme::ThemeNames>().clone();
        if dark {
            names.dark = name.into();
        } else {
            names.light = name.into();
        }
        cx.set_global(names);
        theme::refresh(mode, window, cx);
        cx.refresh_windows();
        let value = format!("\"{name}\"");
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, theme::slot_key(dark), &value)
        {
            self.settings_write_failed("the theme", error, cx);
        }
    }

    /// Closes the palette; the keyboard goes back to where it was (unless that pane went away
    /// meanwhile).
    pub(crate) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        // Esc / click outside the theme picker: back to the theme it opened on.
        self.restore_theme_pick(cx);
        self.palette = None;
        match self.palette_return.take() {
            Some((focus, session))
                if session.is_none_or(|session| self.panes.contains_key(&session)) =>
            {
                window.focus(&focus, cx);
            }
            _ => self.focus_pane(window, cx),
        }
        cx.notify();
    }

    /// Typing filters, ↑ / ↓ move the pick (kept in view), Enter runs it, Esc closes.
    pub(crate) fn palette_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = quick_open::key(&event.keystroke) else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(palette) = self.palette.as_mut() else {
            return;
        };
        match key {
            quick_open::Key::Close => return self.close_palette(window, cx),
            quick_open::Key::Open => return self.run_palette_pick(window, cx),
            quick_open::Key::Paste => {
                if let Some(text) = paste {
                    palette.insert(&text);
                }
            }
            quick_open::Key::Move(_) | quick_open::Key::Edit(_) | quick_open::Key::Insert(_) => {
                if let quick_open::Key::Edit(edit) = &key {
                    copy_selection(edit, palette.query(), cx);
                }
                palette.key(key);
            }
        }
        self.palette_scroll.scroll_to_item(palette.selected);
        self.preview_theme_pick(cx);
        cx.notify();
    }

    /// Runs the palette's pick. A command goes through the shell's `run` from where the
    /// keyboard was (terminal commands reach their pane; Git ones need the Git list).
    pub(crate) fn run_palette_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(action) = self.palette.as_ref().and_then(Palette::picked) else {
            return;
        };
        if let palette::Action::Theme(index) = action {
            // Chosen: closing must not restore the old theme.
            self.theme_pick = None;
            self.close_palette(window, cx);
            self.choose_theme(index, window, cx);
            return;
        }
        self.close_palette(window, cx);
        let workspace = self.workspace;
        match action {
            palette::Action::Run(command) => {
                if self.view() == View::Git
                    && command.context().is_some_and(|context| {
                        context == GIT_CONTEXT || context == COMMENTS_CONTEXT
                    })
                {
                    window.focus(&self.git_focus, cx);
                }
                // Deferred; gpui-pre captures the focus just restored now and resolves it in
                // the last rendered frame (root if gone), so terminal commands reach the pane.
                window.dispatch_action(Box::new(Run(command)), cx);
            }
            palette::Action::Theme(_) => {}
            palette::Action::Extra(Extra::PhoneAccess) => self.open_phone_access(window, cx),
            palette::Action::Extra(Extra::CheckUpdates) => self.check_updates(true, window, cx),
            palette::Action::Extra(extra) => {
                let Some(project_id) = workspace else {
                    return;
                };
                match extra {
                    Extra::GitFetch => self.run_git_remote(project_id, Remote::Fetch, window, cx),
                    Extra::GitPush => self.run_git_remote(project_id, Remote::Push, window, cx),
                    Extra::GitDiscard => {
                        let target = self
                            .git
                            .get(&project_id)
                            .and_then(|view| view.selected.clone());
                        if let Some(target) = target {
                            self.open_git_discard(project_id, target, window, cx);
                        }
                    }
                    Extra::ViewTaskChanges => {
                        if let Some(task_id) = self.selected_card {
                            self.view_task_changes(task_id, window, cx);
                        }
                    }
                    Extra::FanOut => {
                        if let Some(task_id) = self.selected_card {
                            self.open_fan_out(task_id, window, cx);
                        }
                    }
                    Extra::Broadcast => self.open_broadcast(project_id, window, cx),
                    // Into the open composer, else one opened on the focused agent pane.
                    Extra::InsertPrompt => {
                        if let Some(session) = self.focused_agent() {
                            if self.composers.shown(session).is_none() {
                                self.open_composer(window, cx);
                            }
                            let target = prompt_picker::Target::Composer(session);
                            self.open_prompt_picker(target, window, cx);
                        }
                    }
                    Extra::Turns => {
                        if let Some(session) = self.focused_agent() {
                            self.open_turns(session, window, cx);
                        }
                    }
                    Extra::FollowInZed => {
                        if let Some(session) = self.focused_agent() {
                            self.toggle_follow(session, cx);
                        }
                    }
                    // The list is built on open: the run state may have changed since.
                    Extra::RunStart => {
                        if self.run_summary(project_id).rows.is_empty() {
                            self.open_run_setup(project_id, true, window, cx);
                        } else {
                            self.run_action(project_id, RunAction::Start, None, window, cx);
                        }
                    }
                    Extra::RunRestart => {
                        self.run_action(project_id, RunAction::Restart, None, window, cx)
                    }
                    Extra::RunStop => {
                        self.run_action(project_id, RunAction::Stop, None, window, cx)
                    }
                    Extra::RunSetup => self.open_run_setup(project_id, false, window, cx),
                    Extra::RunShowTab => self.show_run_tab(project_id, window, cx),
                    Extra::MarkUnread => {
                        if let Some(session) = self.focused_session() {
                            self.mark_unread(session, cx);
                        }
                    }
                    // Handled above: it needs no workspace.
                    Extra::PhoneAccess => self.open_phone_access(window, cx),
                    Extra::CheckUpdates => self.check_updates(true, window, cx),
                }
            }
            palette::Action::GoTo(palette::Target::Workspace(project_id)) => {
                self.activate_workspace(project_id, window, cx);
            }
            palette::Action::GoTo(palette::Target::Session(session_id)) => {
                self.open_session(session_id, window, cx);
            }
            palette::Action::GoTo(palette::Target::Task {
                project_id,
                task_id,
            }) => {
                if workspace != Some(project_id) {
                    self.activate_workspace(project_id, window, cx);
                }
                self.selected_card = Some(task_id);
                self.show_view(View::Board, window, cx);
                cx.notify();
            }
        }
    }

    /// Quick open: the query field over the best matches (matched chars in the accent color,
    /// the pick ringed); a click or Enter opens one in the Files view.
    pub(crate) fn render_quick_open(
        &self,
        open: &QuickOpen,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == open.project_id)
            .map_or_else(String::new, |project| format!(" · {}", project.name));
        let note = match &open.results {
            Results::Loading => Some("Searching…".to_owned()),
            Results::Loaded(matches) if matches.is_empty() && open.query.trim().is_empty() => {
                Some("This workspace has no files yet.".to_owned())
            }
            Results::Loaded(matches) if matches.is_empty() => {
                Some(format!("No file matches “{}”.", open.query.trim()))
            }
            Results::Loaded(_) => None,
            Results::Failed(error) => Some(format!("Cannot search the files: {error}")),
        };
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let hover = theme.surface_raised;
        let rows: Vec<gpui::AnyElement> = open
            .matches()
            .iter()
            .enumerate()
            .map(|(index, found)| {
                let picked = index == open.selected;
                let highlights = quick_open::match_ranges(&found.path, &found.indices)
                    .into_iter()
                    .map(|range| {
                        let style = gpui::HighlightStyle {
                            color: Some(accent),
                            font_weight: Some(FontWeight(weight::SEMIBOLD)),
                            ..gpui::HighlightStyle::default()
                        };
                        (range, style)
                    });
                div()
                    .id(("quick-open-match", index))
                    .flex_none()
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
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
                        if let Some(open) = shell.quick_open.as_mut() {
                            open.pick(index);
                            shell.open_quick_pick(window, cx);
                        }
                    }))
                    .child(gpui::StyledText::new(found.path.clone()).with_highlights(highlights))
                    .into_any_element()
            })
            .collect();
        let panel = div()
            .id("quick-open")
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
            .on_key_down(cx.listener(Self::quick_open_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::quick_open_key_down, cx))
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(if open.composer.is_some() {
                        format!("Insert file{workspace}")
                    } else {
                        format!("Open file{workspace}")
                    }),
            )
            .child(caret_field(
                "quick-open-query",
                &open.query,
                "Type part of a file name or path",
                true,
                theme,
                cx,
                |shell, caret| {
                    if let Some(open) = shell.quick_open.as_mut() {
                        open.query.click(caret.at, caret.extend, caret.clicks);
                    }
                },
            ))
            .when(!rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("quick-open-matches")
                        .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.quick_open_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .children(note.map(|note| {
                let failed = matches!(open.results, Results::Failed(_));
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .when(failed, |el| {
                        el.child(div().text_color(rgb(theme.status_exited)).child("✕"))
                    })
                    .child(
                        div()
                            .text_color(rgb(if failed { theme.text } else { theme.text_muted }))
                            .child(note),
                    )
            }))
            .child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(if open.composer.is_some() {
                        "↑ ↓ to pick · Enter types its path in the composer · Esc types @"
                    } else {
                        "↑ ↓ to pick · Enter opens it in Files · Esc closes"
                    }),
            );
        Self::modal_shade("quick-open-shade", panel, cx)
    }

    /// The command palette: the query field over the matching entries (matched chars in the
    /// accent color, shortcuts muted on the right, the pick ringed); a click or Enter runs one.
    pub(crate) fn render_palette(
        &self,
        palette: &Palette,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let hover = theme.surface_raised;
        let rows: Vec<gpui::AnyElement> = palette
            .rows()
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let picked = index == palette.selected;
                let highlights = row.ranges.into_iter().map(|range| {
                    let style = gpui::HighlightStyle {
                        color: Some(accent),
                        font_weight: Some(FontWeight(weight::SEMIBOLD)),
                        ..gpui::HighlightStyle::default()
                    };
                    (range, style)
                });
                div()
                    .id(("palette-entry", index))
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
                        if let Some(palette) = shell.palette.as_mut() {
                            palette.pick(index);
                            shell.run_palette_pick(window, cx);
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(
                                gpui::StyledText::new(row.entry.title.clone())
                                    .with_highlights(highlights),
                            ),
                    )
                    .when(!row.entry.shortcut.is_empty(), |el| {
                        el.child(
                            div()
                                .flex_none()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(row.entry.shortcut.clone()),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        let empty = rows.is_empty();
        let picking_theme = self.theme_pick.is_some();
        let panel = div()
            .id("palette")
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
            .on_key_down(cx.listener(Self::palette_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::palette_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(if picking_theme {
                        "Select theme"
                    } else {
                        "Command palette"
                    }),
            )
            .child(caret_field(
                "palette-query",
                palette.query(),
                if picking_theme {
                    "Type a theme name"
                } else {
                    "Type a command, workspace, session or task"
                },
                true,
                theme,
                cx,
                |shell, caret| {
                    if let Some(palette) = shell.palette.as_mut() {
                        palette.click(caret.at, caret.extend, caret.clicks);
                    }
                },
            ))
            .when(!empty, |el| {
                el.child(
                    div()
                        .id("palette-entries")
                        .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.palette_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .when(empty, |el| {
                el.child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(format!("Nothing matches “{}”.", palette.query().trim())),
                )
            })
            .child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("↑ ↓ to pick · Enter runs it · Esc closes"),
            );
        Self::modal_shade("palette-shade", panel, cx)
    }
}
