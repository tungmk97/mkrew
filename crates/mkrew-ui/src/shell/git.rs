//! Git view state: repos, status, log, diff, stage, commit, remotes.

use crate::*;

impl Shell {
    /// The Git view of `project_id`, created on first use.
    pub(crate) fn git_view(&mut self, project_id: Ulid) -> &mut GitView {
        self.git
            .entry(project_id)
            .or_insert_with(|| GitView::new(project_id, None))
    }

    /// The Git view of `project_id` while it still shows `at` (task worktree, repository; an
    /// answer sent for another is dropped).
    pub(crate) fn git_at(
        &mut self,
        project_id: Ulid,
        at: &(Option<Ulid>, Option<String>),
    ) -> Option<&mut GitView> {
        self.git
            .get_mut(&project_id)
            .filter(|view| view.worktree_task_id == at.0 && view.repo == at.1)
    }

    /// The chooser picked `target` (SPEC §20E.1): the Git view starts over on it, in its
    /// working changes.
    pub(crate) fn set_git_target(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.retarget_git(project_id, target, None) {
            self.refresh_git(project_id, window, cx);
        }
    }

    /// The "Repo" chooser picked `repo` (ADR 0029): the Git view starts over on it.
    pub(crate) fn set_git_repo(
        &mut self,
        project_id: Ulid,
        repo: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.retarget_git(project_id, None, repo) {
            self.refresh_git(project_id, window, cx);
        }
    }

    /// "Review diff" on the Changes card: the Git tab and Diff view of `repo` (`""` = the
    /// workspace folder), in its working changes. `retarget_git` first, so the refresh
    /// already asks for that repository (it stays: `pick_repo` keeps a listed repo). A view
    /// that already showed the repository may sit on History / Against base:
    /// `show(Working)` brings it back (its requests are dropped, the refresh asks status
    /// again).
    pub(crate) fn review_diff(
        &mut self,
        project_id: Ulid,
        repo: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.retarget_git(project_id, None, (!repo.is_empty()).then_some(repo));
        let _ = self.git_view(project_id).show(git_view::Section::Working);
        self.open_git(project_id, window, cx);
    }

    /// A click or Enter on a file (or History commit) of the Git list: the Diff view shows
    /// it; the keyboard stays in the list.
    pub(crate) fn open_git_diff(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.git_view(project_id).open_diff();
        self.show_view(View::Diff, window, cx);
    }

    /// `×` on the diff, or `Esc` in the Diff view: the diff closes and Terminals shows.
    pub(crate) fn close_git_diff(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.git_view(project_id).close_diff();
        self.follow_git_diff(project_id, window, cx);
        // The Git tab is still on screen: the keyboard stays in its list.
        if self.workspace == Some(project_id) && self.git_panel_shown() {
            window.focus(&self.git_focus, cx);
        }
    }

    /// The Diff view was left on screen with its diff closed (by hand, or the list
    /// emptied): Terminals shows instead, with its panes attached again. `show_view` focuses
    /// a pane; the keyboard stays in the Git list while its tab is on screen.
    fn follow_git_diff(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace == Some(project_id)
            && self.views.get(&project_id) == Some(&View::Diff)
            && !self.diff_open(project_id)
        {
            let in_git = self.git_focus.is_focused(window)
                || self.git_filter_focus.is_focused(window)
                || self.git_commit_focus.is_focused(window);
            self.show_view(View::Terminals, window, cx);
            if in_git && self.git_panel_shown() {
                window.focus(&self.git_focus, cx);
            }
        }
    }

    /// The entry points (Review diff, View changes, Diff since, the worktree dialog, the
    /// branch chip), after the caller retargeted the view or picked its section: the right
    /// panel on its Git tab and the Diff view. Git coming on screen refreshes itself
    /// (`show_view`); one on screen already is refreshed here.
    pub(crate) fn open_git(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let git_before = self.git_shown();
        self.open_git_diff(project_id, window, cx);
        self.show_panel_tab(PanelTab::Git, window, cx);
        if git_before {
            self.refresh_git(project_id, window, cx);
        }
    }

