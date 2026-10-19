//! New panes, the default-agent prompt and the action dispatcher `Shell::run`.

use crate::*;

impl Shell {
    /// Starts an agent or a shell (the workspace's default of that kind) in the workspace folder
    /// and shows it next to the focused pane of the active tab, split along `axis`.
    pub(crate) fn new_pane(
        &mut self,
        kind: SessionKind,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(tab_id) = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id)
        else {
            return;
        };
        if self.starting_pane.is_some() || self.ask_for_default(project_id, kind, window, cx) {
            return;
        }
        self.starting_pane = Some(kind);
        cx.notify();
        let params = lifecycle::spawn_params(project_id, Some(tab_id), kind);
        let spawned = connection.call::<SessionSpawnResult>(Request::SessionSpawn(params));
        cx.spawn_in(window, async move |this, cx| {
            let spawned = spawned.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.starting_pane = None;
                match spawned {
                    Ok(spawned) => {
                        // Next to the focused pane of the tab active now (the user may have
                        // switched tabs meanwhile), unless the daemon placed it already.
                        shell.place_session(project_id, spawned.session_id, axis, window, cx);
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "session.spawn failed: {error:#}");
                        let what = lifecycle::kind_noun(kind);
                        shell.tab_error = Some(format!("Cannot start {what}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// True (after telling the user and opening the workspace menu) when the workspace has no
    /// default profile of `kind`; the daemon would refuse to start one.
    pub(crate) fn ask_for_default(
        &mut self,
        project_id: Ulid,
        kind: SessionKind,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        self.tab_error = None;
        let project = (self.sidebar.projects().iter()).find(|project| project.id == project_id);
        let (default, what) = match kind {
            SessionKind::Agent => (project.map(|p| &p.default_agent), "agent"),
            SessionKind::Shell => (project.map(|p| &p.default_shell), "shell"),
        };
        let missing = default.is_some_and(Option::is_none);
        if missing {
            self.tab_error = Some(format!(
                "Pick a default {what} for this workspace, then try again."
            ));
            self.open_workspace_menu(project_id, window, cx);
        }
        missing
    }

    /// A shortcut (SPEC §13). Terminal commands (paste) are handled by the focused pane.
    pub(crate) fn run(
        &mut self,
        Run(command): &Run,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // Voice input types into the field that has the keyboard, so it acts before the
        // focused-field checks below hand the key to that field, and before anything moves
        // the focus (§20G).
        if *command == Command::VoiceInput {
            return self.toggle_voice(window, cx);
        }
        // A shortcut ends every open popover (they never own the keyboard).
        self.close_menus();
        // Shortcuts still fire while a title is edited: save it first (closing its tab, …).
        self.finish_rename(true, window, cx);
        // Its own shortcut closes the palette again.
        if *command == Command::CommandPalette && self.palette.is_some() {
            return self.close_palette(window, cx);
        }
        // The dialogs are modal: bound keys go on to their key handler (e.g. Ctrl+Alt+digit
        // is AltGr on many layouts) instead of acting behind them.
        if self.modal_open() {
            cx.propagate();
            return;
        }
        // Same for the follow-up field while it has the keyboard, unless its card stopped
        // showing it (session exited, card moved on): then the shortcut acts as usual.
        if self.follow_up_focus.is_focused(window) {
            if self.follow_up_shown() {
                cx.propagate();
                return;
            }
            self.follow_up = None;
            window.focus(&self.focus_handle, cx);
        }
        // Same for a Docs comment draft (Ctrl+Alt+M is AltGr+M on many layouts).
        if self.comment_focus.is_focused(window) {
            if self.comment_draft().is_some() {
                cx.propagate();
                return;
            }
            window.focus(&self.focus_handle, cx);
        }
        // Same for the Git commit box (Esc leaves it, then shortcuts act again), unless it went
        // away (everything committed, mode switched): then the list takes the keyboard back.
        if self.git_commit_focus.is_focused(window) {
            if self.git_commit_shown() {
                cx.propagate();
                return;
            }
            window.focus(&self.git_focus, cx);
        }
        // Same for the Files find / replace fields while they have the keyboard (Esc gives it
        // back), except the Files commands (Ctrl+S, Ctrl+F, Ctrl+H act from there).
        if self.file_find_focus.is_focused(window) || self.file_replace_focus.is_focused(window) {
            let open = self.view() == View::Files
                && (self.workspace)
                    .and_then(|id| self.files.get(&id))
                    .is_some_and(|view| view.find.is_some() && !view.previewing());
            let files_command = matches!(
                command,
                Command::SaveFile | Command::FindInFile | Command::ReplaceInFile
            );
            if open && !files_command {
                cx.propagate();
                return;
            }
            if !open {
                window.focus(&self.focus_handle, cx);
            }
        }
        // Same for the Docs filter while it has the keyboard.
        if self.docs_filter_focus.is_focused(window) {
            if self.view() == View::Docs {
                cx.propagate();
                return;
            }
            window.focus(&self.focus_handle, cx);
        }
        // Same for the sidebar filter while it has the keyboard, except the shortcuts that
        // leave it (Peek, Toggle sidebar, Toggle compact sidebar).
        if self.filter_focus.is_focused(window) {
            if matches!(
                command,
                Command::Peek | Command::ToggleSidebar | Command::ToggleCompactSidebar
            ) {
                self.focus_pane(window, cx);
            } else {
                cx.propagate();
                return;
            }
        }
        // Same for the focused pane's composer (§20B.1), unless it went away. Its own shortcut
        // keeps it focused (Ctrl+Shift+Enter would read as Ctrl+Enter: send).
        if self.composer_focus.is_focused(window) {
            if self.composer_session().is_some() {
                if *command != Command::OpenComposer {
                    cx.propagate();
                }
                return;
            }
            self.focus_pane(window, cx);
        }
        // A shortcut that acts ends the peek too (it restores the keyboard first, so a shortcut
        // typed in the peek still runs); its own shortcut toggles it below.
        if *command != Command::Peek {
            self.close_peek(window, cx);
        }
        if self.view() != View::Terminals {
            match *command {
                // Opening or picking a tab or pane shows the terminals.
                Command::NewShellTab
                | Command::NewAgentTab
                | Command::NextTab
                | Command::PreviousTab
                | Command::Tab(_)
                | Command::SplitRight
                | Command::SplitDown
                | Command::SplitAgent
                | Command::SplitShell
                | Command::Layout(_) => self.show_view(View::Terminals, window, cx),
                // The rest act on panes that are not on screen.
                Command::RenameTab
                | Command::RenamePane
                | Command::CloseTab
                | Command::Focus(_)
                | Command::Resize(_)
                | Command::EqualizePanes
                | Command::ZoomPane
                | Command::ClosePane => return,
                _ => {}
            }
        }
        match *command {
            Command::AddWorkspace => self.add_workspace(window, cx),
            Command::Workspace(n) => {
                let nth = self.sidebar.projects().get(usize::from(n) - 1);
                if let Some(project_id) = nth.map(|project| project.id) {
                    self.activate_workspace(project_id, window, cx);
                }
            }
            Command::NextWorkspace => self.cycle_workspace(1, window, cx),
            Command::PreviousWorkspace => self.cycle_workspace(-1, window, cx),
            Command::NewShellTab => self.new_tab(SessionKind::Shell, window, cx),
            Command::NewAgentTab => self.new_tab(SessionKind::Agent, window, cx),
            Command::NextTab => self.switch_tab(|tabs| tabs.activate_offset(1), window, cx),
            Command::PreviousTab => self.switch_tab(|tabs| tabs.activate_offset(-1), window, cx),
            Command::Tab(n) => self.switch_tab(
                |tabs| {
                    tabs.activate_nth(usize::from(n) - 1);
                },
                window,
                cx,
            ),
            Command::RenameTab => self.start_rename(window, cx),
            Command::RenamePane => {
                if let Some(session_id) = self.focused_session() {
                    self.start_pane_rename(session_id, window, cx);
                }
            }
            Command::CloseTab => {
                let active = self
                    .workspace
                    .and_then(|project_id| self.tabs.get(&project_id))
                    .and_then(|tabs| tabs.active())
                    .map(|tab| tab.id);
                if let Some(tab_id) = active {
                    self.close_tab(tab_id, window, cx);
                }
            }
            Command::SplitRight => self.split(Axis::Vertical, window, cx),
            Command::SplitDown => self.split(Axis::Horizontal, window, cx),
            // Exactly the + Agent / + Shell buttons.
            Command::SplitAgent => self.new_pane(SessionKind::Agent, Axis::Vertical, window, cx),
            Command::SplitShell => self.new_pane(SessionKind::Shell, Axis::Vertical, window, cx),
            Command::Layout(preset) => self.apply_layout(preset, window, cx),
            Command::Focus(direction) => {
                if self
                    .active_tabs_mut()
                    .is_some_and(|tabs| tabs.focus_neighbour(direction))
                {
                    // A focus move ends a zoom: the hidden panes come back (and re-attach).
                    self.sync_panes(window, cx);
                    cx.notify();
                }
            }
            Command::Resize(direction) => {
                let Some(project_id) = self.workspace else {
                    return;
                };
                let write = self
                    .active_tabs_mut()
                    .and_then(|tabs| tabs.resize_focused(direction));
                if let Some(write) = write {
                    self.save_layout(project_id, write, window, cx);
                    cx.notify();
                }
            }
            Command::EqualizePanes => {
                let Some(project_id) = self.workspace else {
                    return;
                };
                let write = self.active_tabs_mut().and_then(Tabs::equalize_active);
                if let Some(write) = write {
                    self.save_layout(project_id, write, window, cx);
                    cx.notify();
                }
            }
            Command::ZoomPane => {
                if self.active_tabs_mut().is_some_and(Tabs::toggle_zoom) {
                    self.sync_panes(window, cx);
                    cx.notify();
                }
            }
            Command::ClosePane => self.close_focused_pane(window, cx),
            Command::OpenInZed => match self.workspace {
                // Docs open the shown file (SPEC §13 "workspace or current file").
                Some(project_id)
                    if self.view() == View::Docs
                        && self
                            .docs
                            .get(&project_id)
                            .is_some_and(|docs| docs.selected.is_some()) =>
                {
                    self.open_doc_in_zed(project_id, cx);
                }
                // Files open the opened file, at the selected line.
                Some(project_id)
                    if self.view() == View::Files
                        && self
                            .files
                            .get(&project_id)
                            .is_some_and(|view| view.file.is_some()) =>
                {
                    self.open_file_in_zed(project_id, cx);
                }
                // Git opens the selected changed file.
                Some(project_id)
                    if self.view() == View::Git
                        && self
                            .git
                            .get(&project_id)
                            .is_some_and(|view| view.selected.is_some()) =>
                {
                    self.open_git_file_in_zed(project_id, cx);
                }
                Some(project_id) => self.open_in_zed(project_id, cx),
                None => {}
            },
            Command::NextInbox => self.jump_next_inbox(window, cx),
            // Again while it has the keyboard closes it.
            Command::Peek => {
                if self.peek.as_ref().is_some_and(|peek| peek.focused) {
                    self.close_peek(window, cx);
                } else if let Some(first) = self.inbox_entries().first() {
                    self.open_peek(first.session_id, true, window, cx);
                }
            }
            Command::OpenComposer => self.open_composer(window, cx),
            Command::SendSelection => self.open_selection_dialog(window, cx),
            Command::ShowBoard => self.show_view(View::Board, window, cx),
            Command::ShowTerminals => self.show_view(View::Terminals, window, cx),
            Command::ShowDocs => self.show_view(View::Docs, window, cx),
            Command::ShowFiles => self.show_view(View::Files, window, cx),
            Command::ShowGit => self.show_view(View::Git, window, cx),
            Command::QuickOpen => self.open_quick_open(window, cx),
            Command::CommandPalette => self.open_palette(window, cx),
            Command::OpenKeymapSettings => self.open_keymap_settings(cx),
            Command::ToggleTheme => self.toggle_theme(window, cx),
            Command::ToggleFullScreen => window.toggle_fullscreen(),
            Command::ToggleSidebar => self.toggle_sidebar(window, cx),
            Command::ToggleCompactSidebar => self.toggle_compact_sidebar(window, cx),
            Command::ToggleInspector => self.toggle_inspector(window, cx),
            // Handled at the top of `run`.
            Command::VoiceInput => {}
            Command::AskAi => self.open_ask(window, cx),
            // Only bound while the Git list has focus.
            Command::GitToggleMode => self.git_key(git_view::Key::ToggleMode, window, cx),
            Command::GitToggleHistory => self.git_key(git_view::Key::ToggleHistory, window, cx),
            Command::GitNextHunk => self.git_key(git_view::Key::NextHunk, window, cx),
            Command::GitPreviousHunk => self.git_key(git_view::Key::PreviousHunk, window, cx),
            Command::GitStage => {
                self.git_key(git_view::Key::Stage(git_view::Stage::Stage), window, cx);
            }
            Command::GitUnstage => {
                self.git_key(git_view::Key::Stage(git_view::Stage::Unstage), window, cx);
            }
            Command::GitFocusCommit => {
                if self.git_commit_shown() {
                    window.focus(&self.git_commit_focus, cx);
                    cx.notify();
                }
            }
            Command::GitToggleDiffMode => self.set_diff_mode(self.diff_mode.other(), cx),
            Command::SaveFile => {
                // Only bound while Files are shown.
                if let Some(project_id) = self.workspace {
                    self.save_file(project_id, None, window, cx);
                }
            }
            Command::ToggleIgnored => {
                // Only bound while Files are shown.
                if let Some(project_id) = self.workspace {
                    let requests = self.files_view(project_id).toggle_ignored();
                    self.files_requests(project_id, requests, window, cx);
                }
            }
            Command::ToggleDocSource => {
                // Only bound while Docs are shown.
                if let Some(docs) = self.workspace.and_then(|id| self.docs.get_mut(&id)) {
                    docs.toggle_mode();
                    cx.notify();
                }
            }
            Command::AddComment => {
                // Only bound while Docs are shown or the Git list has focus.
                if let (Some(project_id), Some(context)) = (self.workspace, self.comment_context())
                {
                    self.start_comment(project_id, context, window, cx);
                }
            }
            Command::SendComments => {
                // Only bound while Docs are shown or the Git list has focus.
                if let (Some(project_id), Some(context)) = (self.workspace, self.comment_context())
                {
                    self.open_send_dialog(project_id, context, window, cx);
                }
            }
            Command::NewTask => {
                if let Some(project_id) = self.workspace {
                    // The new card shows up on the board.
                    self.show_view(View::Board, window, cx);
                    self.open_task_dialog(TaskDialog::new(project_id), window, cx);
                }
            }
            Command::RunTask if self.jira_side_shown() => {}
            Command::RunTask => {
                // Only bound while a board is shown; the selection may be another workspace's.
                let target = self
                    .workspace
                    .and_then(|project_id| self.board.run_target(self.selected_card, project_id));
                if let Some(task_id) = target {
                    self.run_task(task_id, window, cx);
                }
            }
            // Handled by the focused terminal pane.
            Command::Copy | Command::Paste | Command::PageUp | Command::PageDown => {}
            // A focused terminal pane handles it itself.
            Command::Find => match self.view() {
                View::Files => self.open_file_find(false, window, cx),
                View::Docs => {
                    window.focus(&self.docs_filter_focus, cx);
                    cx.notify();
                }
                _ => {}
            },
            Command::FindInFile => self.open_file_find(false, window, cx),
            Command::ReplaceInFile => self.open_file_find(true, window, cx),
        }
    }
}
