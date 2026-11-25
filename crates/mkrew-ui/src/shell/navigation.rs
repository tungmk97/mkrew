//! Links, file reveal and the palette.

use crate::*;

impl Shell {
    /// `@` in a prompt (a composer, the task dialog): the palette on Files in insert mode, over
    /// its workspace's files; the pick is typed as `composer.file_ref` (§20B.1). Without a
    /// daemon the `@` is typed as is.
    pub(crate) fn open_file_ref_picker(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let project_id = match target {
            composer::Target::Composer(session) => (self.sidebar.sessions().iter())
                .find(|s| s.id == session)
                .map(|s| s.project_id),
            composer::Target::TaskDialog => self.task_dialog.as_ref().map(TaskDialog::project_id),
        };
        let Some(project_id) = project_id.filter(|_| self.connection.is_some()) else {
            self.prompt_typed(target, "@", window, cx);
            cx.notify();
            return;
        };
        // Project-relative paths for the prompt, whatever the tree shows.
        let source = palette::Source::new(project_id, None, None);
        self.restore_theme_pick(window, cx);
        self.palette = Some(Palette::file_ref(source, target));
        self.palette_return = None;
        self.palette_host = self.host_of(window);
        // The task dialog under it stays the open dialog: its late replies (the agent list)
        // still land.
        if target != composer::Target::TaskDialog {
            self.dialog_serial += 1;
        }
        self.palette_scroll.scroll_to_item(0);
        window.focus(&self.dialog_focus, cx);
        self.palette_changed(window, cx);
    }

    /// What an OS file drop types (§20B.5), or `None` after a toast: in a machine window these
    /// files are on this computer, the session or project is not.
    fn dropped_files_text(
        &mut self,
        paths: &ExternalPaths,
        cx: &mut Context<'_, Self>,
    ) -> Option<String> {
        if let Some(name) = self.machine_name().map(str::to_owned) {
            self.toast_info(mkrew_ui::machines::local_files_refused(&name), cx);
            return None;
        }
        Some(composer::dropped_files(paths.paths()))
    }

    /// Files dropped from the OS on `session`'s pane (§20B.5): their absolute paths.
    pub(crate) fn drop_paths(
        &mut self,
        session: Ulid,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(text) = self.dropped_files_text(paths, cx) {
            self.type_into_pane(session, &text, window, cx);
        }
    }

