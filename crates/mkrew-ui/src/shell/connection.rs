//! Startup and the daemon link: `Shell::new`, refresh, loads, incoming-frame dispatch.

use crate::*;

impl Shell {
    pub(crate) fn new(
        launched: Instant,
        paths: Paths,
        keymap: Keymap,
        keymap_errors: &[KeymapError],
        jump: Option<(Ulid, Ulid)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let keymap_file = paths.keymap_file();
        let settings_file = paths.settings_file();
        // A broken settings file keeps the defaults, like the other reads here.
        let settings = mkrew_core::settings::load(&settings_file).unwrap_or_default();
        let flash_taskbar = settings.notify.flash_taskbar;
        let features = settings.features;
        let jira_on = !settings.jira.site.trim().is_empty();
        let jira_mine = settings.jira.mine;
        let jira_layout = settings.jira.layout;
        let ui = settings.ui;
        let diff_mode = ui.diff_mode;
        let diff_wrap = ui.diff_wrap;
        let close_to_tray = ui.close_to_tray;
        let reading_default = ui.reading_width;
        let inspector_width = inspector::saved_width(ui.inspector_width);
        let sidebar_width = sidebar::saved_width(ui.sidebar_width);
        let sidebar_mode = ui.sidebar;
        let prompts_dir = paths.prompts_dir();
        let config_dir = paths.config_dir.clone();
        // System mode follows the OS live (ADR 0027).
        cx.observe_window_appearance(window, |_, window, cx| {
            theme::refresh(cx.theme().mode, window, cx);
            cx.notify();
        })
        .detach();
        // Minimized → detach every pane; restored → re-attach from daemon snapshots (§10).
        cx.observe_window_visibility(window, |shell, _, window, cx| {
            shell.sync_panes(window, cx);
            cx.notify();
        })
        .detach();
        // Coming back to the window views the focused pane (a done session leaves the Inbox),
        // even one marked unread while it kept the keyboard, and ends the taskbar flash.
        cx.observe_window_activation(window, |shell, window, cx| {
            if window.is_window_active() {
                shell.viewed.activated();
                shell.desktop.flash(false);
            }
            cx.notify();
        })
        .detach();
        // The tray icon (Windows): its clicks arrive here from the tray thread.
        let (desktop, tray_events) = Desktop::new(window, &paths.socket);
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = tray_events.recv().await {
                let handled =
                    this.update_in(cx, |shell, window, cx| shell.tray_event(event, window, cx));
                if handled.is_err() {
                    return;
                }
            }
        })
        .detach();
        // Unsaved Files changes (§2.5): the close button asks first.
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |shell, cx| shell.window_should_close(window, cx))
                .unwrap_or(true)
        });
        // Spinner and elapsed/idle times move on their own (§11).
        cx.spawn_in(window, async move |this, cx| {
            let mut last = status::now_ms();
            loop {
                cx.background_executor().timer(theme::STATUS_TICK).await;
                let now = status::now_ms();
                let ticked = this.update_in(cx, |shell, window, cx| {
                    // ponytail: mtime poll on the 500 ms tick; a file watcher if it ever shows
                    // in a profile.
                    let modified = std::fs::metadata(&shell.settings_file)
                        .and_then(|m| m.modified())
                        .ok();
                    if modified != shell.settings_modified {
                        shell.settings_modified = modified;
                        shell.apply_settings_file(window, cx);
                    }
                    // Toasts hide here, within one tick of their time (SPEC §14).
                    if shell.toasts.expire(Instant::now()) {
                        cx.notify();
                    }
                    shell.sync_thread(cx);
                    let sessions = shell.sidebar.sessions();
                    let board_due = shell.workspace.is_some_and(|project_id| {
                        shell.view() == View::Board
                            && shell.board.redraw_due(project_id, sessions, last, now)
                    });
                    // The recording meter has its own faster redraw (`toggle_voice`).
                    let transcribing = matches!(shell.voice, voice::State::Transcribing);
                    let jira_busy = shell.jira.values().any(JiraView::busy);
                    let due =
                        board_due || shell.starting_pane.is_some() || transcribing || jira_busy;
                    if window.is_visible() && (due || status::redraw_due(sessions, last, now)) {
                        cx.notify();
                    }
                });
                last = now;
                if ticked.is_err() {
                    return;
                }
            }
        })
        .detach();
        // The title bar's phone icon follows `daemon.status.web`: read every `PHONE_POLL_IDLE`,
        // and on every tick while the Phone access dialog is open (ADR 0053).
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(theme::PHONE_TICK).await;
                let ticked = this.update_in(cx, |shell, window, cx| {
                    if shell.phone.due(Instant::now(), shell.phone_open) {
                        shell.poll_phone(window, cx);
                    }
                });
                if ticked.is_err() {
                    return;
                }
            }
        })
        .detach();
        // In-app update (SPEC §20K): ask GitHub now and every `CHECK_EVERY`; clear what a
        // previous macOS update left beside the app.
        cx.background_executor()
            .spawn(async {
                mkrew_ui::update::clean_leftovers(&mkrew_ui::update::current_install());
            })
            .detach();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let checked = this.update_in(cx, |shell, window, cx| {
                    shell.check_updates(false, window, cx)
                });
                if checked.is_err() {
                    return;
                }
                cx.background_executor()
                    .timer(mkrew_ui::update::CHECK_EVERY)
                    .await;
            }
        })
        .detach();
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let rename_focus = cx.focus_handle();
        let rename_blur = cx.on_blur(&rename_focus, window, |shell, window, cx| {
            shell.finish_rename(true, window, cx);
        });
        let follow_up_focus = cx.focus_handle();
        let follow_up_blur = cx.on_blur(&follow_up_focus, window, |shell, _, cx| {
            // A typed draft (or one being sent) stays on its card without its caret; an empty
            // one goes.
            shell
                .follow_up
                .take_if(|draft| draft.text.text().is_empty() && !draft.busy);
            cx.notify();
        });
        let (painted, painted_rx) = async_channel::bounded(1);
        // What the user saw before this start (A2): a corrupt or missing file is empty.
        let viewed = Viewed::load(&paths.data_dir);
        let mut shell = Self {
            status: Status::Connecting,
            connection: None,
            update: None,
            updating: false,
            sidebar: Sidebar::default(),
            refreshing: false,
            refresh_again: false,
            workspace: None,
            tabs: HashMap::new(),
            loading_tabs: HashSet::new(),
            early_tab_events: Vec::new(),
            reveal: None,
            tab_error: None,
            starting_pane: None,
            router: Router::default(),
            panes: HashMap::new(),
            attach_errors: HashMap::new(),
            painted,
            painted_rx,
            paths,
            connect_gen: 0,
            reconnect_attempts: 0,
            reconnect_in: None,
            focus_handle,
            renaming: None,
            rename_focus,
            _rename_blur: rename_blur,
            keymap,
            keymap_error: keymap::notice(keymap_errors),
            keymap_errors: keymap_errors.to_vec(),
            keymap_file,
            settings_modified: None,
            settings_error: None,
            settings_file,
            prompts_dir,
            config_dir,
            phone: Phone::default(),
            phone_open: false,
            settings_dialog: None,
            settings_scroll: ScrollHandle::new(),
            workspace_menu: None,
            file_menu: None,
            file_tab_menu: None,
            tab_menu: None,
            menu_focus: cx.focus_handle(),
            menu_return: None,
            menu_selected: None,
            menu_swatch: 0,
            menu_pending: None,
            reorder: None,
            layout_menu: None,
            pane_menu: None,
            share_menu: None,
            run_setup: None,
            run_setup_scroll: ScrollHandle::new(),
            run_setup_drag: scrollbar::Drag::default(),
            run_menu: None,
            workspace_switcher: false,
            new_menu: false,
            empty_workspaces_open: false,
            collapsed_groups: HashSet::new(),
            reading_default,
            reading_width: HashMap::new(),
            run_reveal: None,
            ports: Ports::default(),
            agent_icons: HashMap::new(),
            agent_profiles: Vec::new(),
            broadcast: None,
            ask: None,
            ask_web: false,
            viewed,
            desktop,
            attention: Tracker::default(),
            close_to_tray,
            flash_taskbar,
            turns: Turns::default(),
            usages: Usages::default(),
            plans: Plans::default(),
            plan_error: None,
            sidebar_layout: sidebar::Layout::from_mode(sidebar_mode),
            inspector_open: false,
            thread: None,
            inspector_width,
            inspector_dragging: false,
            sidebar_width,
            sidebar_dragging: false,
            pane_dragging: false,
            board: Board::default(),
            board_error: None,
            conflicts: Conflicts::default(),
            card_error: None,
            jira: HashMap::new(),
            jira_on,
            jira_mine,
            jira_layout,
            jira_keys_focus: cx.focus_handle(),
            jira_filter_focus: cx.focus_handle(),
            selected_card: None,
            starting: HashSet::new(),
            run_with: None,
            fan_out: None,
            follow_up: None,
            follow_up_focus,
            _follow_up_blur: follow_up_blur,
            follow_up_scroll: ScrollHandle::new(),
            composers: Composers::default(),
            drop_hover: None,
            composer_focus: cx.focus_handle(),
            ime_marked: Rc::default(),
            typing_target: Rc::default(),
            voice: voice::State::default(),
            composer_scroll: ScrollHandle::new(),
            views: HashMap::new(),
            docs: HashMap::new(),
            comment_focus: cx.focus_handle(),
            files: HashMap::new(),
            files_focus: cx.focus_handle(),
            files_scroll: UniformListScrollHandle::new(),
            file_tabs_scroll: ScrollHandle::new(),
            file_find_focus: cx.focus_handle(),
            file_replace_focus: cx.focus_handle(),
            docs_filter_focus: cx.focus_handle(),
            doc_page: preview::Page::default(),
            file_page: preview::Page::default(),
            file_code_scroll: UniformListScrollHandle::new(),
            file_code_drag: scrollbar::Drag::default(),
            files_after_save: Vec::new(),
            file_editor_focus: cx.focus_handle(),
            file_block_focus: cx.focus_handle(),
            file_text_left: Rc::default(),
            git: HashMap::new(),
            git_repos: HashMap::new(),
            git_summary: None,
            git_summary_busy: false,
            git_summary_again: false,
            git_focus: cx.focus_handle(),
            git_commit_focus: cx.focus_handle(),
            git_filter_focus: cx.focus_handle(),
            git_menu: None,
            git_repo_query: Input::default(),
            git_repo_query_focus: cx.focus_handle(),
            git_files_scroll: UniformListScrollHandle::new(),
            git_diff_list: ListState::new(0, ListAlignment::Top, px(theme::GIT_DIFF_OVERDRAW)),
            git_diff_layout: Cell::new(None),
            git_diff_looks: Cell::new(None),
            git_diff_scroll_to: Cell::new(None),
            diff_mode,
            diff_wrap,
            features,
            git_discard: None,
            git_base: None,
            add_workspace: None,
            task_dialog: None,
            undo_dialog: None,
            worktree_dialog: None,
            compare: None,
            turns_dialog: None,
            turns_return: None,
            follow: Follow::default(),
            history: None,
            history_pane: None,
            doc_dialog: None,
            quick_open: None,
            quick_open_scroll: ScrollHandle::new(),
            palette: None,
            palette_scroll: ScrollHandle::new(),
            palette_return: None,
            theme_pick: None,
            prompt_picker: None,
            prompt_picker_scroll: ScrollHandle::new(),
            peek: None,
            peek_hover: peek::Hover::default(),
            peek_focus: cx.focus_handle(),
            filter_focus: cx.focus_handle(),
            peek_return: None,
            dialog_focus: cx.focus_handle(),
            prompt_scroll: ScrollHandle::new(),
            dialog_serial: 0,
            launched: Some(launched),
            first_frame_logged: false,
            sidebar_view: None,
            toasts: Toasts::default(),
            pending_jump: jump,
        };
        shell.connect(window, cx);
        shell
    }

    /// The user's Connect / Retry / Reconnect (and startup): connects now and restarts the
    /// automatic retry backoff.
    pub(crate) fn connect(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.reconnect_attempts = 0;
        self.start_connect(window, cx);
    }

    /// Waits [`mkrew_ui::connect::backoff`] after a failed or lost connect, then connects
    /// again. One timer per failure (no per-frame work); a newer attempt (`connect_gen`, e.g.
    /// the user's Reconnect) makes a pending timer a no-op, so attempts never overlap.
    fn schedule_reconnect(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let delay = mkrew_ui::connect::backoff(self.reconnect_attempts);
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
        self.reconnect_in = Some(delay);
        let generation = self.connect_gen;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if shell.connect_gen == generation
                    && matches!(shell.status, Status::Failed(_) | Status::Disconnected)
                {
                    shell.start_connect(window, cx);
                }
            });
        })
        .detach();
    }

    /// Connects to the daemon (starting it if needed) off the UI thread, then feeds its frames
    /// until the connection closes, then schedules an automatic retry.
    fn start_connect(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.connect_gen += 1;
        self.reconnect_in = None;
        self.status = Status::Connecting;
        cx.notify();
        let paths = self.paths.clone();
        let painted_rx = self.painted_rx.clone();
        cx.spawn_in(window, async move |this, cx| {
            // Blocking (polls a starting daemon for up to 10 s): off the UI thread.
            let connected = cx
                .background_executor()
                .spawn(async move { mkrew_ui::connect::connect(&paths) })
                .await;
            let connection = match connected {
                Ok(connection) => Rc::new(connection),
                Err(error) => {
                    let _ = this.update_in(cx, |shell, window, cx| {
                        shell.status = Status::Failed(format!("{error:#}"));
                        shell.schedule_reconnect(window, cx);
                        cx.notify();
                    });
                    return;
                }
            };
            let incoming = connection.incoming();
            tracing::info!(
                daemon_version = %connection.hello().daemon_version,
                "connected to the daemon"
            );
            let loaded = this.update_in(cx, |shell, window, cx| {
                shell.reconnect_attempts = 0;
                shell.connection = Some(connection);
                shell.refresh(window, cx);
                shell.poll_phone(window, cx);
            });
            if loaded.is_err() {
                return;
            }
            // Drain at once when frames arrive (low key latency); after feeding output, wait
            // until that frame is painted and `OUTPUT_FRAME` has passed, so a flood is fed once
            // per frame and at most ~60 times a second whatever the display rate.
            while let Ok(first) = incoming.recv().await {
                // A paint from an earlier frame must not end the wait below.
                let _ = painted_rx.try_recv();
                let mut batch = vec![first];
                while batch.len() < DRAIN_BATCH
                    && let Ok(next) = incoming.try_recv()
                {
                    batch.push(next);
                }
                let started = Instant::now();
                match this.update_in(cx, |shell, window, cx| shell.dispatch(batch, window, cx)) {
                    Ok(true) => {
                        let painted = async {
                            let _ = painted_rx.recv().await;
                        };
                        let timeout = cx.background_executor().timer(PAINT_TIMEOUT);
                        futures_lite::future::or(painted, timeout).await;
                        if let Some(rest) = OUTPUT_FRAME.checked_sub(started.elapsed()) {
                            cx.background_executor().timer(rest).await;
                        }
                    }
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.status = Status::Disconnected;
                shell.connection = None;
                shell.workspace = None;
                shell.tabs.clear();
                shell.loading_tabs.clear();
                shell.early_tab_events.clear();
                shell.reveal = None;
                shell.tab_error = None;
                shell.router.clear();
                shell.panes.clear();
                shell.attach_errors.clear();
                shell.renaming = None;
                shell.close_menus();
                shell.run_setup = None;
                shell.run_reveal = None;
                shell.ports = Ports::default();
                shell.plans = Plans::default();
                shell.plan_error = None;
                shell.broadcast = None;
                shell.ask = None;
                // Kept with its draft: `save_task` / `delete_task` do nothing without a
                // connection, and an in-flight save is not waited for.
                if let Some(dialog) = shell.task_dialog.as_mut() {
                    dialog.busy = false;
                }
                shell.undo_dialog = None;
                shell.worktree_dialog = None;
                shell.compare = None;
                shell.turns_dialog = None;
                shell.turns_return = None;
                shell.git_discard = None;
                shell.git_base = None;
                // Kept with its typed text (it cannot send without a connection); a clone in
                // flight will never report back, so it stops waiting.
                if let Some(dialog) = shell.add_workspace.as_mut()
                    && dialog.cloning.take().is_some()
                {
                    dialog.error = Some(
                        "The connection closed while cloning; check the workspace list once it is back."
                            .into(),
                    );
                }
                shell.phone_open = false;
                shell.phone = Phone::default();
                shell.history = None;
                shell.history_pane = None;
                shell.doc_dialog = None;
                shell.quick_open = None;
                shell.restore_theme_pick(window, cx);
                shell.palette = None;
                shell.prompt_picker = None;
                shell.palette_return = None;
                shell.peek = None;
                shell.peek_return = None;
                shell.follow_up = None;
                shell.docs.clear();
                shell.files.clear();
                shell.git.clear();
                shell.git_summary = None;
                // An open confirmation keeps the keyboard; it hands focus back when answered.
                // A kept dialog (typed text survives) keeps it too.
                if !window.has_active_prompt()
                    && shell.task_dialog.is_none()
                    && shell.add_workspace.is_none()
                {
                    window.focus(&shell.focus_handle, cx);
                }
                shell.schedule_reconnect(window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-reads workspaces and sessions. The first time, shows the workspace of the most
    /// recently started live session.
    pub(crate) fn refresh(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        self.sidebar.begin_load();
        self.board.begin_load();
        let projects = connection.call::<Vec<Project>>(Request::ProjectList);
        let sessions =
            connection.call::<Vec<Session>>(Request::SessionList(SessionListParams::default()));
        let tasks = connection.call::<Vec<Task>>(Request::TaskList(TaskListParams::default()));
        let profiles = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let listed = match (projects.await, sessions.await) {
                (Ok(projects), Ok(sessions)) => Ok((projects, sessions)),
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            let tasks = tasks.await;
            let profiles = profiles.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.refreshing = false;
                match profiles {
                    Ok(listed) => {
                        shell.agent_icons = listed
                            .profiles
                            .iter()
                            .filter_map(|p| Some((p.id.clone(), p.icon.clone()?)))
                            .collect();
                        shell.agent_profiles = listed.profiles;
                    }
                    // Badges fall back to letters; the listing below is unaffected.
                    Err(error) => tracing::warn!("agent.profiles failed: {error:#}"),
                }
                match tasks {
                    Ok(tasks) => {
                        shell.board.load(tasks);
                        shell.drop_gone_targets(window, cx);
                        shell.board_error = None;
                    }
                    Err(error) => {
                        tracing::warn!("task.list failed: {error:#}");
                        shell.board.cancel_load();
                        shell.board_error = Some(format!("{error:#}"));
                    }
                }
                match listed {
                    Ok((projects, sessions)) => {
                        shell.sidebar.load(projects, sessions);
                        shell.sync_attention(cx);
                        shell.load_usage(window, cx);
                        shell.load_ports(window, cx);
                        shell.load_plans(window, cx);
                        shell.status = Status::Connected;
                        let current = shell
                            .workspace
                            .filter(|id| shell.sidebar.projects().iter().any(|p| p.id == *id));
                        match current.or_else(|| shell.initial_workspace()) {
                            Some(project_id) => {
                                // A new session (e.g. spawned by `mk`) fills an empty tab.
                                shell.fill_empty_tab(project_id, window, cx);
                                shell.activate_workspace(project_id, window, cx);
                            }
                            None => {
                                shell.workspace = None;
                                shell.sync_panes(window, cx);
                            }
                        }
                        // After the default workspace, so the pane a toast asked for wins.
                        if let Some((session, project)) = shell.pending_jump.take() {
                            shell.notification_clicked(session, project, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!("listing workspaces failed: {error:#}");
                        shell.sidebar.cancel_load();
                        shell.list_failed(format!("{error:#}"));
                    }
                }
                // Tasks or sessions may have moved on while the lists were read.
                shell
                    .conflicts
                    .prune(&shell.board, shell.sidebar.sessions());
                if shell.peek.is_some() {
                    shell.update_peek(false, window, cx);
                }
                cx.notify();
                if std::mem::take(&mut shell.refresh_again) {
                    shell.refresh(window, cx);
                }
            });
        })
        .detach();
    }

    /// Re-reads `settings.toml` and applies what the UI reads from it: fonts, theme, diff
    /// layout and wrap, tray, taskbar flash, Jira, reading width, features (§20.0). A file that
    /// does not parse keeps the last good settings; the palette lists why.
    pub(crate) fn apply_settings_file(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let installed = cx.text_system().all_font_names();
        let last = cx.global::<theme::Fonts>().clone();
        let mode = cx.theme().mode;
        let names = cx.global::<theme::ThemeNames>().clone();
        match theme::reload(&self.settings_file, &installed, &last, mode, &names) {
            Ok(changed) => {
                self.settings_error = None;
                if let Some((fonts, mode, names)) = changed {
                    cx.set_global(fonts);
                    cx.set_global(names);
                    theme::refresh(mode, window, cx);
                    cx.refresh_windows();
                }
                // `theme::reload` only returns fonts, theme mode and names; a hand edit of
                // `diff_mode` / `diff_wrap` / `[features]` applies too (the file
                // just parsed, so this second read only happens when it changed).
                if let Ok(settings) = mkrew_core::settings::load(&self.settings_file) {
                    self.close_to_tray = settings.ui.close_to_tray;
                    self.flash_taskbar = settings.notify.flash_taskbar;
                    if !self.flash_taskbar {
                        self.desktop.flash(false);
                    }
                    self.apply_diff_mode(settings.ui.diff_mode);
                    self.diff_wrap = settings.ui.diff_wrap;
                    self.jira_on = !settings.jira.site.trim().is_empty();
                    self.jira_layout = settings.jira.layout;
                    // A hand edit of `[jira] mine` searches the shown list again.
                    if self.apply_jira_mine(settings.jira.mine)
                        && self.jira_side_shown()
                        && let Some(project_id) = self.workspace
                    {
                        self.load_jira(project_id, window, cx);
                    }
                    self.reading_default = settings.ui.reading_width;
                    if self.features != settings.features {
                        // `view()` may now map Docs to Terminals.
                        self.features = settings.features;
                        self.sync_panes(window, cx);
                    }
                }
            }
            // The last good settings stay; the palette lists why (§20.0).
            Err(error) => self.settings_error = Some(error),
        }
        if let Some(dialog) = &mut self.settings_dialog {
            dialog.reload(
                mkrew_core::settings::load(&self.settings_file).map_err(|error| error.to_string()),
            );
        }
        cx.notify();
    }

    /// The badge before `agent`'s name (ADR 0025).
    pub(crate) fn agent_badge(&self, agent: &str) -> sidebar::Badge {
        sidebar::agent_badge(self.agent_icons.get(agent).map(String::as_str), agent)
    }

    /// Reads the token usage of each listed workspace not read yet (SPEC §20E.3); events keep
    /// it live afterwards.
    pub(crate) fn load_usage(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let projects = self.sidebar.projects().iter().map(|project| project.id);
        for project_id in self.usages.to_request(projects.collect::<Vec<_>>()) {
            let listed = connection
                .call::<Vec<SessionUsageEvent>>(Request::SessionUsage(ProjectRef { project_id }));
            cx.spawn_in(window, async move |this, cx| {
                let listed = listed.await;
                let _ = this.update(cx, |shell, cx| match listed {
                    Ok(listed) => {
                        if shell.usages.load(listed) {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        tracing::warn!("session.usage failed: {error:#}");
                        // Asked again on the next refresh.
                        shell.usages.request_failed(project_id);
                    }
                });
            })
            .detach();
        }
    }

    /// Reads the ports of each listed workspace not read yet (SPEC §10); events keep them live.
    pub(crate) fn load_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let projects = self.sidebar.projects().iter().map(|project| project.id);
        for project_id in self.ports.to_request(projects.collect::<Vec<_>>()) {
            let listed = connection
                .call::<Vec<SessionPortsEvent>>(Request::SessionPorts(ProjectRef { project_id }));
            cx.spawn_in(window, async move |this, cx| {
                let listed = listed.await;
                let _ = this.update(cx, |shell, cx| match listed {
                    Ok(listed) => {
                        if shell.ports.load(listed) {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        tracing::warn!("session.ports failed: {error:#}");
                        // Asked again on the next refresh.
                        shell.ports.request_failed(project_id);
                    }
                });
            })
            .detach();
        }
    }

    /// Reads the plans of each listed workspace not read yet (ADR 0039); events keep them live.
    pub(crate) fn load_plans(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let projects = self.sidebar.projects().iter().map(|project| project.id);
        for project_id in self.plans.to_request(projects.collect::<Vec<_>>()) {
            let listed = connection
                .call::<Vec<SessionPlanEvent>>(Request::SessionPlan(ProjectRef { project_id }));
            cx.spawn_in(window, async move |this, cx| {
                let listed = listed.await;
                let _ = this.update(cx, |shell, cx| match listed {
                    Ok(listed) => {
                        if shell.plans.load(listed) {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        tracing::warn!("session.plan failed: {error:#}");
                        // Asked again on the next refresh.
                        shell.plans.request_failed(project_id);
                    }
                });
            })
            .detach();
        }
    }

    /// A list failed; ignored once disconnected (that status says more).
    pub(crate) fn list_failed(&mut self, error: String) {
        if self.connection.is_some() {
            self.status = Status::ListFailed(error);
        }
    }

    /// Routes a batch of incoming frames; returns whether a pane got new content.
    pub(crate) fn dispatch(
        &mut self,
        batch: Vec<Incoming>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let mut fed: Vec<Entity<TerminalPane>> = Vec::new();
        let mut layout_changed = false;
        let mut peek_changed = false;
        for incoming in batch {
            if let Incoming::Event(event) = &incoming {
                if self.board.apply(event) {
                    self.drop_gone_targets(window, cx);
                    cx.notify();
                }
                if self.turns.apply(event) {
                    cx.notify();
                }
                if self.usages.apply(event) {
                    cx.notify();
                }
                if self.plans.apply(event) {
                    cx.notify();
                }
                if self.ports.apply(event) {
                    cx.notify();
                }
                if self.composers.apply(event) {
                    cx.notify();
                }
                if let Some(wait) = self.follow.apply(event) {
                    self.follow_wait(wait, window, cx);
                }
                if let Event::SessionState(state) = event {
                    peek_changed |= self
                        .peek
                        .as_ref()
                        .is_some_and(|peek| peek.session_id == state.session_id);
                }
                // Before the sidebar applies it: `viewed` and the flash read the previous state.
                if self.flash_taskbar
                    && !window.is_window_active()
                    && attention::entered_needs_input(event, self.sidebar.sessions())
                {
                    self.desktop.flash(true);
                }
                self.viewed.apply(event, self.sidebar.sessions());
                match self.sidebar.apply(event) {
                    Change::None => {}
                    Change::Redraw => {
                        cx.notify();
                        self.sync_attention(cx);
                    }
                    // A new session (e.g. spawned by `mk`): re-list.
                    Change::Refresh => self.refresh(window, cx),
                }
                // After the board and sidebar, so a task leaving Running clears its warning.
                let detected = self.conflicts.apply(event);
                if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected {
                    cx.notify();
                }
                for tabs in self.tabs.values_mut() {
                    layout_changed |= tabs.apply(event);
                }
                if let Event::TabUpdated(tab) = event
                    && self.loading_tabs.contains(&tab.project_id)
                {
                    self.early_tab_events.push(event.clone());
                }
                if let Event::TabUpdated(tab) = event
                    && self.run_reveal == Some(tab.id)
                {
                    self.reveal_run_tab(tab.project_id, window, cx);
                }
                if let Event::NotificationClicked(clicked) = event {
                    self.notification_clicked(clicked.session_id, clicked.project_id, window, cx);
                }
                if matches!(event, Event::DocChanged(_) | Event::CommentUpdated(_)) {
                    self.doc_event(event, window, cx);
                }
                if let Event::CommentUpdated(comment) = event {
                    for view in self.git.values_mut() {
                        view.comments.apply_event(view.project_id, comment);
                    }
                    cx.notify();
                }
                if matches!(event, Event::GitChanged(_)) {
                    self.files_event(event, window, cx);
                    self.git_event(event, window, cx);
                    if let Event::GitChanged(changed) = event
                        && changed.worktree_task_id.is_none()
                        && self.workspace == Some(changed.project_id)
                    {
                        self.load_git_summary(changed.project_id, window, cx);
                    }
                }
                if let Event::AiGenerated(done) = event {
                    let result = ai_result(done);
                    for view in self.git.values_mut() {
                        view.commit.generated(done.generation_id, result.clone());
                    }
                    if let Some(dialog) = self.ask.as_mut() {
                        dialog.answered(done.generation_id, result.clone());
                    }
                    self.composers.enhanced(done.generation_id, result);
                    cx.notify();
                }
                if let Event::ProjectCloned(done) = event {
                    self.workspace_cloned(done, window, cx);
                }
            }
            let Some(delivery) = self.router.route(incoming) else {
                continue;
            };
            let Some(pane) = self.panes.get(&delivery.session_id()).cloned() else {
                continue;
            };
            pane.update(cx, |pane, _| match delivery {
                Delivery::Output { data, .. } => pane.feed(&data),
                Delivery::Snapshot { snapshot, .. } => pane.load_snapshot(&snapshot),
                Delivery::Exited { exit_code, .. } => pane.set_exited(exit_code),
            });
            if !fed.iter().any(|p| p.entity_id() == pane.entity_id()) {
                fed.push(pane);
            }
        }
        if layout_changed {
            self.sync_panes(window, cx);
            cx.notify();
        }
        if self.peek.is_some() {
            self.update_peek(peek_changed, window, cx);
        }
        for pane in &fed {
            pane.update(cx, |_, cx| cx.notify());
        }
        !fed.is_empty()
    }

    /// One short sentence, plus optional detail shown smaller below it.
    pub(crate) fn status_line(&self) -> (SharedString, Option<SharedString>) {
        match &self.status {
            Status::Connecting if self.reconnect_attempts > 0 => {
                ("Reconnecting to the daemon…".into(), None)
            }
            Status::Connecting => ("Connecting to the daemon…".into(), None),
            Status::Connected if self.sidebar.is_empty() => (
                "No workspaces yet.".into(),
                Some("Add a project folder or clone a repository to get started.".into()),
            ),
            Status::Connected => ("Opening the workspace…".into(), None),
            Status::Disconnected => (
                "The daemon connection closed.".into(),
                self.retry_note().map(Into::into),
            ),
            Status::Failed(error) => (
                "Cannot reach the daemon.".into(),
                Some(
                    match self.retry_note() {
                        Some(note) => format!("{error}\n{note}"),
                        None => error.clone(),
                    }
                    .into(),
                ),
            ),
            Status::ListFailed(error) => (
                "Cannot load the workspace.".into(),
                Some(error.clone().into()),
            ),
        }
    }

    /// "Reconnecting in 5 s…" while an automatic retry is pending (set once per attempt, so
    /// the text is static and nothing repaints while waiting).
    fn retry_note(&self) -> Option<String> {
        let wait = self.reconnect_in?;
        Some(format!(
            "Reconnecting automatically in {} s…",
            wait.as_secs()
        ))
    }

    /// The one action under the status line: Add workspace when there is none, Reconnect /
    /// Retry after a failure; `None` while connecting or opening.
    pub(crate) fn status_action(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let retry = |id: &'static str, label: &'static str| {
            button(id, label, theme, true).on_click(cx.listener(|shell, _, window, cx| {
                match shell.status {
                    Status::ListFailed(_) => shell.refresh(window, cx),
                    _ => shell.connect(window, cx),
                }
            }))
        };
        let action = match &self.status {
            Status::Connected if self.sidebar.is_empty() => {
                button("status-add-workspace", "Add workspace", theme, true)
                    .child(kbd(self.keymap.label(Command::AddWorkspace), theme, true))
                    .on_click(cx.listener(|shell, _, window, cx| shell.add_workspace(window, cx)))
            }
            Status::Disconnected => retry("status-reconnect", "Reconnect"),
            Status::Failed(_) | Status::ListFailed(_) => retry("status-retry", "Retry"),
            Status::Connecting | Status::Connected => return None,
        };
        Some(action.into_any_element())
    }
}