    /// Puts a fresh Git view of `target` / `repo` in place (the commit draft is kept); `false`
    /// = it shows them already.
    pub(crate) fn retarget_git(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        repo: Option<String>,
    ) -> bool {
        let old = self.git_view(project_id);
        if old.worktree_task_id == target && old.repo == repo {
            return false;
        }
        let draft = std::mem::take(&mut old.commit.text);
        let open = old.diff_open();
        let mut view = GitView::new(project_id, target);
        view.repo = repo;
        view.commit.text = draft;
        if open {
            // Another repository or worktree: its first file shows (or the diff closes when
            // it has none).
            view.open_diff();
        }
        self.git.insert(project_id, view);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.scroll_diff_to(0);
        true
    }

    /// Asks the Git view of `project_id` for its repositories, status and shown diff again.
    /// Every way the view is shown (show_view, activate_workspace, Retry) goes through it;
    /// `git.changed` does not (it only re-asks status / diff).
    pub(crate) fn refresh_git(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.sync_watches(cx);
        self.load_git_repos(project_id, window, cx);
        let requests = self.git_view(project_id).refresh();
        self.git_requests(project_id, requests, window, cx);
    }

    /// `git.repos` of `project_id` (ADR 0029): the "Repo" chips; a shown repository that is
    /// gone, or a workspace folder that is no repository, moves the view to
    /// [`git_view::pick_repo`] (never while a task worktree is shown: the daemon refuses a
    /// repo there).
    pub(crate) fn load_git_repos(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let listed = connection.call::<Vec<String>>(Request::GitRepos(ProjectRef { project_id }));
        cx.spawn_in(window, async move |this, cx| {
            let repos = match listed.await {
                Ok(repos) => repos,
                Err(error) => {
                    tracing::warn!(%project_id, "git.repos failed: {error:#}");
                    return;
                }
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let view = shell.git_view(project_id);
                let worktree = view.worktree_task_id;
                let picked = git_view::pick_repo(view.repo.as_deref(), &repos);
                shell.git_repos.insert(project_id, repos);
                // ponytail: the first status of a non-repo root may fail before this answer
                // and flash "Cannot read the git status"; send status after git.repos if it shows.
                if worktree.is_none() && shell.retarget_git(project_id, None, picked) {
                    let requests = shell.git_view(project_id).refresh();
                    shell.git_requests(project_id, requests, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The summary of the shown workspace (None until loaded, and for other workspaces).
    pub(crate) fn git_summary(&self) -> Option<&GitSummary> {
        self.git_summary
            .as_ref()
            .filter(|(id, _)| Some(*id) == self.workspace)
            .map(|(_, summary)| summary)
    }

    /// `git.repos`, then one `git.status` per repository, for the shown workspace (title bar).
    /// Asked on a workspace switch and on its `git.changed`, never polled and never for a
    /// hidden workspace. One request set at a time: a change that arrives meanwhile asks once
    /// more when it answers.
    pub(crate) fn load_git_summary(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.git_summary_busy {
            self.git_summary_again = true;
            return;
        }
        self.git_summary_busy = true;
        cx.spawn_in(window, async move |this, cx| {
            let loaded = git_summary_of(&connection, project_id).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.git_summary_busy = false;
                let again = std::mem::take(&mut shell.git_summary_again);
                let shown = shell.workspace;
                match loaded {
                    // An answer for a workspace that is no longer shown is dropped.
                    Ok(summary) if shown == Some(project_id) => {
                        let same = matches!(
                            &shell.git_summary,
                            Some((id, old)) if *id == project_id && *old == summary
                        );
                        if !same {
                            shell.git_summary = Some((project_id, summary));
                            cx.notify();
                        }
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%project_id, "git summary failed: {error:#}"),
                }
                if let Some(shown) = shown
                    && (again || shown != project_id)
                {
                    shell.load_git_summary(shown, window, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn git_requests(
        &mut self,
        project_id: Ulid,
        requests: git_view::Requests,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if requests.status {
            self.load_git_status(project_id, window, cx);
            self.load_session_files(project_id, window, cx);
        }
        if let Some(target) = requests.diff {
            self.load_git_diff(project_id, target, window, cx);
            // Another file: its diff comments are listed too.
            if let Some(path) = self.git_view(project_id).begin_comments() {
                self.load_comments(project_id, CommentContext::Diff, path, window, cx);
            }
        }
        if requests.log {
            self.load_git_log(project_id, window, cx);
        }
        if let Some(hash) = requests.show {
            self.load_git_show(project_id, hash, window, cx);
        }
        // The list emptied (commit, discard, clean worktree): the open diff closed with it.
        self.follow_git_diff(project_id, window, cx);
        cx.notify();
    }

    /// `git.log` of the History of `project_id`; while one is in flight, it is read once more
    /// after it answers.
    pub(crate) fn load_git_log(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.git_view(project_id).begin_log() else {
            return;
        };
        let at = (params.worktree_task_id, params.repo.clone());
        let loaded = connection.call::<Vec<Commit>>(Request::GitLog(params));
        cx.spawn_in(window, async move |this, cx| {
            let loaded = loaded.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, "git.log failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.logged(loaded);
                shell.git_requests(project_id, requests, window, cx);
            });
        })
        .detach();
    }

    /// `git.show` of History commit `hash`: its files, then its first file's diff.
    pub(crate) fn load_git_show(
        &mut self,
        project_id: Ulid,
        hash: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let params = self.git_view(project_id).show_params(hash.clone());
        let at = (params.worktree_task_id, params.repo.clone());
        let loaded = connection.call::<CommitDetail>(Request::GitShow(params));
        cx.spawn_in(window, async move |this, cx| {
            let loaded = loaded.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, commit = hash, "git.show failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.commit_shown(&hash, loaded);
                shell.git_requests(project_id, requests, window, cx);
            });
        })
        .detach();
    }

    /// A click on a History commit: lists its files; the Diff view shows its first file's
    /// diff.
    pub(crate) fn select_git_commit(
        &mut self,
        project_id: Ulid,
        hash: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.git_focus, cx);
        let show = self.git_view(project_id).select_commit(hash);
        self.scroll_diff_to(0);
        let requests = git_view::Requests {
            show,
            ..git_view::Requests::default()
        };
        self.git_requests(project_id, requests, window, cx);
        self.open_git_diff(project_id, window, cx);
    }

    /// A click on a section of the Git header (Working changes / Against base / History).
    pub(crate) fn show_git_section(
        &mut self,
        project_id: Ulid,
        section: git_view::Section,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.git_focus, cx);
        let requests = self.git_view(project_id).show(section);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.git_requests(project_id, requests, window, cx);
    }

    /// `git.status` of the Git view of `project_id` in its shown mode (also keeps the daemon
    /// watching the workspace, so `git.changed` follows file changes).
    pub(crate) fn load_git_status(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.git_view(project_id).begin_status() else {
            return;
        };
        let at = (params.worktree_task_id, params.repo.clone());
        let loaded = connection.call::<GitStatus>(Request::GitStatus(params));
        cx.spawn_in(window, async move |this, cx| {
            let loaded = loaded.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, "git.status failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.status_loaded(loaded);
                shell.git_requests(project_id, requests, window, cx);
                // Which of the listed files are still marked as viewed (no-op in the History).
                shell.load_git_viewed(project_id, window, cx);
                // Everything committed: the box is gone, the list takes the keyboard back.
                if shell.git_commit_focus.is_focused(window) && !shell.git_commit_shown() {
                    window.focus(&shell.git_focus, cx);
                }
            });
        })
        .detach();
    }

    /// `session.files` of the last 24 h for the Git view's "Changed by" filter (SPEC §20C.4);
    /// read with every status, so it follows the same refreshes. A failure keeps the last answer.
    pub(crate) fn load_session_files(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        let at = view.at();
        let (params, serial) = view.begin_session_files(status::now_ms());
        let loaded = connection.call::<SessionFiles>(Request::SessionFiles(params));
        cx.spawn_in(window, async move |this, cx| {
            let files = match loaded.await {
                Ok(files) => files,
                Err(error) => {
                    tracing::warn!(%project_id, "session.files failed: {error:#}");
                    return;
                }
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.session_files_loaded(serial, files);
                shell.git_requests(project_id, requests, window, cx);
            });
        })
        .detach();
    }

    /// "Changed by" in the Git view: keeps only `session`'s files (`None` = All).
    pub(crate) fn filter_git_changed_by(
        &mut self,
        project_id: Ulid,
        session: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let requests = self.git_view(project_id).filter_changed_by(session);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.git_requests(project_id, requests, window, cx);
    }

    /// `git.diff` of the selected file; while one is in flight, the selected file's diff is
    /// read once more after it answers.
    pub(crate) fn load_git_diff(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        if !view.begin_diff() {
            return;
        }
        let at = view.at();
        let loaded = connection.call::<FileDiff>(Request::GitDiff(target.params(
            project_id,
            at.0,
            at.1.clone(),
        )));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            // Syntax highlighting is slow on big diffs: lay out off the UI thread.
            let loaded = match loaded.await {
                Ok(diff) => Ok(background
                    .spawn(async move { git_view::Shown::new(diff) })
                    .await),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, path = target.path, "git.diff failed: {error}");
                }
                let again = shell
                    .git_at(project_id, &at)
                    .and_then(|view| view.diff_loaded(&target, loaded));
                if let Some(target) = again {
                    shell.load_git_diff(project_id, target, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Follows `git.changed` while the active workspace's Git is on screen (the panel's tab or
    /// the diff); hidden ones refresh when shown.
    pub(crate) fn git_event(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace.filter(|_| self.git_shown()) else {
            return;
        };
        let Some(view) = self.git.get_mut(&project_id) else {
            return;
        };
        let requests = view.apply(event);
        self.git_requests(project_id, requests, window, cx);
    }

    /// A click on a changed file: shows its diff in the Diff view.
    pub(crate) fn select_git_file(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.git_focus, cx);
        let diff = self.git_view(project_id).select(target);
        self.scroll_diff_to(0);
        let requests = git_view::Requests {
            diff,
            ..git_view::Requests::default()
        };
        self.git_requests(project_id, requests, window, cx);
        self.open_git_diff(project_id, window, cx);
    }

    /// ↑ ↓ / j k move between files; Enter opens the selection's diff; Esc unselects the picked
    /// diff lines, else closes the diff (an open Git menu has the keyboard itself); the other
    /// Git keys are keymap commands ([`Self::git_key`]).
    pub(crate) fn git_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
            if let Some(project_id) = self.workspace
                && self
                    .git
                    .get(&project_id)
                    .is_some_and(|view| view.comments.selection.is_some())
            {
                cx.stop_propagation();
                return self.clear_diff_pick(project_id, cx);
            }
            if let Some(project_id) = self.workspace
                && self.view() == View::Diff
            {
                cx.stop_propagation();
                return self.close_git_diff(project_id, window, cx);
            }
        }
        if event.keystroke.key == "enter"
            && !event.keystroke.modifiers.modified()
            && let Some(project_id) = self.workspace
            && self
                .git
                .get(&project_id)
                .is_some_and(|view| view.selected.is_some())
        {
            cx.stop_propagation();
            return self.open_git_diff(project_id, window, cx);
        }
        let Some(key) = git_view::key(&event.keystroke) else {
            return;
        };
        cx.stop_propagation();
        self.git_key(key, window, cx);
    }

    /// A Git key of the active workspace: files move (kept in view), hunks move (scrolled to
    /// the top), the mode toggles, or the selected file / current hunk is (un)staged.
    pub(crate) fn git_key(
        &mut self,
        key: git_view::Key,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if let git_view::Key::Stage(stage) = key {
            if let Some(change) = self.git_view(project_id).stage_key(stage) {
                self.stage_git(project_id, change, window, cx);
            }
            return;
        }
        let mode = self.diff_mode;
        let view = self.git_view(project_id);
        let requests = view.key(key);
        let selected = view.selected.clone();
        // The selected file, else (History, files loading) the expanded commit.
        let file_row = view.list_rows().iter().position(|row| match row {
            git_view::ListRow::File(target, _) => selected.as_ref() == Some(target),
            git_view::ListRow::Commit { expanded, .. } => selected.is_none() && *expanded,
            git_view::ListRow::Header(..)
            | git_view::ListRow::Note(_)
            | git_view::ListRow::Hint(_) => false,
        });
        let hunk_row = match (&view.diff, view.hunk) {
            (git_view::Diff::Shown(shown), Some(hunk)) => shown.hunk_row(hunk, mode),
            _ => None,
        };
        if let Some(at) = file_row {
            self.git_files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
        match key {
            git_view::Key::NextHunk | git_view::Key::PreviousHunk => {
                if let Some(at) = hunk_row {
                    self.scroll_diff_to(at);
                }
            }
            git_view::Key::Up | git_view::Key::Down
                if requests.diff.is_some() || requests.show.is_some() =>
            {
                self.scroll_diff_to(0);
            }
            _ => {}
        }
        self.git_requests(project_id, requests, window, cx);
    }

    /// Shift+S / Shift+U: stage the selected file's group (every change without one) or
    /// unstage every staged file.
    pub(crate) fn git_stage_all(
        &mut self,
        stage: git_view::Stage,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if let Some(change) = self.git_view(project_id).stage_all_key(stage) {
            self.stage_git(project_id, change, window, cx);
        }
    }

    /// `x`: the discard confirmation of the selected file (nothing for a file that cannot be
    /// discarded: `open_git_discard` checks).
    pub(crate) fn git_discard_selected(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let target = (self.git.get(&project_id)).and_then(|view| view.selected.clone());
        if let Some(target) = target {
            self.open_git_discard(project_id, target, window, cx);
        }
    }

    /// Scrolls the diff to `row` once its list is laid out for the shown diff (the render
    /// resets the list for another diff or mode, which would drop a scroll done now).
    pub(crate) fn scroll_diff_to(&self, row: usize) {
        self.git_diff_scroll_to.set(Some(row));
    }

    /// Opens the selected changed file of the Git view in Zed; a failure shows in the diff
    /// header.
    pub(crate) fn open_git_file_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        if self.local_only(cx) {
            return;
        }
        let Some((target, path)) = self.git.get(&project_id).and_then(|view| {
            let selected = view.selected.as_ref()?;
            Some((view.worktree_task_id, selected.path.clone()))
        }) else {
            return;
        };
        let Some(full) = self.target_file(project_id, target, &path) else {
            return;
        };
        let error = zed::open(&full, None).err().map(|error| {
            tracing::warn!(%project_id, path, "open in Zed failed: {error}");
            error.to_string()
        });
        self.git_view(project_id).zed_error = error;
        cx.notify();
    }

    /// `git.stage` / `git.unstage` of a file or hunk; then the list and diff are read again
    /// (a failure shows under the diff header).
    pub(crate) fn stage_git(
        &mut self,
        project_id: Ulid,
        change: git_view::StageRequest,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if !self.git_view(project_id).begin_stage() {
            return;
        }
        cx.notify();
        let stage = change.stage;
        let at = (change.params.worktree_task_id, change.params.repo.clone());
        let sent = connection.request(change.request());
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%project_id, ?stage, "git stage/unstage failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    // The view moved to another repo or worktree: no row left for the failure.
                    if let Err(error) = result {
                        shell.toast_error(format!("{} failed: {error}", stage.label()), cx);
                    }
                    return;
                };
                let requests = view.staged(result);
                shell.git_requests(project_id, requests, window, cx);
            });
        })
        .detach();
    }

    /// Keys typed in the commit box: Enter is a new line, Ctrl+Enter commits, Esc goes back
    /// to the changed-file list (the draft stays).
    pub(crate) fn git_commit_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let draft = &mut self.git_view(project_id).commit;
        if let Some(text) = paste {
            draft.insert(&text);
        }
        copy_selection(&key, draft.text.input(), cx);
        match draft.key(key) {
            git_actions::Outcome::Edited => cx.notify(),
            git_actions::Outcome::Leave => {
                window.focus(&self.git_focus, cx);
                cx.notify();
            }
            git_actions::Outcome::Commit => self.commit_git(project_id, window, cx),
        }
    }

    /// Whether the panel's Git tab is shown with its commit box.
    pub(crate) fn git_commit_shown(&self) -> bool {
        self.git_panel_shown()
            && self
                .workspace
                .and_then(|id| self.git.get(&id))
                .is_some_and(GitView::commit_box_shown)
    }

    /// `git.commit` of the staged changes with the drafted message (after one `git.stage` of
    /// every change when the box offered "stage all" and the user confirmed); then the view
    /// reads its status again either way (a hook may have changed files; a failure stays under
    /// the box).
    pub(crate) fn commit_git(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        let staged = match &view.status {
            git_view::Status::Loaded(status) => status.staged.len(),
            git_view::Status::Loading | git_view::Status::Failed(_) => 0,
        };
        let stage_all = view.stage_all();
        let unstaged = stage_all.as_ref().map_or(0, |all| all.params.paths.len());
        let at = view.at();
        let begun = view.commit.begin(project_id, staged, unstaged);
        cx.notify();
        let (mut params, stage_first) = match begun {
            Some(git_actions::Begin::Commit(params)) => (params, None),
            Some(git_actions::Begin::StageAllThenCommit(params)) => (params, stage_all),
            None => return,
        };
        (params.worktree_task_id, params.repo) = at.clone();
        cx.spawn_in(window, async move |this, cx| {
            let staged = match stage_first {
                Some(change) => match connection.request(change.request()).await {
                    Ok(Outcome::Result(_)) => Ok(()),
                    Ok(Outcome::Error(body)) => Err(body.message),
                    Err(error) => Err(format!("{error:#}")),
                }
                .map_err(|error| format!("cannot stage the changes: {error}")),
                None => Ok(()),
            };
            let result = match staged {
                Ok(()) => connection
                    .call::<GitCommitResult>(Request::GitCommit(params))
                    .await
                    .map(|_| ())
                    .map_err(|error| format!("{error:#}")),
                Err(error) => Err(error),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%project_id, "git.commit failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    if let Err(error) = result {
                        shell.toast_error(format!("Commit failed: {error}"), cx);
                    }
                    return;
                };
                view.commit.committed(result);
                shell.refresh_git(project_id, window, cx);
            });
        })
        .detach();
    }

    /// ✦ in the commit box (SPEC §12B): the workspace's default agent writes a Conventional
    /// Commits message from the changes; the answer arrives as `ai.generated`.
    pub(crate) fn generate_commit_message(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        let generation_id = Ulid::new();
        if !view.commit.begin_generate(generation_id) {
            return;
        }
        let params = AiGenerateParams {
            generation_id,
            project_id: Some(project_id),
            worktree_task_id: view.worktree_task_id,
            repo: view.repo.clone(),
            agent_profile: None,
            kind: AiKind::CommitMessage,
            input: String::new(),
            web: false,
        };
        cx.notify();
        let started = connection.call::<()>(Request::AiGenerate(params));
        cx.spawn_in(window, async move |this, cx| {
            let Err(error) = started.await else {
                return;
            };
            let _ = this.update_in(cx, |shell, _, cx| {
                for view in shell.git.values_mut() {
                    view.commit
                        .generated(generation_id, Err(format!("{error:#}")));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Fetch or Push of the Git view header; then the view reads its status again (base,
    /// ahead/behind). A failure shows under the header.
    pub(crate) fn run_git_remote(
        &mut self,
        project_id: Ulid,
        remote: Remote,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        if !view.remotes.begin(remote) {
            return;
        }
        let at = view.at();
        cx.notify();
        let sent = connection.request(remote.request(project_id, at.0, at.1.clone()));
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%project_id, ?remote, "git fetch/push failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    if let Err(error) = result {
                        shell.toast_error(format!("{} failed: {error}", remote.label()), cx);
                    }
                    return;
                };
                view.remotes.finished(remote, result);
                shell.refresh_git(project_id, window, cx);
            });
        })
        .detach();
    }

    /// "Review in Docs" for an opened `.md` file: the Docs view shows it.
    pub(crate) fn review_in_docs(
        &mut self,
        project_id: Ulid,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.docs
            .entry(project_id)
            .or_insert_with(|| Docs::new(project_id));
        self.select_doc(project_id, &path, window, cx);
        self.show_view(View::Docs, window, cx);
    }
}

/// The repositories of `project_id` and each one's Working-mode status. Asking `git.repos`
/// first means a root without `.git` never sends the status that would fail. The status
/// requests go out together; a repository whose status fails is left out (logged).
pub(crate) async fn git_summary_of(
    connection: &Connection,
    project_id: Ulid,
) -> anyhow::Result<GitSummary> {
    let names = connection
        .call::<Vec<String>>(Request::GitRepos(ProjectRef { project_id }))
        .await?;
    let pending: Vec<_> = names
        .into_iter()
        .map(|name| {
            let params = GitStatusParams {
                project_id,
                worktree_task_id: None,
                repo: (!name.is_empty()).then(|| name.clone()),
                mode: GitMode::Working,
                since: None,
            };
            let status = connection.call::<GitStatus>(Request::GitStatus(params));
            (name, status)
        })
        .collect();
    let mut repos = Vec::new();
    for (name, status) in pending {
        match status.await {
            Ok(status) => repos.push(RepoSummary::of(&name, &status)),
            Err(error) => tracing::warn!(%project_id, repo = name, "git.status failed: {error:#}"),
        }
    }
    Ok(GitSummary { repos })
}
