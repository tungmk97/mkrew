//! Git view rendering: the side panel (header, tabs, list, commit box) and its empty states.

use crate::*;

impl Shell {
    /// The workspace's changes (SPEC §12B): the selected file's diff, and on its right the
    /// side panel: repository tabs, branch header, Working / Vs. base / History, the file
    /// filter, the changed-file list and the commit box.
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
        let rows = view.list_rows();
        // Something is listed: a file, a commit, or a group header that counts files.
        let any_files = rows.iter().any(|row| match row {
            git_view::ListRow::File(..) | git_view::ListRow::Commit { .. } => true,
            git_view::ListRow::Header(_, count) => *count > 0,
            git_view::ListRow::Note(_) | git_view::ListRow::Hint(_) => false,
        });
        // No uncommitted changes at all (nothing filtered away): the clean state hides the
        // filter row and the commit box.
        let clean = status.is_some()
            && !any_files
            && view.section() == git_view::Section::Working
            && view.changed_by.session.is_none()
            && view.filter.is_empty();
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
            (None, git_view::Status::Loaded(_)) if !any_files && !view.filter.is_empty() => {
                empty(format!("No files match “{}”.", &*view.filter), None)
                    .child(
                        button("git-filter-clear", "Clear filter", theme, true).on_click(
                            cx.listener(move |shell, _, window, cx| {
                                shell.git_view(project_id).filter.clear();
                                window.focus(&shell.git_focus, cx);
                                cx.notify();
                            }),
                        ),
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
            (None, git_view::Status::Loaded(status)) if clean => {
                self.git_clean_state(project_id, view, status, theme, cx)
            }
            (None, git_view::Status::Loaded(status)) if !any_files => {
                self.git_no_changes(project_id, view, status, theme, cx)
            }
            (None, git_view::Status::Loaded(_)) => {
                self.git_list(project_id, view, rows, theme, window, cx)
            }
        };
        let column = div()
            .id("git-files")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .pb(px(space::S))
            .track_focus(&self.git_focus)
            .key_context(GIT_CONTEXT)
            .on_key_down(cx.listener(Self::git_key_down))
            .child(list);
        div()
            .flex_1()
            .min_h_0()
            .flex()
            // The side panel stays at the right edge even when no diff is shown.
            .justify_end()
            .font_family(theme.ui_font_family.clone())
            .when(any_files, |el| {
                el.child(self.render_git_diff(project_id, view, theme, window, cx))
            })
            .child(
                // The header, filter and commit box sit outside the `GIT_CONTEXT` column:
                // typing in their fields never stages.
                div()
                    .flex_none()
                    .w(px(theme::GIT_FILE_LIST_WIDTH))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.git_header(project_id, view, status, theme, window, cx))
                    .when(!clean, |el| {
                        el.child(self.git_filter_row(project_id, view, theme, window, cx))
                    })
                    .child(column)
                    .when_some(status.filter(|_| view.commit_box_shown()), |el, status| {
                        el.child(self.git_commit_box(project_id, view, status, theme, window, cx))
                    }),
            )
            .into_any_element()
    }

    /// The workspace's name (the root repository's tab label).
    pub(crate) fn workspace_name(&self, project_id: Ulid) -> &str {
        self.sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or("Workspace", |p| p.name.as_str())
    }

    /// The task worktree chooser, the repository tabs, the branch header, then the Working /
    /// Vs. base / History switch and (on Vs. base) the base it compares to.
    pub(crate) fn git_header(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: Option<&GitStatus>,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let chooser = self
            .target_chooser(project_id, view.worktree_task_id, true, theme, cx)
            .map(|chooser| chooser.px(px(space::M)).pt(px(space::S)));
        // Wrapped, not cut: git's push / fetch errors say what to do.
        let error = |text: String| {
            div()
                .px(px(space::M))
                .text_size(font_size::META)
                .text_color(rgb(theme.text))
                .child(text)
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .pb(px(space::S))
            .children(chooser)
            .children(self.git_repo_strip(project_id, view, theme, window, cx))
            .child(self.git_branch_header(project_id, view, status, theme, cx))
            .children(
                view.remotes
                    .error
                    .clone()
                    .map(|text| error(format!("✕ {text}"))),
            )
            .child(self.git_tabs(project_id, view, status, theme, cx))
            .children(self.git_base_line(project_id, view, status, theme, cx))
            .children(git_viewed_line(view, theme))
            .children(
                view.refresh_error
                    .clone()
                    .map(|text| error(format!("✕ Refresh failed: {text}"))),
            )
    }

    /// The branch card (`card` fill, 1 px border): a branch icon, the branch name over the
    /// upstream it tracks, Fetch as an icon button and Push.
    fn git_branch_header(
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
        div()
            .mx(px(space::M))
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::M))
            .py(px(space::S))
            .rounded(px(radius::CARD))
            .bg(rgb(theme.card))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .child(icon_svg(
                Icon::GitBranch,
                theme::ICON_SIZE,
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .text_size(font_size::BODY)
                            .font_weight(FontWeight(weight::SEMIBOLD))
                            .text_color(rgb(theme.text))
                            .child(branch),
                    )
                    .children(status.map(|status| {
                        div()
                            .truncate()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_subtle))
                            .child(git_view::tracking(status))
                    })),
            )
            .child(self.git_fetch_button(project_id, view, theme, cx))
            .child(self.git_push_button(project_id, view, status, theme, cx))
    }

    /// Fetch as an icon: "⋯" while it runs, faded while Push runs.
    fn git_fetch_button(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let running = view.remotes.running;
        let tip = "Fetch from the remotes (refreshes the base)";
        match running {
            Some(Remote::Fetch) => icon_button("git-fetch", Icon::Ellipsis, theme)
                .tooltip(tooltip(theme, Remote::Fetch.running_label(), ""))
                .into_any_element(),
            Some(Remote::Push) => icon_button("git-fetch", Icon::Download, theme)
                .opacity(opacity::DISABLED)
                .tooltip(tooltip(theme, tip, ""))
                .into_any_element(),
            None => icon_button("git-fetch", Icon::Download, theme)
                .tooltip(tooltip(theme, tip, ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.run_git_remote(project_id, Remote::Fetch, window, cx);
                }))
                .into_any_element(),
        }
    }

    /// "Push N" (N commits ahead): muted with nothing to push but still clickable; faded
    /// without a branch and upstream or while Fetch runs; "Pushing…" while it runs.
    fn git_push_button(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: Option<&GitStatus>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let running = view.remotes.running;
        let no_upstream = status.is_none_or(|s| s.branch.is_none() || s.upstream.is_none());
        let ahead = status.map_or(0, |status| status.ahead);
        let label = match running {
            Some(Remote::Push) => Remote::Push.running_label().to_owned(),
            _ if ahead > 0 => format!("Push {ahead}"),
            _ => "Push".to_owned(),
        };
        let tip = if no_upstream {
            "This branch has no upstream to push to"
        } else {
            "Push this branch to its upstream (never forced)"
        };
        if running.is_some() || no_upstream {
            return button_frame("git-push", icon_text(Icon::Upload, label, theme.text_muted))
                .flex_none()
                .cursor_default()
                .opacity(opacity::DISABLED)
                .text_color(rgb(theme.text_muted))
                .tooltip(tooltip(theme, tip, ""))
                .into_any_element();
        }
        let push = secondary_button(
            "git-push",
            icon_text(
                Icon::Upload,
                label,
                if ahead > 0 {
                    theme.text
                } else {
                    theme.text_muted
                },
            ),
            theme,
        );
        push.flex_none()
            .tooltip(tooltip(theme, tip, ""))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.run_git_remote(project_id, Remote::Push, window, cx);
            }))
            .into_any_element()
    }

    /// Working N / Vs. base / History: three equal segments (N = changed files).
    fn git_tabs(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: Option<&GitStatus>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let toggle_keys = SharedString::from(self.keymap.label(Command::GitToggleMode));
        let history_keys = SharedString::from(self.keymap.label(Command::GitToggleHistory));
        let shown = view.section();
        let working = match self.git_working_count(view, status) {
            Some(count) => format!("Working {count}"),
            None => "Working".to_owned(),
        };
        let tab =
            |section: git_view::Section, id: &'static str, label: String, tip: &'static str| {
                let active = shown == section;
                let keys = if section == git_view::Section::History {
                    history_keys.clone()
                } else {
                    toggle_keys.clone()
                };
                segment(id, active, theme)
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .px(px(space::S))
                    .when(!active, |el| {
                        el.on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_git_section(project_id, section, window, cx);
                        }))
                    })
                    .tooltip(tooltip(theme, tip, keys))
                    .child(label)
            };
        segmented(theme)
            .mx(px(space::M))
            .child(tab(
                git_view::Section::Working,
                "git-mode-working",
                working,
                "Uncommitted changes: staged, unstaged, untracked",
            ))
            .child(tab(
                git_view::Section::Base,
                "git-mode-base",
                "Vs. base".to_owned(),
                "Everything on this branch since it left the base",
            ))
            .child(tab(
                git_view::Section::History,
                "git-mode-history",
                "History".to_owned(),
                "The last 50 commits and their changes",
            ))
    }

    /// The changed files of the working tree for the Working tab: from the shown status in
    /// Working changes, else from the title bar summary of the shown repository.
    fn git_working_count(&self, view: &GitView, status: Option<&GitStatus>) -> Option<usize> {
        if view.section() == git_view::Section::Working {
            return status.map(|status| RepoSummary::of("", status).changed());
        }
        if view.worktree_task_id.is_some() {
            return None;
        }
        let repo = view.repo.as_deref().unwrap_or("");
        self.git_summary()?
            .repos
            .iter()
            .find(|summary| summary.repo == repo)
            .map(RepoSummary::changed)
    }

    /// "Base: X (auto-detected) · Change…" on the Vs. base tab; a task's or turn's changes
    /// say what they compare instead, with Close back to the working changes.
    fn git_base_line(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: Option<&GitStatus>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let section = view.section();
        let label = match (section, &view.since) {
            (git_view::Section::Since, Some(since)) => since.label.clone(),
            (git_view::Section::Base, _) => {
                status.map(|status| match (&status.base_ref, status.base_auto) {
                    (Some(base), true) => format!("Base: {base} (auto-detected)"),
                    (Some(base), false) => format!("Base: {base}"),
                    (None, _) => "No base branch".to_owned(),
                })?
            }
            _ => return None,
        };
        let action = if section == git_view::Section::Since {
            row_action("git-task-close".into(), "Close", theme)
                .tooltip(tooltip(
                    theme,
                    "Back to the working changes",
                    self.keymap.label(Command::GitToggleMode),
                ))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    let working = git_view::Section::Working;
                    shell.show_git_section(project_id, working, window, cx);
                }))
        } else {
            let label = if status.is_some_and(|status| status.base_ref.is_some()) {
                "Change…"
            } else {
                "Set base…"
            };
            row_action("git-base-change".into(), label, theme)
                .tooltip(tooltip(theme, "Choose the branch to compare with", ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.open_git_base(project_id, window, cx);
                }))
        };
        Some(
            div()
                .px(px(space::M))
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(meta_line(theme, label.into()).flex_1().min_w_0().truncate())
                .child(action),
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

    /// No uncommitted changes in Working: "Working tree clean", Review against the base and
    /// Open history; in a multi-repo workspace, a card for another repository with changes.
    fn git_clean_state(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: &GitStatus,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let workspace = self.workspace_name(project_id);
        let repo = view.repo.as_deref().unwrap_or(workspace);
        let review = match &status.base_ref {
            Some(base) => format!("Review against {base}"),
            None => "Review against base".to_owned(),
        };
        // Another repository of the workspace (never while a task worktree is shown).
        let dirty = self
            .git_summary()
            .filter(|_| view.worktree_task_id.is_none())
            .and_then(|summary| {
                let shown = view.repo.as_deref().unwrap_or("");
                summary
                    .repos
                    .iter()
                    .find(|other| other.repo != shown && other.changed() > 0)
            })
            .map(|other| other.repo.clone());
        let card = dirty.map(|other| {
            let label = if other.is_empty() {
                workspace.to_owned()
            } else {
                other.clone()
            };
            div()
                .mt(px(space::S))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::M))
                .py(px(space::S))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border))
                .text_size(font_size::META)
                .child(changes_dot(theme))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(rgb(theme.text_dim))
                        .child(format!("{label} has uncommitted changes")),
                )
                .child(
                    row_action("git-clean-switch".into(), "Switch", theme)
                        .text_color(rgb(theme.accent_text))
                        .tooltip(tooltip(theme, "Show this repository's changes", ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            window.focus(&shell.git_focus, cx);
                            let repo = (!other.is_empty()).then(|| other.clone());
                            shell.set_git_repo(project_id, repo, window, cx);
                        })),
                )
        });
        empty_state(
            theme,
            Some("✓"),
            Some("Working tree clean"),
            format!("No uncommitted changes in {repo}.").into(),
            None,
        )
        .flex_1()
        .child(
            div()
                .flex()
                .flex_wrap()
                .justify_center()
                .gap(px(space::S))
                .child(
                    button("git-empty-mode", review, theme, true)
                        .tooltip(tooltip(
                            theme,
                            "Show everything on this branch",
                            self.keymap.label(Command::GitToggleMode),
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            window.focus(&shell.git_focus, cx);
                            let requests = shell.git_view(project_id).toggle_mode();
                            shell.git_requests(project_id, requests, window, cx);
                        })),
                )
                .child(
                    secondary_button("git-empty-history", "Open history", theme)
                        .tooltip(tooltip(
                            theme,
                            "The last 50 commits and their changes",
                            self.keymap.label(Command::GitToggleHistory),
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            window.focus(&shell.git_focus, cx);
                            let requests = shell.git_view(project_id).toggle_history();
                            shell.git_requests(project_id, requests, window, cx);
                        })),
                ),
        )
        .children(card)
        .into_any_element()
    }

    /// Nothing listed on the Vs. base tab or in a task's / turn's changes: why, with the
    /// way back (or, without a usable base, Set base… first).
    fn git_no_changes(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: &GitStatus,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
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
            (None, GitMode::Base, Some(base)) => (format!("No changes against {base}."), None),
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
        message(theme, None, sentence.into(), detail.map(Into::into))
            .flex_1()
            .child(buttons)
            .into_any_element()
    }

    /// The commit box pinned under the list (Working changes): the message (Enter breaks a
    /// line, Ctrl+Enter or Commit commits the staged changes); under it the summary line's
    /// length over what is staged, ✦ Generate (accent) and Commit (muted while it cannot go).
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
            n => format!("{n} staged"),
        };
        // Nothing staged but changes to stage: the button stages them all, then commits.
        let stage_all = staged == 0 && view.stage_all().is_some();
        let label = if draft.busy {
            "Committing…"
        } else if stage_all {
            "Stage all & commit"
        } else {
            "Commit"
        };
        let tip = if stage_all {
            "Stage every change, then commit (Ctrl+Enter twice)"
        } else if staged == 0 {
            "Stage changes to commit them"
        } else {
            "Commit the staged changes"
        };
        let commit = if draft.busy || generating || (staged == 0 && !stage_all) {
            disabled_button("git-commit", label, theme, true)
        } else {
            button("git-commit", label, theme, true).on_click(cx.listener(
                move |shell, _, window, cx| {
                    // The click is the confirmation the keyboard gives with a second Ctrl+Enter.
                    if stage_all {
                        shell.git_view(project_id).commit.stage_all_offered = true;
                    }
                    shell.commit_git(project_id, window, cx);
                },
            ))
        };
        let write = if draft.busy || generating {
            let label = if generating { "Writing…" } else { "Generate" };
            disabled_button(
                "git-commit-generate",
                icon_text(Icon::Sparkles, label, theme.text_muted),
                theme,
                false,
            )
        } else {
            quiet_button(
                "git-commit-generate",
                icon_text(Icon::Sparkles, "Generate", theme.accent_text),
                theme,
                theme.hover,
            )
            .text_color(rgb(theme.accent_text))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.generate_commit_message(project_id, window, cx)
            }))
        };
        let text = draft.text.text();
        let summary_len = text.lines().next().unwrap_or("").chars().count();
        let too_long = summary_len > git_actions::SUMMARY_LIMIT;
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .child(
                caret_field(
                    "git-commit-message",
                    draft.text.input(),
                    "Summary, then a blank line and details",
                    focused,
                    theme,
                    cx,
                    move |shell, caret| {
                        let text = &mut shell.git_view(project_id).commit.text;
                        text.click(caret.at, caret.extend, caret.clicks);
                    },
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_subtle))
                            .child(
                                div()
                                    .id("git-commit-length")
                                    .when(too_long, |el| el.text_color(rgb(theme.danger_text)))
                                    .tooltip(tooltip(
                                        theme,
                                        "Length of the summary line (keep it under 72)",
                                        "",
                                    ))
                                    .child(format!(
                                        "{summary_len} / {}",
                                        git_actions::SUMMARY_LIMIT
                                    )),
                            )
                            .child(div().truncate().child(summary)),
                    )
                    .child(write.flex_none().tooltip(tooltip(
                        theme,
                        "Write the message with the workspace's default agent (Conventional Commits)",
                        "",
                    )))
                    .child(commit.flex_none().tooltip(tooltip(theme, tip, "Ctrl+Enter"))),
            )
    }
}
