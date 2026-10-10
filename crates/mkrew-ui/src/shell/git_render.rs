//! Git view rendering: header, choosers, remote actions, commit box.

use crate::*;

impl Shell {
    /// The workspace's changes (SPEC §12B): the selected file's unified diff, and on its right
    /// the branch header and mode toggle over the changed-file list and the commit box.
    pub(crate) fn render_git(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let empty = |sentence: String, detail: Option<String>| {
            message(theme, None, sentence.into(), detail.map(Into::into)).flex_1()
        };
        let Some(view) = self.git.get(&project_id) else {
            return empty("Loading changes…".into(), None).into_any_element();
        };
        let status = match &view.status {
            git_view::Status::Loaded(status) => Some(&**status),
            git_view::Status::Loading | git_view::Status::Failed(_) => None,
        };
        let column = div()
            .id("git-files")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .pb(px(space::S))
            .track_focus(&self.git_focus)
            .key_context(GIT_CONTEXT)
            .on_key_down(cx.listener(Self::git_key_down))
            .child(self.git_header(project_id, view, status, theme, cx));
        let rows = view.list_rows();
        let any_files = !rows.is_empty();
        let commits = view.history.as_ref().map(|history| &history.commits);
        let list = match (commits, &view.status) {
            (Some(Commits::Loading), _) => {
                empty("Loading history…".into(), None).into_any_element()
            }
            (Some(Commits::Failed(error)), _) => {
                empty("Cannot read the history.".into(), Some(error.clone()))
                    .child(
                        button("git-history-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| shell.refresh_git(project_id, window, cx),
                        )),
                    )
                    .into_any_element()
            }
            (Some(Commits::Loaded(_)), _) if !any_files => empty(
                "No commits yet.".into(),
                Some("Commits on this branch show here.".into()),
            )
            .child(
                button("git-history-back", "Show changes", theme, true)
                    .tooltip(tooltip(
                        theme,
                        "Back to the changes",
                        self.keymap.label(Command::GitToggleHistory),
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        window.focus(&shell.git_focus, cx);
                        let requests = shell.git_view(project_id).toggle_history();
                        shell.git_requests(project_id, requests, window, cx);
                    })),
            )
            .into_any_element(),
            (Some(Commits::Loaded(_)), _) => {
                self.git_list(project_id, view, rows, theme, window, cx)
            }
            (None, git_view::Status::Loading) => {
                empty("Loading changes…".into(), None).into_any_element()
            }
            (None, git_view::Status::Failed(error)) if view.since.is_some() => {
                // Most likely the checkpoint is gone (gc, rewritten history): Retry won't help.
                let sentence = match view.since.as_ref().map(|since| since.origin) {
                    Some(git_view::Origin::Turn { n, .. }) => {
                        format!("Cannot read the changes since turn {n}.")
                    }
                    _ => "Cannot read the task's changes.".to_owned(),
                };
                empty(sentence, Some(error.clone()))
                    .child(
                        button("git-task-back", "Working changes", theme, true).on_click(
                            cx.listener(move |shell, _, window, cx| {
                                let working = git_view::Section::Working;
                                shell.show_git_section(project_id, working, window, cx);
                            }),
                        ),
                    )
                    .into_any_element()
            }
            (None, git_view::Status::Failed(error)) => {
                empty("Cannot read the git status.".into(), Some(error.clone()))
                    .child(
                        button("git-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| shell.refresh_git(project_id, window, cx),
                        )),
                    )
                    .into_any_element()
            }
            (None, git_view::Status::Loaded(_))
                if !any_files && view.changed_by.session.is_some() =>
            {
                // "Changed by" hides every listed file; All brings them back.
                let who = view
                    .changed_by_label(self.sidebar.sessions())
                    .unwrap_or_else(|| "this session".to_owned());
                empty(format!("No changes here by {who}."), None)
                    .child(
                        button("git-changed-by-show-all", "Show all", theme, true).on_click(
                            cx.listener(move |shell, _, window, cx| {
                                shell.filter_git_changed_by(project_id, None, window, cx);
                            }),
                        ),
                    )
                    .into_any_element()
            }
            (None, git_view::Status::Loaded(status)) if !any_files => {
                // No merge-base with a commit checked out: the base names nothing usable.
                let unusable = status.head.is_some() && status.merge_base.is_none();
                let no_base = view.since.is_none()
                    && view.mode == GitMode::Base
                    && (status.base_ref.is_none() || unusable);
                let since = view.since.as_ref().map(|since| match since.origin {
                    git_view::Origin::Turn { n, .. } => (
                        format!("No changes since turn {n} started."),
                        Some("Its changes may have been restored or undone.".to_owned()),
                    ),
                    git_view::Origin::Task(_) => (
                        "No changes since the task's run started.".to_owned(),
                        Some("Its changes may have been undone or committed elsewhere.".to_owned()),
                    ),
                });
                let (sentence, detail) = match (since, view.mode, &status.base_ref) {
                    (Some(since), _, _) => since,
                    (None, GitMode::Working, _) => ("No uncommitted changes.".to_owned(), None),
                    (None, GitMode::Base, None) => (
                        "No base branch found.".to_owned(),
                        Some("None of origin/HEAD, main or master exists here.".to_owned()),
                    ),
                    (None, GitMode::Base, Some(base)) if unusable => (
                        format!("Cannot compare with {base}."),
                        Some("It names no commit here, or shares no history with HEAD.".to_owned()),
                    ),
                    (None, GitMode::Base, Some(base)) => {
                        (format!("No changes against {base}."), None)
                    }
                };
                let (label, tip) = match view.mode {
                    GitMode::Working => ("Against base", "Show everything on this branch"),
                    GitMode::Base => ("Working changes", "Show uncommitted changes"),
                };
                // Without a base, picking one is the primary action.
                let buttons = div()
                    .flex()
                    .gap(px(space::S))
                    .when(no_base, |el| {
                        el.child(
                            button("git-empty-base", "Set base…", theme, true)
                                .tooltip(tooltip(theme, "Choose the branch to compare with", ""))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.open_git_base(project_id, window, cx);
                                })),
                        )
                    })
                    .child(
                        button("git-empty-mode", label, theme, !no_base)
                            .tooltip(tooltip(
                                theme,
                                tip,
                                self.keymap.label(Command::GitToggleMode),
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.git_focus, cx);
                                let requests = shell.git_view(project_id).toggle_mode();
                                shell.git_requests(project_id, requests, window, cx);
                            })),
                    );
                empty(sentence, detail).child(buttons).into_any_element()
            }
            (None, git_view::Status::Loaded(_)) => {
                self.git_list(project_id, view, rows, theme, window, cx)
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            // The list column stays at the right edge even when no diff is shown.
            .justify_end()
            .font_family(theme.ui_font_family.clone())
            .when(any_files, |el| {
                el.child(self.render_git_diff(project_id, view, theme, window, cx))
            })
            .child(
                // The commit box sits outside the `GIT_CONTEXT` column: typing never stages.
                div()
                    .flex_none()
                    .w(px(theme::GIT_FILE_LIST_WIDTH))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(column.child(list))
                    .when_some(status.filter(|_| view.commit_box_shown()), |el, status| {
                        el.child(self.git_commit_box(project_id, view, status, theme, window, cx))
                    }),
            )
            .into_any_element()
    }

    /// Branch, upstream and ahead/behind, then the Working changes / Against base / History
    /// switch and the base it compares to.
    pub(crate) fn git_header(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: Option<&GitStatus>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let branch = match status {
            Some(status) => status
                .branch
                .clone()
                .unwrap_or_else(|| "Detached HEAD".to_owned()),
            None => "Git".to_owned(),
        };
        let tracking = status.map(|status| {
            let upstream = match &status.upstream {
                Some(upstream) => format!("→ {upstream}"),
                None => "No upstream".to_owned(),
            };
            match git_view::ahead_behind(status) {
                Some(counts) => format!("{upstream}  {counts}"),
                None => upstream,
            }
        });
        let base = status.map(|status| match (&status.base_ref, status.base_auto) {
            (Some(base), true) => format!("Base: {base} (auto-detected)"),
            (Some(base), false) => format!("Base: {base}"),
            (None, _) => "No base branch".to_owned(),
        });
        // A task's or turn's changes: what is compared replaces the base line.
        let since = view
            .since
            .as_ref()
            .filter(|_| view.section() == git_view::Section::Since);
        let base = match since {
            Some(since) => Some(since.label.clone()),
            None => base,
        };
        let shows_since = since.is_some();
        // The upstream line ends with Fetch and Push.
        let tracking = tracking.map(|tracking| {
            div()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .child(
                    meta_line(theme, tracking.into())
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap(),
                )
                .children([Remote::Fetch, Remote::Push].map(|remote| {
                    self.git_remote_action(project_id, view, status, remote, theme, cx)
                }))
        });
        let chooser = self.target_chooser(project_id, view.worktree_task_id, true, theme, cx);
        let repos = self.repo_chooser(project_id, view, theme, cx);
        let toggle_keys = SharedString::from(self.keymap.label(Command::GitToggleMode));
        let history_keys = SharedString::from(self.keymap.label(Command::GitToggleHistory));
        // Lighter than the active segment's fill, so hovering never looks selected.
        let hover = theme.surface;
        let shown = view.section();
        let segment = |section: git_view::Section,
                       id: &'static str,
                       label: &'static str,
                       tip: &'static str| {
            let active = shown == section;
            let keys = if section == git_view::Section::History {
                history_keys.clone()
            } else {
                toggle_keys.clone()
            };
            // Sized from the label: three equal segments would cut "Against base".
            div()
                .id(id)
                .flex_auto()
                .whitespace_nowrap()
                .flex()
                .justify_center()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .text_size(font_size::BODY)
                .when(active, |el| {
                    el.bg(rgb(theme.surface_raised))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                })
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_git_section(project_id, section, window, cx);
                        }))
                })
                .tooltip(tooltip(theme, tip, keys))
                .child(label)
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .px(px(space::M))
            .pt(px(space::XS))
            .children(chooser)
            .children(repos)
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(branch),
            )
            .children(tracking)
            // Wrapped, not cut: git's push / fetch errors say what to do.
            .children(view.remotes.error.clone().map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text))
                    .child(format!("✕ {error}"))
            }))
            .child(
                div()
                    .mt(px(space::XS))
                    .flex()
                    .gap(px(space::XS))
                    .child(segment(
                        git_view::Section::Working,
                        "git-mode-working",
                        "Working",
                        "Uncommitted changes: staged, unstaged, untracked",
                    ))
                    .child(segment(
                        git_view::Section::Base,
                        "git-mode-base",
                        "Against base",
                        "Everything on this branch since it left the base",
                    ))
                    .child(segment(
                        git_view::Section::History,
                        "git-mode-history",
                        "History",
                        "The last 50 commits and their changes",
                    )),
            )
            .children(base.map(|base| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        meta_line(theme, base.into())
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap(),
                    )
                    .when(shows_since, |el| {
                        el.child(
                            row_action("git-task-close".into(), "Close", theme)
                                .tooltip(tooltip(
                                    theme,
                                    "Back to the working changes",
                                    toggle_keys.clone(),
                                ))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.show_git_section(
                                        project_id,
                                        git_view::Section::Working,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                    })
                    .when(!shows_since, |el| {
                        el.child(
                            row_action(
                                "git-base-change".into(),
                                if status.is_some_and(|status| status.base_ref.is_some()) {
                                    "Change…"
                                } else {
                                    "Set base…"
                                },
                                theme,
                            )
                            .tooltip(tooltip(theme, "Choose the branch to compare with", ""))
                            .on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    shell.open_git_base(project_id, window, cx);
                                },
                            )),
                        )
                    })
            }))
            .children(self.git_changed_by(project_id, view, theme, cx))
            .children(
                view.refresh_error
                    .clone()
                    .map(|error| notice(theme.text, format!("✕ Refresh failed: {error}"))),
            )
    }

    /// "Project | <task worktree>…" (SPEC §20E.1): what the Files (`git: false`) or Git view
    /// shows; hidden while no task of the workspace has a worktree.
    pub(crate) fn target_chooser(
        &self,
        project_id: Ulid,
        shown: Option<Ulid>,
        git: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let choices = worktree_target::choices(project_id, self.board.tasks());
        if choices.is_empty() {
            return None;
        }
        let view = if git { "git" } else { "files" };
        let chips = choices
            .into_iter()
            .map(|choice| {
                let target = choice.worktree_task_id;
                Chip {
                    id: match target {
                        Some(task_id) => SharedString::from(format!("{view}-target-{task_id}")),
                        None => SharedString::from(format!("{view}-target-project")),
                    },
                    tip: match &choice.branch {
                        Some(branch) => format!("This task's worktree: {branch}"),
                        None => "The workspace folder".to_owned(),
                    },
                    active: shown == target,
                    label: choice.label.into(),
                    pick: target,
                }
            })
            .collect();
        Some(Self::chip_row(
            "Showing",
            chips,
            theme,
            move |shell, target, window, cx| {
                if git {
                    window.focus(&shell.git_focus, cx);
                    shell.set_git_target(project_id, target, window, cx);
                } else {
                    window.focus(&shell.files_focus, cx);
                    shell.set_files_target(project_id, target, window, cx);
                }
            },
            cx,
        ))
    }

    /// "Repo: <workspace> | fe | be" (ADR 0029): which repository of the workspace the Git view
    /// shows; hidden with fewer than two and while a task worktree is shown (one repository).
    pub(crate) fn repo_chooser(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        if view.worktree_task_id.is_some() {
            return None;
        }
        let repos = self.git_repos.get(&project_id)?;
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or("Workspace", |p| p.name.as_str());
        let chips: Vec<_> = git_view::repo_choices(repos, workspace)
            .into_iter()
            .enumerate()
            .map(|(at, (repo, label))| Chip {
                id: SharedString::from(format!("git-repo-{at}")),
                label: SharedString::from(label.to_owned()),
                tip: match &repo {
                    Some(repo) => format!("The git repository in {repo}/"),
                    None => "The workspace folder's git repository".to_owned(),
                },
                active: view.repo == repo,
                pick: repo,
            })
            .collect();
        if chips.is_empty() {
            return None;
        }
        Some(Self::chip_row(
            "Repo",
            chips,
            theme,
            move |shell, repo, window, cx| {
                window.focus(&shell.git_focus, cx);
                shell.set_git_repo(project_id, repo, window, cx);
            },
            cx,
        ))
    }

    /// A chooser row: `heading`, then one chip per choice. A click on an inactive chip calls
    /// `on_pick` with its `pick`.
    pub(crate) fn chip_row<P: Clone + 'static>(
        heading: &'static str,
        chips: Vec<Chip<P>>,
        theme: &Theme,
        on_pick: impl Fn(&mut Self, P, &mut Window, &mut Context<'_, Self>) + Clone + 'static,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let hover = theme.surface;
        let chips = chips.into_iter().map(|chip| {
            let Chip {
                id,
                label,
                tip,
                active,
                pick,
            } = chip;
            let on_pick = on_pick.clone();
            div()
                .id(id)
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .tooltip(tooltip(theme, &tip, ""))
                .when(active, |el| {
                    el.bg(rgb(theme.surface_raised))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                })
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            on_pick(shell, pick.clone(), window, cx);
                        }))
                })
                .child(label)
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(space::XS))
            .text_size(font_size::META)
            .child(div().text_color(rgb(theme.text_muted)).child(heading))
            .children(chips)
    }

    /// "Changed by: All | claude #1 | …" (SPEC §20C.4): keeps only the files a session changed
    /// in the last 24 h; hidden while no session changed any (and in the History).
    pub(crate) fn git_changed_by(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let changers = view.changers(self.sidebar.sessions());
        if changers.is_empty() {
            return None;
        }
        let filtered = view.changed_by.session;
        let hover = theme.surface;
        let chip = |id: SharedString, label: String, session: Option<Ulid>| {
            let active = filtered == session;
            let tip = match session {
                None => "Every changed file".to_owned(),
                Some(_) => format!("Only the files {label} changed in the last 24 h"),
            };
            div()
                .id(id)
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .tooltip(tooltip(theme, &tip, ""))
                .whitespace_nowrap()
                .when(active, |el| {
                    el.bg(rgb(theme.surface_raised))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                })
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            window.focus(&shell.git_focus, cx);
                            shell.filter_git_changed_by(project_id, session, window, cx);
                        }))
                })
                .child(label)
        };
        Some(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.text_muted)).child("Changed by"))
                .child(chip("git-changed-by-all".into(), "All".into(), None))
                .children(changers.into_iter().map(|changer| {
                    let id = SharedString::from(format!("git-changed-by-{}", changer.session_id));
                    chip(id, changer.label, Some(changer.session_id))
                })),
        )
    }

    /// "Fetch" / "Push" on the header's upstream line: shows "Fetching…" while it runs, and is
    /// faded while another runs (or, for Push, without a branch and upstream).
    pub(crate) fn git_remote_action(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: Option<&GitStatus>,
        remote: Remote,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let id = SharedString::from(format!("git-{}", remote.label().to_lowercase()));
        let running = view.remotes.running;
        let no_upstream = status.is_none_or(|s| s.branch.is_none() || s.upstream.is_none());
        let tip = match remote {
            Remote::Fetch => "Fetch from the remotes (refreshes the base)",
            Remote::Push if no_upstream => "This branch has no upstream to push to",
            Remote::Push => "Push this branch to its upstream (never forced)",
        };
        let idle = |label: &'static str, opacity: f32| {
            div()
                .id(id.clone())
                .flex_none()
                .px(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .opacity(opacity)
                .child(label)
        };
        if running == Some(remote) {
            return idle(remote.running_label(), 1.0).into_any_element();
        }
        if running.is_some() || (remote == Remote::Push && no_upstream) {
            return idle(remote.label(), opacity::DISABLED)
                .tooltip(tooltip(theme, tip, ""))
                .into_any_element();
        }
        row_action(id, remote.label(), theme)
            .tooltip(tooltip(theme, tip, ""))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.run_git_remote(project_id, remote, window, cx);
            }))
            .into_any_element()
    }

    /// The commit message box under the changed-file list (Working changes): Enter breaks a
    /// line, Ctrl+Enter or the button commits the staged changes.
    pub(crate) fn git_commit_box(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: &GitStatus,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let draft = &view.commit;
        let generating = draft.generating.is_some();
        let focused = self.git_commit_focus.is_focused(window);
        let staged = status.staged.len();
        let summary = match staged {
            0 => "Nothing staged".to_owned(),
            1 => "1 staged file".to_owned(),
            n => format!("{n} staged files"),
        };
        let label = if draft.busy {
            "Committing…"
        } else {
            "Commit"
        };
        let commit = if draft.busy || generating || staged == 0 {
            disabled_button("git-commit", label, theme, true)
        } else {
            button("git-commit", label, theme, true).on_click(
                cx.listener(move |shell, _, window, cx| shell.commit_git(project_id, window, cx)),
            )
        };
        let tip = if staged == 0 {
            "Stage changes to commit them"
        } else {
            "Commit the staged changes"
        };
        let write = if draft.busy || generating {
            let label = if generating { "✦ Writing…" } else { "✦" };
            disabled_button("git-commit-generate", label, theme, false)
        } else {
            quiet_button("git-commit-generate", "✦", theme, theme.surface_raised).on_click(
                cx.listener(move |shell, _, window, cx| {
                    shell.generate_commit_message(project_id, window, cx)
                }),
            )
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .child(
                text_field(
                    "git-commit-message",
                    draft.text.text(),
                    "Commit message",
                    focused,
                    draft.text.all_selected(),
                    theme,
                )
                .max_h(px(theme::GIT_COMMIT_BOX_MAX_HEIGHT))
                .overflow_y_scroll()
                .when(draft.busy || generating, |el| el.opacity(opacity::DISABLED))
                .tooltip(tooltip(
                    theme,
                    "Message of the next commit (Enter: new line)",
                    self.keymap.label(Command::GitFocusCommit),
                ))
                .track_focus(&self.git_commit_focus)
                .on_key_down(cx.listener(Shell::git_commit_key_down))
                .relative()
                .child(self.typing(&self.git_commit_focus, Shell::git_commit_key_down, cx))
                .on_click(cx.listener(|shell, _, window, cx| {
                    window.focus(&shell.git_commit_focus, cx);
                    cx.notify();
                })),
            )
            // Wrapped like the Fetch / Push error: hook output says what to fix.
            .children(draft.error.clone().map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text))
                    .child(format!("✕ {error}"))
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(meta_line(theme, summary.into()).flex_1().min_w_0())
                    .child(write.flex_none().tooltip(tooltip(
                        theme,
                        "Write the message with the workspace's default agent (Conventional Commits)",
                        "",
                    )))
                    .child(commit.tooltip(tooltip(theme, tip, "Ctrl+Enter"))),
            )
    }
}