    /// Files dropped from the OS on the task dialog's Prompt box (§20B.5, ADR 0095): typed
    /// like a pane drop. A dialog that is saving takes nothing.
    pub(crate) fn drop_prompt_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let busy = (self.task_dialog.as_ref()).is_none_or(|dialog| dialog.busy);
        if busy {
            return;
        }
        if let Some(text) = self.dropped_files_text(paths, cx)
            && !text.is_empty()
        {
            self.prompt_typed(composer::Target::TaskDialog, &text, window, cx);
            cx.notify();
        }
    }

    /// Files dropped from the OS on a file or diff pane or on the tab strip (§20B.5): each file
    /// inside the workspace root or the notes folder opens as a new tab; one toast says that
    /// the others do not; folders are skipped.
    /// In a machine window these files are on this computer, not on the machine: refused.
    pub(crate) fn drop_os_files(
        &mut self,
        project_id: Ulid,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() {
            return;
        }
        if let Some(name) = self.machine_name().map(str::to_owned) {
            self.toast_info(mkrew_ui::machines::local_files_refused(&name), cx);
            return;
        }
        let Some(root) = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map(|project| PathBuf::from(&project.path))
        else {
            return;
        };
        let files: Vec<PathBuf> = (paths.paths().iter())
            .filter(|path| !path.is_dir())
            .cloned()
            .collect();
        let notes = self.notes_folder();
        let found = mkrew_ui::drop_zone::classify_os_files(&root, notes.as_deref(), &files);
        if found.refused > 0 {
            self.toast_error(mkrew_ui::drop_zone::REFUSED_OS_FILES, cx);
        }
        for path in found.open {
            self.open_document(
                project_id,
                FileKey::project(path),
                None,
                mkrew_ui::documents::Place::NewTab,
                window,
                cx,
            );
        }
    }

    /// The absolute `[notes] folder` the daemon reads and writes files in (`fs.read` / `fs.write`
    /// accept absolute paths inside it); `None` while the folder is empty, relative or unusable.
    /// A folder inside the workspace root is returned too: `classify_os_files` lets the root win,
    /// so those files open by their relative path. OS drops are local files, so this is the
    /// local `settings.toml`; a remote machine's daemon has its own `[notes] folder`, and
    /// `drop_os_files` refuses OS drops in a machine window before it asks.
    fn notes_folder(&self) -> Option<PathBuf> {
        let setting = mkrew_core::settings::load(&self.settings_file)
            .ok()?
            .notes
            .folder;
        match mkrew_core::notes::folder(&setting) {
            Ok(mkrew_core::notes::NotesFolder::Absolute(folder)) => Some(folder),
            _ => None,
        }
    }
    /// Types a drop into `session`'s pane: into its composer draft while the composer is
    /// shown, else into the terminal; no Enter either way. A modal dialog covers the panes,
    /// so a drop on it types nothing.
    pub(crate) fn type_into_pane(
        &mut self,
        session: Ulid,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() || text.is_empty() {
            return;
        }
        self.focus_session_pane(session);
        if self.composers.shown(session).is_some() {
            self.prompt_typed(composer::Target::Composer(session), text, window, cx);
        } else if let Some(pane) = self.panes.get(&session).cloned() {
            pane.update(cx, |pane, cx| pane.type_text(text, cx));
            self.focus_pane(window, cx);
        }
        cx.notify();
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

    /// Ctrl+Click on a terminal link: a URL opens in a browser pane (ADR 0111), a file as a file tab.
    pub(crate) fn open_terminal_link(
        &mut self,
        session: Ulid,
        link: &OpenLink,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match link {
            OpenLink::Url(url) => {
                self.open_web(url.clone(), Some(PaneKey::Session(session)), window, cx)
            }
            OpenLink::File { path, line } => {
                let Some((project_id, target, _)) = self.link_scope(session) else {
                    return;
                };
                let (path, line) = (path.clone(), *line);
                if self.host_of(window) != HostId::Main {
                    return self.in_window(HostId::Main, cx, move |shell, window, cx| {
                        window.activate_window();
                        shell.open_file_at(project_id, target, path, line, window, cx);
                    });
                }
                self.open_file_at(project_id, target, path, line, window, cx);
            }
        }
    }

    /// Shows `path` of the files on `target` (the project, or a task worktree) as a file tab
    /// with the caret on `line`. The file is of that folder for good (`FileKey::task`); the
    /// Files tree follows it.
    pub(crate) fn open_file_at(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.set_files_target(project_id, target, window, cx);
        self.reveal_file(project_id, target, path.clone(), window, cx);
        self.open_document(
            project_id,
            FileKey::new(path, target),
            line.map(|line| (line, 1)),
            documents::Place::NewTab,
            window,
            cx,
        );
    }

    /// Shows `path` in the tree of `project_id` (its folders open, the cursor lands on it). It
    /// does not open the file.
    pub(crate) fn reveal_file(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // Revealed only while the tree still shows the folder that was searched.
        let Some(view) = self.files_at(project_id, target) else {
            return;
        };
        let requests = view.reveal(&path);
        let at = view.rows().iter().position(
            |row| matches!(row, files_view::Row::Entry { entry, .. } if entry.path == path),
        );
        if let Some(at) = at {
            self.files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
        self.files_requests(project_id, requests, window, cx);
    }

    /// The command palette (Ctrl+Shift+P) on All: the commands that can act where the keyboard
    /// is, palette-only actions, "Go to" items and the workspace's files, branches and commits.
    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.open_palette_on(palette::Scope::All, window, cx);
    }

    /// The palette on `scope` (Quick open is Files, Search in files is Search).
    pub(crate) fn open_palette_on(
        &mut self,
        scope: palette::Scope,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // The workspace of the window's tab: the shown one in the main window, the tab's own
        // in a detached one.
        let window_key = self.detached_key(window);
        let workspace = match window_key {
            None => self.workspace,
            Some(_) => self.tab_scope(window).map(|(project, _)| project),
        };
        let view = if window_key.is_some() {
            View::Terminals
        } else {
            self.view()
        };
        let terminal = self.focused_session(window).filter(|session| {
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
        let worktree_task = self
            .selected_card
            .is_some_and(|id| self.on_board(id) && self.board.manages_worktree(id));
        let task_changes = self.selected_card.is_some_and(|id| {
            self.on_board(id)
                && self.board.task(id).is_some_and(|task| {
                    task.status == TaskStatus::Review && task.checkpoint.is_some()
                })
        });
        let fan_out = self.selected_card.is_some_and(|id| {
            self.on_board(id)
                && self.board.can_fan_out(id, self.sidebar.sessions())
                && !self.starting.contains(&id)
        });
        let focused = self.focused_session(window);
        let focused_info = focused
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id))
            .cloned();
        // Offered in the main window only (as in the tab menu), for the tabs on its strip.
        let tab_closable = |right_only| {
            (self.tab_scope(window).filter(|_| window_key.is_none())).is_some_and(
                |(project, tab_id)| {
                    let same = self.same_window(project, tab_id);
                    (self.tabs.get(&project)).is_some_and(|tabs| {
                        !tabs.close_targets(tab_id, right_only, same).is_empty()
                    })
                },
            )
        };
        let run_state = workspace.map(|id| self.run_summary(id)).unwrap_or_default();
        let context = palette::Context {
            workspace: workspace.is_some(),
            view,
            workspaces: self.sidebar.projects().len(),
            tabs: match (window_key, workspace) {
                (Some(key), _) => self.detached.tabs_of(key).len(),
                (None, Some(id)) => self.tabs.get(&id).map_or(0, |tabs| {
                    let main = |tab: &&Tab| self.terminal_host(id, tab.id) == HostId::Main;
                    tabs.tabs().iter().filter(main).count()
                }),
                (None, None) => 0,
            },
            file_pane: self.focused_file(window).is_some(),
            file_tree: self.file_tree_shown(),
            web_pane: self.focused_web(window).is_some(),
            browser: mkrew_ui::browser::available().is_ok(),
            terminal: terminal.is_some(),
            discard,
            git: self.git_shown(),
            diff_pane: self.focused_diff(window).is_some(),
            worktree_task,
            task_changes,
            fan_out,
            agents: workspace.is_some_and(|id| broadcast::has_targets(id, self.sidebar.sessions())),
            agent_pane: self.focused_agent(window).is_some(),
            markable: self
                .focused_session(window)
                .is_some_and(|id| self.can_mark_unread(id)),
            run_stopped: workspace.is_some()
                && (run_state.rows.is_empty() || run_state.live() < run_state.rows.len()),
            run_live: run_state.any_live(),
            tab_others: tab_closable(false),
            tab_right: tab_closable(true),
            pane: focused.is_some(),
            live_session: focused_info.as_ref().is_some_and(|session| {
                inspector::stop_action(Some(session)) != inspector::StopAction::Nothing
            }),
            transcript: self.focused_agent(window).is_some_and(|id| {
                self.usages
                    .get(id)
                    .and_then(|usage| usage.transcript.as_ref())
                    .is_some()
            }),
            chat: focused_info
                .as_ref()
                .is_some_and(|info| self.chat_available(info)),
            surface: focused_info
                .as_ref()
                .is_some_and(|info| self.tsp_pane_view(info) != mkrew_ui::tsp::PaneView::Terminal),
            plan_linked: self.focused_agent(window).is_some_and(|id| {
                (self.sidebar.sessions().iter()).any(|s| s.id == id && s.plan_path.is_some())
            }),
            composer_open: focused.is_some_and(|id| self.composers.shown(id).is_some()),
            approve_card: workspace.is_some_and(|id| {
                !self.jira_side_shown()
                    && (self.board)
                        .approve_target(self.selected_card, self.board_scope(id))
                        .is_some()
            }),
            git_base: workspace
                .and_then(|id| self.git.get(&id))
                .is_some_and(|view| view.section() == git_view::Section::Base),
            git_repos: workspace.is_some_and(|id| self.git_repos_listed(id)),
            jira: self.jira_side_shown(),
            last_error: self.toasts.last_error().is_some(),
            toast: !self.toasts.items().is_empty(),
            update_ready: self.update.is_some() && !self.updating,
            mk_install: matches!(
                mkrew_ui::update::current_install(),
                mkrew_ui::update::InstallKind::MacApp { .. }
            ),
            machine: self.target.is_machine(),
            detached_window: window_key.is_some(),
            features: self.features,
            idle_removable: self.sidebar.idle_agents_and_terminals().len(),
            workspaces_open: self.sidebar.any_expanded(),
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
        entries.extend(palette::agent_entries(
            &self.keymap,
            &context,
            &self.pane_agents(),
        ));
        entries.extend(palette::machine_entries(&context, &self.machine_names()));
        if workspace.is_some() {
            entries.extend(palette::run_entries(&run_state));
        }
        // A detached window lists what acts there.
        if window_key.is_some() {
            entries.retain(|entry| palette::acts_in_detached_window(&entry.action));
        }
        self.restore_theme_pick(window, cx);
        self.workspace_menu = None;
        self.palette_host = self.host_of(window);
        let source = workspace.map(|project_id| self.palette_source(project_id));
        self.palette = Some(Palette::scoped(
            entries,
            source,
            scope,
            self.palette_recent.clone(),
        ));
        self.palette_return = window.focused(cx).map(|focus| (focus, terminal));
        self.palette_scroll.scroll_to_item(0);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.palette_changed(window, cx);
    }

    /// What the palette's Files, Search, Branches and Commits read for `project_id`: the task
    /// worktree its Files view shows (the folder `fs.find` lists), and the repository its Git view
    /// shows while that view is on the same worktree (`Source::new` drops the repo of a
    /// worktree anyway).
    fn palette_source(&self, project_id: Ulid) -> palette::Source {
        let worktree = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        let repo = (self.git.get(&project_id))
            .filter(|view| view.worktree_task_id == worktree)
            .and_then(|view| view.repo.clone());
        palette::Source::new(project_id, worktree, repo)
    }

    /// After any change of the palette: sends the calls it now wants, keeps the pick in view
    /// (the list's children are its lines), previews a picked theme and redraws.
    pub(crate) fn palette_changed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let fetches = (self.palette.as_mut()).map_or_else(Vec::new, Palette::wanted);
        self.run_palette_fetches(fetches, window, cx);
        if let Some(palette) = self.palette.as_ref() {
            self.palette_scroll.scroll_to_item(palette.selected_line());
        }
        self.preview_theme_pick(cx);
        cx.notify();
    }

    /// Sends the daemon calls of the open palette. A call with a delay waits, then checks it
    /// is still the palette's latest (a newer keystroke made it stale); an answer goes back by
    /// its serial, so an old one, or one for a palette that closed, is dropped there.
    fn run_palette_fetches(
        &mut self,
        fetches: Vec<palette::Fetch>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if fetches.is_empty() {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            if let Some(palette) = self.palette.as_mut() {
                for fetch in fetches {
                    let failed = fetch.call.failed("Not connected to the daemon.");
                    palette.answer(fetch.serial, failed, status::now_ms());
                }
            }
            return;
        };
        for fetch in fetches {
            let palette::Fetch {
                scope,
                serial,
                delay,
                call,
            } = fetch;
            let connection = connection.clone();
            cx.spawn_in(window, async move |this, cx| {
                if !delay.is_zero() {
                    cx.background_executor().timer(delay).await;
                    let current = this.update(cx, |shell, _| {
                        (shell.palette.as_ref())
                            .is_some_and(|palette| palette.is_current(scope, serial))
                    });
                    if !current.unwrap_or(false) {
                        return;
                    }
                }
                let answer = match call {
                    palette::Call::Files(params) => palette::Answer::Files(
                        (connection
                            .call::<Vec<FsMatch>>(Request::FsFind(params))
                            .await)
                            .map_err(|error| format!("{error:#}")),
                    ),
                    palette::Call::Search(params) => palette::Answer::Search(
                        (connection
                            .call::<mkrew_protocol::FsSearchResult>(Request::FsSearch(params))
                            .await)
                            .map_err(|error| format!("{error:#}")),
                    ),
                    palette::Call::Branches(params) => palette::Answer::Branches(
                        (connection
                            .call::<Vec<mkrew_protocol::Branch>>(Request::GitBranches(params))
                            .await)
                            .map_err(|error| format!("{error:#}")),
                    ),
                    palette::Call::Commits(params) => palette::Answer::Commits(
                        (connection
                            .call::<Vec<Commit>>(Request::GitLog(params))
                            .await)
                            .map_err(|error| format!("{error:#}")),
                    ),
                };
                let _ = this.update(cx, |shell, cx| {
                    if let Some(palette) = shell.palette.as_mut()
                        && palette.answer(serial, answer, status::now_ms())
                    {
                        let line = palette.selected_line();
                        shell.palette_scroll.scroll_to_item(line);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    /// The theme picker (ADR 0078): the palette listing the theme catalog, on the current one.
    pub(crate) fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.restore_theme_pick(window, cx);
        let current = cx.theme().clone();
        let catalog = cx.global::<ThemeCatalog>();
        let mut palette = Palette::new(palette::theme_entries(catalog, &current.name));
        if let Some(index) = catalog.index_of(&current.name) {
            palette.pick_entry(index);
        }
        self.workspace_menu = None;
        self.palette_scroll.scroll_to_item(palette.selected_line());
        self.palette_host = self.host_of(window);
        self.palette = Some(palette);
        let terminal = self.focused_session(window).filter(|session| {
            self.panes
                .get(session)
                .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
        });
        self.palette_return = window.focused(cx).map(|focus| (focus, terminal));
        self.theme_pick = Some(current.mode);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// Back to the current theme for the mode the picker or a gallery preview started on (Esc,
    /// or anything replacing it); rebuilt, so an OS appearance or settings change meanwhile
    /// is kept.
    pub(crate) fn restore_theme_pick(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        let picked = self.theme_pick.take();
        let previewed = self.gallery_preview.take();
        if let Some(dialog) = self.settings_dialog.as_mut() {
            dialog.end_preview();
        }
        if let Some(mode) = picked.or(previewed.map(|(mode, _)| mode)) {
            theme::refresh(mode, window, cx);
            cx.refresh_windows();
        }
    }

    /// Shows the picked theme without saving it.
    fn preview_theme_pick(&mut self, cx: &mut Context<'_, Self>) {
        let Some(mode) = self.theme_pick else {
            return;
        };
        let Some(palette::Action::Theme(index)) = self.palette.as_ref().and_then(Palette::picked)
        else {
            return;
        };
        let fonts = cx.global::<theme::Fonts>().clone();
        let Some(listed) = cx.global::<ThemeCatalog>().themes().get(index) else {
            return;
        };
        let shown = Theme {
            mode,
            ..listed.theme.clone()
        };
        cx.set_global(shown.with_fonts(&fonts));
        cx.refresh_windows();
    }

    /// Saves theme `index` to the slot shown and shows it (Enter or a click in the picker).
    fn choose_theme(&mut self, index: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        // The catalog may have been rebuilt since the picker listed it.
        let Some(name) = (cx.global::<ThemeCatalog>().themes().get(index)).map(|t| t.name.clone())
        else {
            return;
        };
        let dark = theme::slot_is_dark(cx.theme().mode, theme::os_dark(window));
        self.keep_theme(&name, dark, window, cx);
    }

    /// Saves `name` to the slot of `dark` and shows the kept theme of the slot that is shown
    /// (a theme of the other appearance changes the other slot only). A failed write keeps
    /// the choice for this session and says so, like the title bar toggle.
    pub(crate) fn keep_theme(
        &mut self,
        name: &str,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let mode = cx.theme().mode;
        let mut names = cx.global::<theme::ThemeNames>().clone();
        if dark {
            names.dark = name.into();
        } else {
            names.light = name.into();
        }
        cx.set_global(names);
        self.gallery_preview = None;
        theme::refresh(mode, window, cx);
        cx.refresh_windows();
        let value = theme::toml_string(name);
        match mkrew_core::settings::write_ui_value(
            &self.settings_file,
            theme::slot_key(dark),
            &value,
        ) {
            Err(error) => self.settings_write_failed("the theme", error, cx),
            Ok(()) => {
                // Now, not at the next mtime poll: the dialog's rows show the new value, and a
                // hand edit saved in the same poll window applies too.
                self.settings_modified = std::fs::metadata(&self.settings_file)
                    .and_then(|m| m.modified())
                    .ok();
                self.apply_settings_file(window, cx);
            }
        }
    }

    /// Closes the palette; the keyboard goes back to where it was (unless that pane went away
    /// meanwhile).
    pub(crate) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        // Esc from the theme picker: back to the theme it opened on.
        self.restore_theme_pick(window, cx);
        let insert_into = self
            .palette
            .take()
            .and_then(|palette| palette.insert_into());
        self.palette_host = HostId::Main;
        // Esc on an `@` picker types the `@` it replaced (an e-mail, a handle).
        if let Some(target) = insert_into {
            self.palette_return = None;
            self.prompt_typed(target, "@", window, cx);
            cx.notify();
            return;
        }
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

    /// Typing filters, Tab / Shift+Tab switch the scope, ↑ / ↓ move the pick (kept in view),
    /// Enter runs it, Esc closes.
    pub(crate) fn palette_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        if let Some(by) = palette::scope_step(&event.keystroke) {
            if let Some(palette) = self.palette.as_mut() {
                palette.step_scope(by);
            }
            return self.palette_changed(window, cx);
        }
        let Some(key) = palette::key(&event.keystroke) else {
            return;
        };
        let paste = (key == palette::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(palette) = self.palette.as_mut() else {
            return;
        };
        match key {
            palette::Key::Close => return self.close_palette(window, cx),
            palette::Key::Open => return self.run_palette_pick(window, cx),
            palette::Key::Paste => {
                if let Some(text) = paste {
                    palette.insert(&text);
                }
            }
            palette::Key::Move(_) | palette::Key::Edit(_) | palette::Key::Insert(_) => {
                if let palette::Key::Edit(edit) = &key {
                    copy_selection(edit, palette.query(), cx);
                }
                palette.key(key);
            }
        }
        self.palette_changed(window, cx);
    }

    /// Runs the palette's pick. A command goes through the shell's `run` from where the
    /// keyboard was (terminal commands reach their pane; Git ones need the Git list).
    pub(crate) fn run_palette_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        // "+N more": the scope of that section.
        if let Some(scope) = self.palette.as_ref().and_then(Palette::picked_more) {
            if let Some(palette) = self.palette.as_mut() {
                palette.set_scope(scope);
            }
            return self.palette_changed(window, cx);
        }
        // `@` in a prompt: the path is typed there, nothing is opened.
        if let Some(target) = self.palette.as_ref().and_then(Palette::insert_into) {
            return self.insert_palette_pick(target, window, cx);
        }
        let Some(action) = self.palette.as_ref().and_then(Palette::picked) else {
            return;
        };
        let source = (self.palette.as_ref()).and_then(|palette| palette.source().cloned());
        palette::remember(&mut self.palette_recent, &action);
        if let palette::Action::Theme(index) = action {
            // Chosen: closing must not restore the old theme.
            self.theme_pick = None;
            self.close_palette(window, cx);
            self.choose_theme(index, window, cx);
            return;
        }
        self.close_palette(window, cx);
        // A detached window's own extras; they need no workspace of the main window.
        if let Some(key) = self.detached_key(window) {
            match action {
                palette::Action::Extra(Extra::TabMoveToMainWindow) => {
                    if let Some((project, tab_id)) = self.tab_scope(window) {
                        let tab = DockTab::Terminal { project, tab_id };
                        self.move_tab_to(tab, HostId::Main, None, window, cx);
                    }
                    return;
                }
                palette::Action::Extra(Extra::WindowMoveAllBack) => {
                    return self.move_all_back(key, window, cx);
                }
                // The ✕ of the window: its tabs close for real, one question for the agents.
                palette::Action::Extra(Extra::WindowClose) => {
                    self.detached_should_close(key, window, cx);
                    return;
                }
                palette::Action::Extra(Extra::TabPin) => {
                    if let Some((project, tab_id)) = self.tab_scope(window) {
                        let pinned = (self.tabs.get(&project))
                            .and_then(|tabs| tabs.tab(tab_id))
                            .is_some_and(|tab| tab.pinned);
                        self.pin_tab(tab_id, !pinned, window, cx);
                    }
                    return;
                }
                _ => {}
            }
        }
        let workspace = self.workspace;
        match action {
            palette::Action::Run(command) => {
                if self.git_shown()
                    && command.context().is_some_and(|context| {
                        context == GIT_CONTEXT || context == COMMENTS_CONTEXT
                    })
                {
                    window.focus(&self.git_focus, cx);
                } else if self.file_tree_shown() && command.context() == Some(FILE_TREE_CONTEXT) {
                    window.focus(&self.files_focus, cx);
                }
                // Deferred; gpui-pre captures the focus just restored now and resolves it in
                // the last rendered frame (root if gone), so terminal commands reach the pane.
                window.dispatch_action(Box::new(Run(command)), cx);
            }
            palette::Action::Theme(_) => {}
            palette::Action::NewAgentPane(index) => {
                let agent = self
                    .agent_profiles
                    .get(index)
                    .map(|profile| profile.id.clone());
                if let Some(agent) = agent {
                    // Like "New agent pane": the new pane is seen in Terminals.
                    self.show_view(View::Terminals, window, cx);
                    self.new_pane_with(SessionKind::Agent, Some(agent), Axis::Vertical, window, cx);
                }
            }
            palette::Action::OpenMachine(index) => self.open_machine_at(index),
            palette::Action::Extra(Extra::AddMachine) => self.open_machine_add(window, cx),
            palette::Action::Extra(Extra::PhoneAccess) => self.open_phone_access(window, cx),
            palette::Action::Extra(Extra::Ports) => self.open_ports(window, cx),
            palette::Action::Extra(Extra::ManagePrompts) => self.open_prompts_settings(window, cx),
            palette::Action::Extra(Extra::NewThemeFromCurrent) => self.new_theme_from_current(cx),
            palette::Action::Extra(Extra::OpenThemesFolder) => self.open_themes_folder(cx),
            palette::Action::Extra(Extra::CheckUpdates) => self.check_updates(true, window, cx),
            palette::Action::Extra(Extra::CopyLastError) => self.copy_last_error(cx),
            palette::Action::Extra(Extra::DismissToasts) => {
                self.toasts.clear();
                cx.notify();
            }
            palette::Action::Extra(Extra::InstallUpdate) => self.start_update(window, cx),
            palette::Action::Extra(Extra::InstallMkCommand) => self.install_mk_command(cx),
            palette::Action::Extra(Extra::SidebarRemoveIdle) => {
                self.remove_idle_agents(window, cx);
            }
            palette::Action::Extra(Extra::SidebarToggleAll) => {
                self.sidebar.toggle_all();
                cx.notify();
            }
            palette::Action::RunCommand(action, index) => {
                let Some(project_id) = workspace else {
                    return;
                };
                // The row is resolved now: a command removed meanwhile is simply not found.
                let name =
                    (self.run_summary(project_id).rows.get(index)).map(|row| row.name.clone());
                if let Some(name) = name {
                    self.run_action(project_id, action, Some(name), window, cx);
                }
            }
            palette::Action::Extra(extra) => {
                let Some(project_id) = workspace else {
                    return;
                };
                match extra {
                    Extra::ViewTaskChanges => {
                        if let Some(task_id) = self.selected_card {
                            self.view_task_changes(task_id, window, cx);
                        }
                    }
                    Extra::MergeBack | Extra::RemoveWorktree => {
                        let kind = match extra {
                            Extra::MergeBack => worktree_dialog::Kind::Merge,
                            _ => worktree_dialog::Kind::Remove,
                        };
                        if let Some(task_id) = self
                            .selected_card
                            .filter(|&id| self.board.manages_worktree(id) && self.on_board(id))
                        {
                            self.open_worktree_dialog(kind, task_id, window, cx);
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
                        if let Some(session) = self.focused_agent(window) {
                            if self.composers.shown(session).is_none() {
                                self.open_composer(window, cx);
                            }
                            let target = composer::Target::Composer(session);
                            self.open_prompt_picker(target, window, cx);
                        }
                    }
                    Extra::Turns => {
                        if let Some(session) = self.focused_agent(window) {
                            self.open_turns(session, window, cx);
                        }
                    }
                    Extra::FollowInZed => {
                        if let Some(session) = self.focused_agent(window) {
                            self.toggle_follow(session, cx);
                        }
                    }
                    Extra::RunSetup => self.open_run_setup(project_id, false, window, cx),
                    Extra::RunShowTab => self.show_run_tab(project_id, window, cx),
                    Extra::WorkspacePin => {
                        let pinned = (self.sidebar.projects().iter())
                            .find(|p| p.id == project_id)
                            .is_some_and(|p| p.pinned);
                        self.pin_workspace(project_id, !pinned, window, cx);
                    }
                    Extra::WorkspaceExplorer => self.open_workspace_in_explorer(project_id, cx),
                    // The default agent is chosen in the workspace's menu.
                    Extra::WorkspaceDefaultAgent => {
                        self.open_workspace_menu_at_switcher(project_id, window, cx);
                        self.menu_select_after(DEFAULT_AGENT_HEADING, window);
                    }
                    Extra::WorkspaceMenu => {
                        self.open_workspace_menu_at_switcher(project_id, window, cx);
                    }
                    Extra::WorkspaceRemove => self.archive_workspace(project_id, window, cx),
                    Extra::TabPin => {
                        let active = (self.tabs.get(&project_id))
                            .and_then(Tabs::active)
                            .map(|tab| (tab.id, tab.pinned));
                        if let Some((tab_id, pinned)) = active {
                            self.pin_tab(tab_id, !pinned, window, cx);
                        }
                    }
                    Extra::TabCloseOthers | Extra::TabCloseRight => {
                        let active = (self.tabs.get(&project_id))
                            .and_then(Tabs::active)
                            .map(|tab| tab.id);
                        if let Some(tab_id) = active {
                            let right_only = extra == Extra::TabCloseRight;
                            self.close_other_tabs(tab_id, right_only, window, cx);
                        }
                    }
                    // Handled above: a detached window's own entries.
                    Extra::TabMoveToMainWindow | Extra::WindowMoveAllBack | Extra::WindowClose => {}
                    Extra::PanePin => {
                        let pane = self.focused_session(window).and_then(|id| {
                            let session = self.sidebar.sessions().iter().find(|s| s.id == id)?;
                            Some((id, session.pinned))
                        });
                        if let Some((session, pinned)) = pane {
                            self.pin_session(session, !pinned, window, cx);
                        }
                    }
                    Extra::PaneReading => {
                        if let Some(session) = self.focused_agent(window) {
                            self.toggle_reading_width(session, cx);
                        }
                    }
                    // Opens the workspace menu at the switcher with the shell list selected.
                    Extra::WorkspaceDefaultShell => {
                        self.open_workspace_menu_at_switcher(project_id, window, cx);
                        self.menu_select_after(DEFAULT_SHELL_HEADING, window);
                    }
                    Extra::WorkspaceMoveUp => self.move_workspace(project_id, -1, window, cx),
                    Extra::WorkspaceMoveDown => self.move_workspace(project_id, 1, window, cx),
                    Extra::AgentStop => {
                        if let Some(session) = self.focused_session(window) {
                            self.stop_session(session, window, cx);
                        }
                    }
                    Extra::AgentTranscript => {
                        if let Some(session) = self.focused_agent(window) {
                            self.open_transcript(session, cx);
                        }
                    }
                    Extra::AgentLinkPlan => {
                        if let Some(session) = self.focused_agent(window) {
                            self.pick_plan(session, window, cx);
                        }
                    }
                    Extra::AgentUnlinkPlan => {
                        if let Some(session) = self.focused_agent(window) {
                            self.link_plan(session, None, window, cx);
                        }
                    }
                    Extra::AgentImprove => {
                        if let Some(session) = self.composer_session(window) {
                            self.enhance_prompt(composer::Target::Composer(session), window, cx);
                        }
                    }
                    Extra::MarkUnread => {
                        if let Some(session) = self.focused_session(window) {
                            self.mark_unread(session, cx);
                        }
                    }
                    Extra::GitChangeBase => self.open_git_base(project_id, window, cx),
                    Extra::GitSwitchRepo => {
                        // The menu hangs on the repo strip: show the Git tab first.
                        self.show_git_panel(window, cx);
                        self.open_git_menu(GitMenu::Repos, window, cx);
                    }
                    Extra::BoardScope => self.set_board_all(!self.board_all, cx),
                    Extra::JiraMine => {
                        self.set_jira_mine(project_id, !self.jira_mine, window, cx);
                    }
                    Extra::JiraLayout => {
                        let other = match self.jira_layout {
                            mkrew_core::settings::JiraLayout::List => {
                                mkrew_core::settings::JiraLayout::Kanban
                            }
                            mkrew_core::settings::JiraLayout::Kanban => {
                                mkrew_core::settings::JiraLayout::List
                            }
                        };
                        self.set_jira_layout(other, cx);
                    }
                    Extra::JiraRefresh => self.load_jira(project_id, window, cx),
                    Extra::History => self.show_history(project_id, window, cx),
                    // Handled above: they need no workspace.
                    Extra::CopyLastError
                    | Extra::DismissToasts
                    | Extra::InstallUpdate
                    | Extra::InstallMkCommand => {}
                    // Handled above: it needs no workspace.
                    Extra::PhoneAccess => self.open_phone_access(window, cx),
                    // Handled above: it needs no workspace.
                    Extra::Ports => self.open_ports(window, cx),
                    // Handled above: it needs no workspace.
                    Extra::AddMachine => self.open_machine_add(window, cx),
                    // Handled above: it needs no workspace.
                    Extra::ManagePrompts => self.open_prompts_settings(window, cx),
                    // Handled above: they need no workspace.
                    Extra::NewThemeFromCurrent
                    | Extra::OpenThemesFolder
                    | Extra::SidebarRemoveIdle
                    | Extra::SidebarToggleAll => {}
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
            palette::Action::OpenFile { path } => {
                if let Some(source) = source {
                    self.open_palette_file(source, path, None, window, cx);
                }
            }
            palette::Action::OpenSearchHit { path, line } => {
                if let Some(source) = source {
                    self.open_palette_file(source, path, Some(line), window, cx);
                }
            }
            palette::Action::Checkout {
                current: true,
                branch,
                ..
            } => self.toast_info(format!("Already on {branch}"), cx),
            palette::Action::Checkout {
                repo,
                branch,
                current: false,
            } => {
                if let Some(source) = source {
                    self.in_main_window(window, cx, move |shell, window, cx| {
                        shell.checkout_branch(source, repo, branch, window, cx);
                    });
                }
            }
            palette::Action::ShowCommit { repo, sha } => {
                if let Some(source) = source {
                    self.in_main_window(window, cx, move |shell, window, cx| {
                        shell.show_palette_commit(source, repo, sha, window, cx);
                    });
                }
            }
        }
    }

    /// Enter on an `@` picker: types the picked file as `composer.file_ref` into `target`.
    fn insert_palette_pick(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(palette::Action::OpenFile { path }) =
            self.palette.as_ref().and_then(Palette::picked)
        else {
            return;
        };
        self.palette = None;
        self.palette_host = HostId::Main;
        // A missing or broken settings file types the default form (the daemon reports it).
        let template = mkrew_core::settings::load(&self.settings_file)
            .unwrap_or_default()
            .composer
            .file_ref;
        let text = composer::file_ref(&template, &path);
        self.prompt_typed(target, &text, window, cx);
        cx.notify();
    }

    /// Runs `run` in the main window: at once when the palette was opened there, else after
    /// activating that window. Files, checkouts and commits all live in the main window (the
    /// Editor and Git views, the title bar summary), also when a detached window's palette
    /// picked them.
    pub(crate) fn in_main_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
        run: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
    ) {
        if self.host_of(window) == HostId::Main {
            return run(self, window, cx);
        }
        self.in_window(HostId::Main, cx, move |shell, window, cx| {
            window.activate_window();
            run(shell, window, cx);
        });
    }

    /// Files and Search picks: the file in a file pane (its folders open, the cursor lands
    /// on it), the caret on `line` for a search hit. A detached window's pick opens in the
    /// main one.
    fn open_palette_file(
        &mut self,
        source: palette::Source,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (project, target) = (source.project_id, source.worktree_task_id);
        self.in_main_window(window, cx, move |shell, window, cx| {
            shell.open_file_at(project, target, path, line, window, cx);
        });
    }

    /// A Branches pick: `git.checkout`, after a question when something is at risk (changed
    /// files in the repository, live agents in that folder). A failure is a toast with git's
    /// own message; a success refreshes the Git view. Runs in the main window
    /// ([`Self::in_main_window`]) with the palette's workspace active. `repo_changes` reads
    /// that workspace's title bar summary; activating a workspace clears it and loads it
    /// again, so while it is missing the summary is read here before the question.
    fn checkout_branch(
        &mut self,
        source: palette::Source,
        repo: Option<String>,
        branch: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.workspace != Some(source.project_id) {
            self.activate_workspace(source.project_id, window, cx);
        }
        // ponytail: a task worktree's own changes are not in the title bar summary, so no
        // question is asked there; git refuses a switch that would overwrite a change.
        let known = if source.worktree_task_id.is_none() {
            self.repo_changes(repo.as_deref())
        } else {
            Some(0)
        };
        let has_worktree =
            |task: Ulid| (self.board.task(task)).is_some_and(|task| task.worktree_path.is_some());
        let agents = palette::agents_in_folder(
            self.sidebar.sessions(),
            source.project_id,
            source.worktree_task_id,
            has_worktree,
        );
        let params = GitCheckoutParams {
            project_id: source.project_id,
            worktree_task_id: source.worktree_task_id,
            repo,
            branch,
        };
        cx.spawn_in(window, async move |this, cx| {
            let changes = match known {
                Some(changes) => changes,
                // A summary that cannot be read asks no question (git refuses a clobber).
                None => crate::shell::git::git_summary_of(&connection, params.project_id)
                    .await
                    .ok()
                    .and_then(|summary| {
                        let repo = params.repo.as_deref().unwrap_or("");
                        let found = summary.repos.iter().find(|found| found.repo == repo);
                        found.map(RepoSummary::changed)
                    })
                    .unwrap_or(0),
            };
            let question = palette::checkout_question(&params.branch, changes, agents);
            if let Some((message, detail)) = question {
                let Ok(answer) = this.update_in(cx, |_, window, cx| {
                    window.prompt(
                        PromptLevel::Warning,
                        &message,
                        Some(detail.as_str()),
                        &palette::CHECKOUT_ANSWERS,
                        cx,
                    )
                }) else {
                    return;
                };
                if !palette::confirms_checkout(answer.await.ok()) {
                    return;
                }
            }
            let project_id = params.project_id;
            let branch = params.branch.clone();
            let done = connection.call::<()>(Request::GitCheckout(params)).await;
            let _ = this.update_in(cx, |shell, window, cx| match done {
                Ok(()) => {
                    shell.toast_info(format!("Switched to {branch}"), cx);
                    if shell.git.contains_key(&project_id) {
                        shell.refresh_git(project_id, window, cx);
                    }
                }
                Err(error) => shell.toast_error(format!("{error:#}"), cx),
            });
        })
        .detach();
    }

    /// A Commits pick: the Git view's History on that commit (its log is read long enough to
    /// list it, see `GitView::show_commit`).
    fn show_palette_commit(
        &mut self,
        source: palette::Source,
        repo: Option<String>,
        sha: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let project_id = source.project_id;
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.retarget_git(project_id, source.worktree_task_id, repo);
        let requests = self.git_view(project_id).show_commit(sha);
        self.open_git(project_id, window, cx);
        // A second `git.log` while the first is in flight is coalesced by `begin_log`.
        self.git_requests(project_id, requests, window, cx);
        cx.notify();
    }

    /// The palette: the query field, the scope pills, then the lines of the scope (overlines,
    /// rows with their matched chars in the accent color, "+N more", notes) and the key hints.
    /// A click on a pill switches scope; a click on a row runs it.
    pub(crate) fn render_palette(
        &self,
        palette: &Palette,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let highlights = move |ranges: Vec<std::ops::Range<usize>>| {
            ranges
                .into_iter()
                .map(|range| {
                    let style = gpui::HighlightStyle {
                        color: Some(accent),
                        font_weight: Some(FontWeight(weight::SEMIBOLD)),
                        ..gpui::HighlightStyle::default()
                    };
                    (range, style)
                })
                .collect::<Vec<_>>()
        };
        let muted = |text: String| {
            div()
                .flex_none()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let lines = palette.lines();
        let empty = lines.is_empty();
        let rows: Vec<gpui::AnyElement> = lines
            .into_iter()
            .enumerate()
            .map(|(at, line)| match line {
                palette::Line::Header(title) => overline(title, theme)
                    .flex_none()
                    .px(px(space::M))
                    .pt(px(space::S))
                    .pb(px(space::XS))
                    .into_any_element(),
                palette::Line::Note { text, error } => div()
                    .flex_none()
                    .flex()
                    .gap(px(space::XS))
                    .px(px(space::M))
                    .py(px(space::XS))
                    .text_size(font_size::META)
                    .when(error, |el| {
                        el.child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.danger_text))
                    })
                    .child(
                        div()
                            .text_color(rgb(if error { theme.text } else { theme.text_muted }))
                            .child(text),
                    )
                    .into_any_element(),
                palette::Line::More {
                    scope,
                    hidden,
                    pick,
                } => list_row(("palette-more", at), pick == palette.selected, theme)
                    .flex_none()
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        if let Some(palette) = shell.palette.as_mut() {
                            palette.set_scope(scope);
                        }
                        shell.palette_changed(window, cx);
                    }))
                    .child(muted(format!("+{hidden} more ›")))
                    .into_any_element(),
                palette::Line::Row { row, pick } => {
                    let title = gpui::StyledText::new(row.entry.title.clone())
                        .with_highlights(highlights(row.ranges));
                    let has_detail = row.detail.is_some();
                    list_row(("palette-row", at), pick == palette.selected, theme)
                        .flex_none()
                        .gap(px(space::M))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            if let Some(palette) = shell.palette.as_mut() {
                                palette.pick(pick);
                            }
                            shell.run_palette_pick(window, cx);
                        }))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .when(has_detail, |el| {
                                    el.flex_none()
                                        .max_w(relative(theme::PALETTE_TITLE_MAX_FRACTION))
                                })
                                .when(!has_detail, |el| el.flex_1())
                                .child(title),
                        )
                        .children(row.detail.map(|detail| {
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(detail.to_owned())
                        }))
                        .children(row.age.map(|age| muted(age.to_owned())))
                        .when(!row.entry.shortcut.is_empty(), |el| {
                            // The flat theme picker keeps its muted "Dark" / "✓ Light" text.
                            if palette.tabs() {
                                el.child(kbd(row.entry.shortcut.clone(), theme, false))
                            } else {
                                el.child(muted(row.entry.shortcut.clone()))
                            }
                        })
                        .into_any_element()
                }
            })
            .collect();

        let picking_theme = self.theme_pick.is_some();
        let workspace = palette
            .source()
            .and_then(|source| {
                (self.sidebar.projects().iter()).find(|project| project.id == source.project_id)
            })
            .map_or_else(String::new, |project| format!(" · {}", project.name));
        let title = if picking_theme {
            Some("Select theme".to_owned())
        } else if palette.insert_into().is_some() {
            Some(format!("Insert file{workspace}"))
        } else {
            None
        };
        // Pills, not `segment`s: the current scope is outlined in the accent, no fill.
        let tabs = palette.tabs().then(|| {
            div().flex().items_center().gap(px(space::XS)).children(
                Scope::ORDER.iter().enumerate().map(|(index, scope)| {
                    let scope = *scope;
                    let current = scope == palette.scope();
                    div()
                        .id(("palette-scope", index))
                        .flex_none()
                        .flex()
                        .items_center()
                        .h(theme::rems_px(theme::SEGMENT_HEIGHT))
                        .px(px(space::M))
                        .rounded(px(radius::PILL))
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(if current {
                            gpui::Hsla::from(rgb(theme.accent))
                        } else {
                            gpui::transparent_black()
                        })
                        .whitespace_nowrap()
                        .text_size(font_size::BODY)
                        .text_color(rgb(if current {
                            theme.text
                        } else {
                            theme.text_muted
                        }))
                        .pressable_unless(current, Look::Chip, theme)
                        .on_press(
                            true,
                            cx.listener(move |shell, _, window, cx| {
                                if let Some(palette) = shell.palette.as_mut() {
                                    palette.set_scope(scope);
                                }
                                shell.palette_changed(window, cx);
                            }),
                        )
                        .child(scope.title())
                }),
            )
        });
        let (hint, close): (&str, &str) = if picking_theme {
            ("↑↓ preview · ↵ save", "esc restore")
        } else if palette.insert_into().is_some() {
            ("↑↓ navigate · ↵ type its path", "esc type @")
        } else {
            ("↑↓ navigate · ↵ run · Tab next scope", "esc close")
        };
        let panel = floating_panel("palette", theme)
            .w(px(theme::PALETTE_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::L))
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::palette_key_down))
            .child(self.typing(&self.dialog_focus, Self::palette_key_down, cx))
            .children(title.map(|title| {
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(title)
            }))
            .child(search_box(
                caret_field(
                    "palette-query",
                    palette.query(),
                    if picking_theme {
                        "Type a theme name"
                    } else {
                        palette.scope().placeholder()
                    },
                    true,
                    theme,
                    cx,
                    |shell, caret| {
                        if let Some(palette) = shell.palette.as_mut() {
                            palette.click(caret.at, caret.extend, caret.clicks);
                        }
                    },
                ),
                Some(kbd("esc".to_owned(), theme, false).into_any_element()),
                true,
                theme,
            ))
            .children(tabs)
            .when(!empty, |el| {
                el.child(
                    div()
                        .id("palette-entries")
                        .max_h(px(theme::PALETTE_LIST_HEIGHT))
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
                        .child(match palette.query().trim() {
                            "" => palette.scope().empty().to_owned(),
                            query => format!("Nothing matches “{query}”."),
                        }),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(hint)
                    .child(close),
            );
        Self::modal_shade_top("palette-shade", panel, theme::PALETTE_TOP_OFFSET, cx)
    }
}
