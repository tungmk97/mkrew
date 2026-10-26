//! Workspace actions: add, menus, open in Zed/Explorer, settings, theme, archive.

use crate::*;

impl Shell {
    /// Opens the "Add workspace" dialog (ADR 0076): a local folder, or a clone of a repository.
    pub(crate) fn add_workspace(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.connection.is_none() {
            return;
        }
        self.sidebar_error = None;
        self.add_workspace = Some(CloneDialog::new(self.clone_parent()));
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// Where a clone goes by default: next to the active workspace, else the home folder.
    fn clone_parent(&self) -> String {
        let active = self
            .workspace
            .and_then(|id| self.sidebar.projects().iter().find(|p| p.id == id));
        active
            .and_then(|p| Path::new(&p.path).parent().map(|d| d.display().to_string()))
            .or_else(|| std::env::home_dir().map(|d| d.display().to_string()))
            .unwrap_or_default()
    }

    pub(crate) fn close_add_workspace(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.add_workspace = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Typing edits the URL, Enter clones, Esc closes (a running clone goes on).
    pub(crate) fn add_workspace_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = base_dialog::key(&event.keystroke) else {
            return;
        };
        let paste = (key == base_dialog::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(dialog) = self.add_workspace.as_mut() else {
            return;
        };
        match key {
            base_dialog::Key::Close => return self.close_add_workspace(window, cx),
            base_dialog::Key::Save => return self.clone_workspace(window, cx),
            base_dialog::Key::Paste => {
                if let Some(text) = paste {
                    dialog.insert(&text);
                }
            }
            base_dialog::Key::Edit(_) | base_dialog::Key::Insert(_) => {
                if let base_dialog::Key::Edit(edit) = &key {
                    copy_selection(edit, &dialog.url, cx);
                }
                dialog.key(&key);
            }
        }
        cx.notify();
    }

    /// Picks a folder for the dialog: the workspace to add (`clone_into: false`), or where the
    /// clone goes.
    pub(crate) fn pick_workspace_folder(
        &mut self,
        clone_into: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                if clone_into {
                    "Clone into"
                } else {
                    "Add workspace"
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |shell, cx| {
                        if let Some(dialog) = shell.add_workspace.as_mut() {
                            dialog.error =
                                Some(format!("Cannot open the folder picker: {error:#}"));
                        }
                        cx.notify();
                    });
                    None
                }
            };
            let Some(path) = path else {
                return;
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let path = path.to_string_lossy().into_owned();
                if clone_into {
                    if let Some(dialog) = shell.add_workspace.as_mut() {
                        dialog.parent = path;
                    }
                    cx.notify();
                } else {
                    shell.add_folder_workspace(path, window, cx);
                }
            });
        })
        .detach();
    }

    /// Adds `path` as a workspace (`project.add`) and shows it.
    fn add_folder_workspace(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.close_add_workspace(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let added = connection
                .call::<Project>(Request::ProjectAdd(ProjectAddParams { path, name: None }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match added {
                    Ok(project) => {
                        let project_id = project.id;
                        shell.sidebar_error = None;
                        shell.sidebar.add_project(project);
                        shell.activate_workspace(project_id, window, cx);
                    }
                    Err(error) => {
                        shell.sidebar_error = Some(format!("Cannot add the workspace: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `project.clone` with the typed URL; the dialog shows "Cloning…" until `project.cloned`.
    pub(crate) fn clone_workspace(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.add_workspace.as_mut().and_then(CloneDialog::begin) else {
            cx.notify();
            return;
        };
        let clone_id = params.clone_id;
        let sent = connection.request(Request::ProjectClone(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let refused = match sent.await {
                Ok(Outcome::Result(_)) => return,
                Ok(Outcome::Error(body)) => body.message,
                Err(error) => format!("{error:#}"),
            };
            let _ = this.update(cx, |shell, cx| {
                match shell.add_workspace.as_mut() {
                    Some(dialog) if dialog.cloning == Some(clone_id) => dialog.failed(refused),
                    _ => shell.toast_error(format!("Cannot clone: {refused}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A `project.clone` ended: its dialog closes on the new workspace, or shows git's error.
    /// A clone whose dialog was closed is shown in the sidebar anyway (`project.updated`).
    pub(crate) fn workspace_cloned(
        &mut self,
        done: &ProjectClonedEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let mine = self
            .add_workspace
            .as_ref()
            .is_some_and(|dialog| dialog.is_mine(done));
        match (&done.project, &done.error) {
            (Some(project), _) if mine => {
                self.close_add_workspace(window, cx);
                self.sidebar.add_project(project.clone());
                self.activate_workspace(project.id, window, cx);
            }
            (Some(_), _) => {}
            (None, error) => {
                let error = error.clone().unwrap_or_else(|| "git clone failed".into());
                match self.add_workspace.as_mut().filter(|_| mine) {
                    Some(dialog) => dialog.failed(error),
                    None => self.toast_error(format!("Cannot clone: {error}"), cx),
                }
            }
        }
        cx.notify();
    }

    /// Picks a Markdown file and links it as `session_id`'s plan (`session.link_plan`).
    pub(crate) fn pick_plan(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.plan_error = None;
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Link plan".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.plan_error = Some((
                            session_id,
                            format!("Cannot open the file picker: {error:#}"),
                        ));
                        cx.notify();
                    });
                    None
                }
            };
            let Some(path) = path else {
                return;
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let path = path.to_string_lossy().into_owned();
                shell.link_plan(session_id, Some(path), window, cx);
            });
        })
        .detach();
    }

    /// Links `path` as the plan of `session_id`, or unlinks with `None`. The daemon answers with
    /// `session.updated` and `session.plan`; only a refusal is kept here.
    pub(crate) fn link_plan(
        &mut self,
        session_id: Ulid,
        path: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.plan_error = None;
        cx.notify();
        let linked = connection.call::<Session>(Request::SessionLinkPlan(SessionLinkPlanParams {
            session_id,
            path,
        }));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = linked.await {
                let _ = this.update(cx, |shell, cx| {
                    shell.plan_error = Some((session_id, format!("{error:#}")));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Opens the menu of `project_id` (closing any other) and lists the agent profiles for it.
    pub(crate) fn open_workspace_menu(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.sidebar_layout.show();
        self.file_menu = None;
        self.workspace_menu = Some(WorkspaceMenu::new(project_id));
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // Unless the menu closed (or moved to another workspace) meanwhile.
                if let Some(menu) = shell
                    .workspace_menu
                    .as_mut()
                    .filter(|menu| menu.project_id == project_id)
                {
                    menu.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Opens the folder of `project_id` in Zed; a failure (e.g. `zed` missing) shows in the
    /// sidebar.
    pub(crate) fn open_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        self.workspace_menu = None;
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        match zed::open(Path::new(&project.path), None) {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(%project_id, "open in Zed failed: {error}");
                self.sidebar_error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    /// "Open in File Explorer" on a workspace: opens its folder in the OS file manager.
    pub(crate) fn open_workspace_in_explorer(
        &mut self,
        project_id: Ulid,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        if let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) {
            cx.open_with_system(Path::new(&project.path));
        }
        cx.notify();
    }

    /// Right click on a Files tree row (`path`) or on the tree itself (`""`).
    pub(crate) fn open_file_menu(
        &mut self,
        project_id: Ulid,
        path: String,
        is_dir: bool,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<'_, Self>,
    ) {
        let target = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        self.close_menus();
        self.file_menu = Some(FileMenu {
            project_id,
            target,
            path,
            is_dir,
            position,
        });
        cx.notify();
    }

    /// "Open in File Explorer" on a Files path: a file is shown selected in its folder, a
    /// folder is opened.
    pub(crate) fn open_path_in_explorer(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
        is_dir: bool,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        if let Some(full) = self.target_file(project_id, target, path) {
            if is_dir {
                cx.open_with_system(&full);
            } else {
                cx.reveal_path(&full);
            }
        }
        cx.notify();
    }

    /// "Open in Zed" on a Files path (a file or a folder); a failure shows in the sidebar,
    /// like the workspace menu's.
    pub(crate) fn open_path_in_zed(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        if let Some(full) = self.target_file(project_id, target, path) {
            match zed::open(&full, None) {
                Ok(()) => self.sidebar_error = None,
                Err(error) => {
                    tracing::warn!(path = %full.display(), "open in Zed failed: {error}");
                    self.sidebar_error = Some(error.to_string());
                }
            }
        }
        cx.notify();
    }

    /// "New file…" / "New folder…" on a Files path: the name field opens over the tree.
    pub(crate) fn start_new_entry(
        &mut self,
        entry: NewEntry,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        self.renaming = Some(Renaming {
            target: RenameTarget::NewEntry(entry),
            draft: Input::default(),
        });
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    /// The name field was committed: creates the entry, then shows it (a new file opens in
    /// the editor). A blank name does nothing; a failure shows in the sidebar.
    pub(crate) fn create_entry(
        &mut self,
        entry: NewEntry,
        name: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let NewEntry {
            project_id,
            target,
            dir,
            folder,
        } = entry;
        if name.trim().is_empty() {
            return;
        }
        let Some(root) = self.target_file(project_id, target, "") else {
            return;
        };
        let path = match file_ops::create(&root, &dir, name, folder) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(%project_id, "new file / folder failed: {error}");
                self.sidebar_error = Some(error);
                return;
            }
        };
        self.sidebar_error = None;
        if self.files_at(project_id, target).is_none() {
            return;
        }
        self.refresh_files(project_id, window, cx);
        if folder {
            if let Some(view) = self.files_at(project_id, target) {
                let requests = view.reveal_dir(&path);
                self.files_requests(project_id, requests, window, cx);
            }
        } else {
            self.reveal_file(project_id, target, path, None, Open::Kept, window, cx);
        }
    }

    /// "Copy path" on a Files path: its absolute path, native separators.
    pub(crate) fn copy_file_path(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        if let Some(full) = self.target_file(project_id, target, path) {
            cx.write_to_clipboard(ClipboardItem::new_string(full.display().to_string()));
            self.toast_info("Path copied", cx);
        }
        cx.notify();
    }

    /// "Add to .gitignore" on a Files path: appended to the `.gitignore` of the folder the
    /// view shows; the tree lists again (the entry hides unless ignored files are shown). A
    /// failure shows in the sidebar.
    pub(crate) fn ignore_file_path(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        let Some(root) = self.target_file(project_id, target, "") else {
            return;
        };
        match file_ops::ignore(&root, path, is_dir) {
            Ok(()) => {
                self.sidebar_error = None;
                if self.files_at(project_id, target).is_some() {
                    self.refresh_files(project_id, window, cx);
                }
            }
            Err(error) => {
                tracing::warn!(%project_id, path, "add to .gitignore failed: {error}");
                self.sidebar_error = Some(error);
            }
        }
        cx.notify();
    }

    /// Settings dialog "Edit keymap.toml in Zed": creates the commented template if missing and
    /// opens it in Zed; a failure shows in the sidebar.
    pub(crate) fn edit_keymap_file(&mut self, cx: &mut Context<'_, Self>) {
        let opened = keymap::ensure_file(&self.keymap_file, &keymap::template())
            .map_err(|error| format!("Cannot create keymap.toml: {error}"))
            .and_then(|()| zed::open(&self.keymap_file, None).map_err(|error| error.to_string()));
        match opened {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(path = %self.keymap_file.display(), "open keymap settings failed: {error}");
                self.sidebar_error = Some(error);
            }
        }
        cx.notify();
    }

    /// Makes `keymap` the bound shortcuts at once (the Settings dialog wrote it): GPUI's key
    /// bindings, tooltips, the palette and the sidebar notice.
    pub(crate) fn set_keymap(
        &mut self,
        keymap: Keymap,
        errors: Vec<KeymapError>,
        cx: &mut Context<'_, Self>,
    ) {
        let bindings = keymap.key_bindings(cx.keyboard_mapper().as_ref());
        cx.clear_key_bindings();
        cx.bind_keys(bindings);
        cx.set_global(ActiveKeymap(keymap.clone()));
        self.keymap_error = keymap::notice(&errors);
        self.keymap_errors = errors;
        self.keymap = keymap;
        cx.notify();
    }

    /// Settings dialog "Edit settings.toml in Zed": creates the commented template if missing
    /// and opens it in Zed; a failure shows in the sidebar.
    pub(crate) fn edit_settings_file(&mut self, cx: &mut Context<'_, Self>) {
        let opened = keymap::ensure_file(&self.settings_file, mkrew_core::settings::template())
            .map_err(|error| format!("Cannot create settings.toml: {error}"))
            .and_then(|()| zed::open(&self.settings_file, None).map_err(|error| error.to_string()));
        match opened {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(path = %self.settings_file.display(), "open settings failed: {error}");
                self.sidebar_error = Some(error);
            }
        }
        cx.notify();
    }

    /// Cycles System → Light → Dark (ADR 0027) and saves it as `[ui] theme`. A settings file
    /// that does not parse is not written; the mode still changes for this session, the
    /// palette lists why (§20.0) and a toast says it was not saved.
    pub(crate) fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let mode = theme::next_mode(cx.theme().mode);
        theme::refresh(mode, window, cx);
        cx.notify();
        let value = format!("\"{}\"", mode.name());
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "theme", &value)
        {
            self.settings_write_failed("the theme", error, cx);
        }
    }

    /// Makes `mode` the diff layout (every file and repo) and keeps the current hunk in view
    /// (the two layouts have different row counts). False when nothing changed.
    pub(crate) fn apply_diff_mode(&mut self, mode: DiffMode) -> bool {
        if self.diff_mode == mode {
            return false;
        }
        self.diff_mode = mode;
        let hunk_row = self
            .workspace
            .and_then(|project_id| self.git.get(&project_id))
            .and_then(|view| match (&view.diff, view.hunk) {
                (git_view::Diff::Shown(shown), Some(hunk)) => shown.hunk_row(hunk, mode),
                _ => None,
            });
        self.scroll_diff_to(hunk_row.unwrap_or(0));
        true
    }

    /// Switches the Git diff viewer to `mode` and saves `[ui] diff_mode`. A settings file that
    /// does not parse keeps the choice for this run; the palette says why (§20.0) and a toast says it
    /// was not saved.
    pub(crate) fn set_diff_mode(&mut self, mode: DiffMode, cx: &mut Context<'_, Self>) {
        if !self.apply_diff_mode(mode) {
            return;
        }
        if let Err(error) = mkrew_core::settings::write_ui_value(
            &self.settings_file,
            "diff_mode",
            &format!("\"{}\"", mode.name()),
        ) {
            self.settings_write_failed("the diff layout", error, cx);
        }
        cx.notify();
    }

    /// Wraps long diff lines (or cuts them at the pane edge) and saves `[ui] diff_wrap`.
    pub(crate) fn set_diff_wrap(&mut self, wrap: bool, cx: &mut Context<'_, Self>) {
        if self.diff_wrap == wrap {
            return;
        }
        self.diff_wrap = wrap;
        if let Err(error) = mkrew_core::settings::write_ui_value(
            &self.settings_file,
            "diff_wrap",
            &wrap.to_string(),
        ) {
            self.settings_write_failed("the diff wrapping", error, cx);
        }
        cx.notify();
    }

    /// The prompt picker's empty state: opens the prompts folder (created if missing) in Zed
    /// to add Markdown prompts; a failure shows in the sidebar.
    pub(crate) fn open_prompts_dir(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let opened = std::fs::create_dir_all(&self.prompts_dir)
            .map_err(|error| format!("Cannot create the prompts folder: {error}"))
            .and_then(|()| zed::open(&self.prompts_dir, None).map_err(|error| error.to_string()));
        match opened {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(path = %self.prompts_dir.display(), "open prompts folder failed: {error}");
                self.sidebar_error = Some(error);
            }
        }
        self.close_prompt_picker(None, window, cx);
    }

    /// "Remove workspace…": asks, then `project.archive` (its sessions stop; its folder, tasks
    /// and history stay; adding the folder again brings it back). The sidebar drops it on
    /// `project.updated`; a refresh moves to another workspace if it was shown.
    pub(crate) fn archive_workspace(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(name) = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map(|project| project.name.clone())
        else {
            return;
        };
        let live = (self.sidebar.sessions().iter())
            .filter(|s| s.project_id == project_id && s.state != SessionState::Exited)
            .count();
        let stops = match live {
            0 => String::new(),
            1 => "Its running session stops. ".to_owned(),
            n => format!("Its {n} running sessions stop. "),
        };
        let detail = format!(
            "{stops}The folder, its tasks and history stay. Add the folder again to bring it back."
        );
        let answer = window.prompt(
            PromptLevel::Critical,
            &format!("Remove “{name}” from Mkrew?"),
            Some(detail.as_str()),
            &workspace_menu::REMOVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if !task_dialog::confirms_delete(answer.await.ok()) {
                return;
            }
            let archived = connection
                .call::<Project>(Request::ProjectArchive(ProjectRef { project_id }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match archived {
                    Ok(_) => {
                        shell.sidebar_error = None;
                        shell.refresh(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "project.archive failed: {error:#}");
                        shell.sidebar_error =
                            Some(format!("Cannot remove the workspace: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Makes `agent` the default agent of `project_id`; the sidebar picks the change up from
    /// `project.updated`.
    pub(crate) fn set_default_agent(
        &mut self,
        project_id: Ulid,
        agent: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_default_agent(project_id, &agent);
        self.update_project(params, "Cannot change the default agent", window, cx);
    }

    /// Makes `shell` the default shell of `project_id` (ADR 0032); the sidebar picks the change
    /// up from `project.updated`.
    pub(crate) fn set_default_shell(
        &mut self,
        project_id: Ulid,
        shell: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_default_shell(project_id, &shell);
        self.update_project(params, "Cannot change the default shell", window, cx);
    }

    /// Sets (or clears) the workspace color from the menu.
    pub(crate) fn set_workspace_color(
        &mut self,
        project_id: Ulid,
        color: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_color(project_id, color);
        self.update_project(params, "Cannot change the color", window, cx);
    }

    /// "Pin" / "Unpin" in the workspace menu: `project.update` with `pinned`; the
    /// `project.updated` event re-sorts the sidebar.
    pub(crate) fn pin_workspace(
        &mut self,
        project_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_pinned(project_id, pinned);
        self.update_project(params, "Cannot pin the workspace", window, cx);
    }

    /// Closes the workspace menu and sends `project.update`; a failure shows in the sidebar
    /// after `failure`.
    pub(crate) fn update_project(
        &mut self,
        params: ProjectUpdateParams,
        failure: &'static str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let project_id = params.project_id;
        let updated = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.spawn_in(window, async move |this, cx| {
            let updated = updated.await;
            let _ = this.update(cx, |shell, cx| {
                match updated {
                    Ok(_) => shell.sidebar_error = None,
                    Err(error) => {
                        tracing::warn!(%project_id, "project.update failed: {error:#}");
                        shell.sidebar_error = Some(format!("{failure}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Add workspace": "Open a folder…", or a repository URL, where the clone goes
    /// (Browse…), then Cancel and "Clone" (Enter).
    pub(crate) fn render_add_workspace(
        &self,
        dialog: &CloneDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let busy = dialog.cloning.is_some();
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let target = match dialog.target() {
            Some(target) => format!("Clones into {target}"),
            None => format!("Clones into a new folder in {}", dialog.parent),
        };
        let panel = div()
            .id("add-workspace-dialog")
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
            .on_key_down(cx.listener(Self::add_workspace_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::add_workspace_key_down, cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(font_size::CARD_TITLE)
                            .font_weight(FontWeight(weight::SEMIBOLD))
                            .text_color(rgb(theme.text))
                            .child("Add workspace"),
                    )
                    .child(
                        secondary_button(
                            "add-workspace-folder",
                            icon_text(Icon::FolderPlus, "Open a folder…", theme.text_dim),
                            theme,
                        )
                        .tooltip(tooltip(theme, "Add a folder on this computer", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.pick_workspace_folder(false, window, cx);
                        })),
                    ),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child("Or clone a Git repository (GitHub, GitLab, …):"),
            )
            .child(caret_field(
                "add-workspace-url",
                &dialog.url,
                "https://github.com/owner/repo.git or git@host:owner/repo.git",
                true,
                theme,
                cx,
                |shell, caret| {
                    if let Some(dialog) = shell.add_workspace.as_mut() {
                        dialog.url.click(caret.at, caret.extend, caret.clicks);
                    }
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::S))
                    .child(meta_line(theme, target.into()).min_w_0().truncate())
                    .child(
                        quiet_button("add-workspace-browse", "Browse…", theme, theme.hover)
                            .tooltip(tooltip(theme, "Choose where the clone goes", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.pick_workspace_folder(true, window, cx);
                            })),
                    ),
            )
            .children(dialog.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("add-workspace-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Close (a running clone goes on)", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_add_workspace(window, cx);
                            })),
                    )
                    .child(if busy {
                        disabled_button("add-workspace-clone", "Cloning…", theme, true)
                    } else {
                        button("add-workspace-clone", "Clone", theme, true)
                            .tooltip(tooltip(theme, "Clone and add as a workspace", "Enter"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.clone_workspace(window, cx);
                            }))
                    }),
            );
        Self::modal_shade("add-workspace-shade", panel, cx)
    }
}
