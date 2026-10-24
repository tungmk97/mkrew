//! Git side panel popovers: the repository tabs and their menu, the file filter, "By: …"
//! (Changed by) and the diff toolbar's "⋯".

use crate::*;

impl Shell {
    /// Opens `menu` (closing any other Git popover); the Repositories menu starts with an
    /// empty search that has the keyboard.
    pub(crate) fn open_git_menu(
        &mut self,
        menu: GitMenu,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.git_menu = Some(menu);
        if menu == GitMenu::Repos {
            self.git_repo_query.clear();
            window.focus(&self.git_repo_query_focus, cx);
        }
        cx.notify();
    }

    /// Closes the open Git popover; the list takes the keyboard back.
    pub(crate) fn close_git_menu(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.git_menu = None;
        window.focus(&self.git_focus, cx);
        cx.notify();
    }

    /// The repository tabs of a multi-repo workspace (ADR 0029): the shown one underlined in
    /// the accent, an amber dot on each with uncommitted changes, and a chevron opening the
    /// Repositories menu. When the tabs would not fit, only the shown one is a tab. Hidden
    /// with fewer than two repositories and while a task worktree is shown (one repository).
    pub(crate) fn git_repo_strip(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        if view.worktree_task_id.is_some() {
            return None;
        }
        let repos = self.git_repos.get(&project_id)?;
        let choices = git_view::repo_choices(repos, self.workspace_name(project_id));
        if choices.is_empty() {
            return None;
        }
        // ponytail: tab widths estimated from label length; measure the laid-out tabs if
        // labels in unusual fonts or scripts overflow.
        let wanted: f32 = choices
            .iter()
            .map(|(_, label)| {
                label.chars().count() as f32 * theme::GIT_REPO_TAB_CHAR_WIDTH + space::L * 2.0
            })
            .sum();
        let room = theme::GIT_FILE_LIST_WIDTH - space::M * 2.0 - theme::ICON_BUTTON_SIZE;
        let fits = wanted <= room;
        let hover = theme.text;
        let tabs = choices
            .into_iter()
            .enumerate()
            .filter(|(_, (repo, _))| fits || *repo == view.repo)
            .map(|(at, (repo, label))| {
                let active = view.repo == repo;
                let dirty = self.repo_changes(repo.as_deref()).is_some_and(|n| n > 0);
                let tip = match &repo {
                    Some(repo) => format!("The git repository in {repo}/"),
                    None => "The workspace folder's git repository".to_owned(),
                };
                div()
                    .id(SharedString::from(format!("git-repo-{at}")))
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .px(px(space::M))
                    .border_b(px(theme::PANE_ACCENT_WIDTH))
                    .border_color(if active {
                        rgb(theme.accent).into()
                    } else {
                        gpui::transparent_black()
                    })
                    .text_size(theme::GIT_REPO_TAB_TEXT)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .tooltip(tooltip(theme, &tip, ""))
                    .when(active, |el| el.text_color(rgb(theme.text)))
                    .when(!active, |el| {
                        el.cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.text_color(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.git_focus, cx);
                                shell.set_git_repo(project_id, repo.clone(), window, cx);
                            }))
                    })
                    .child(div().min_w_0().truncate().child(label.to_owned()))
                    .when(dirty, |el| el.child(changes_dot(theme)))
            });
        let open = self.git_menu == Some(GitMenu::Repos);
        Some(
            div()
                .flex_none()
                .h(theme::rems_px(theme::GIT_REPO_STRIP_HEIGHT))
                .flex()
                .items_center()
                .px(px(space::M))
                .border_b(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.divider))
                .child(div().flex_1().min_w_0().h_full().flex().children(tabs))
                .child(
                    div()
                        .flex_none()
                        .child(
                            icon_button("git-repos-menu", Icon::ChevronDown, theme)
                                .tooltip(tooltip(theme, "Repositories", ""))
                                // An open menu already closed on this mouse down (outside it):
                                // the click reopens it.
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.open_git_menu(GitMenu::Repos, window, cx);
                                })),
                        )
                        .when(open, |el| {
                            el.child(self.render_git_repo_menu(project_id, view, theme, window, cx))
                        }),
                ),
        )
    }

    /// Changed files of repository `repo` (`None` = the workspace folder's) from the title
    /// bar summary; `None` until it loaded.
    fn repo_changes(&self, repo: Option<&str>) -> Option<usize> {
        let repo = repo.unwrap_or("");
        self.git_summary()?
            .repos
            .iter()
            .find(|summary| summary.repo == repo)
            .map(RepoSummary::changed)
    }

    /// "Repositories": a search field, then one row per repository with its branch and
    /// Clean / N changes; a click shows it.
    fn render_git_repo_menu(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let query = &self.git_repo_query;
        let focused = self.git_repo_query_focus.is_focused(window);
        let repos = self
            .git_repos
            .get(&project_id)
            .map_or(&[][..], Vec::as_slice);
        let rows = git_view::repo_choices(repos, self.workspace_name(project_id))
            .into_iter()
            .enumerate()
            .filter(|(_, (_, label))| palette::fuzzy(query, label).is_some())
            .map(|(at, (repo, label))| {
                let active = view.repo == repo;
                let summary = self.git_summary().and_then(|summary| {
                    let name = repo.as_deref().unwrap_or("");
                    summary.repos.iter().find(|summary| summary.repo == name)
                });
                let branch = summary
                    .map(|summary| summary.branch.clone().unwrap_or_else(|| "detached".into()))
                    .unwrap_or_default();
                let (state, color) = match summary.map(RepoSummary::changed) {
                    Some(0) => ("Clean".to_owned(), theme.text_muted),
                    Some(n) => (format!("{n} changes"), theme.git_untracked),
                    None => (String::new(), theme.text_muted),
                };
                menu_item(SharedString::from(format!("git-repo-menu-{at}")), theme)
                    .child(
                        div()
                            .flex_none()
                            .w(px(space::M))
                            .text_color(rgb(theme.accent_text))
                            .child(if active { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_color(rgb(theme.text))
                                    .child(label.to_owned()),
                            )
                            .child(meta_line(theme, branch.into()).truncate()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::META)
                            .text_color(rgb(color))
                            .child(state),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.close_git_menu(window, cx);
                        shell.set_git_repo(project_id, repo.clone(), window, cx);
                    }))
            })
            .collect::<Vec<_>>();
        let panel = menu_panel("git-repos-panel", theme)
            .gap(px(space::XS))
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.git_menu = None;
                cx.notify();
            }))
            .child(
                text_field(
                    "git-repos-search",
                    query,
                    "Search repositories",
                    focused,
                    query.all_selected(),
                    theme,
                )
                .track_focus(&self.git_repo_query_focus)
                .on_key_down(cx.listener(Self::git_repo_query_key_down))
                .relative()
                .child(self.typing(
                    &self.git_repo_query_focus,
                    Self::git_repo_query_key_down,
                    cx,
                ))
                .on_click(cx.listener(|shell, _, window, cx| {
                    window.focus(&shell.git_repo_query_focus, cx);
                    cx.notify();
                })),
            )
            .child(
                div()
                    .px(px(space::S))
                    .pt(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("Repositories"),
            )
            .when(rows.is_empty(), |el| {
                el.child(meta_line(theme, "No repository matches.".into()).px(px(space::S)))
            })
            .children(rows);
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }

    /// The Repositories search: typing filters, Enter shows the first match, Esc clears the
    /// search (then closes the menu).
    pub(crate) fn git_repo_query_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(project_id), Some(key)) = (self.workspace, quick_open::key(&event.keystroke))
        else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let query = &mut self.git_repo_query;
        match key {
            quick_open::Key::Close if !query.is_empty() => query.clear(),
            quick_open::Key::Close => return self.close_git_menu(window, cx),
            quick_open::Key::Open => {
                let repos = self
                    .git_repos
                    .get(&project_id)
                    .map_or(&[][..], Vec::as_slice);
                let first = git_view::repo_choices(repos, self.workspace_name(project_id))
                    .into_iter()
                    .find(|(_, label)| palette::fuzzy(&self.git_repo_query, label).is_some())
                    .map(|(repo, _)| repo);
                self.close_git_menu(window, cx);
                if let Some(repo) = first {
                    self.set_git_repo(project_id, repo, window, cx);
                }
                return;
            }
            quick_open::Key::Move(_) => {}
            quick_open::Key::Backspace => {
                query.backspace();
            }
            quick_open::Key::DeleteWord => {
                query.delete_word();
            }
            quick_open::Key::SelectAll => query.select_all(),
            quick_open::Key::Paste => {
                query.insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                query.insert(&text);
            }
        }
        cx.notify();
    }

    /// The "Filter files" field (30 px) and, while sessions changed files, the ghost "All ▾".
    pub(crate) fn git_filter_row(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let focused = self.git_filter_focus.is_focused(window);
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::M))
            .pb(px(space::S))
            .child(
                text_field(
                    "git-filter",
                    &view.filter,
                    "Filter files",
                    focused,
                    view.filter.all_selected(),
                    theme,
                )
                .flex_1()
                .min_w_0()
                .h(theme::rems_px(theme::CONTROL_HEIGHT))
                .py_0()
                .flex()
                .items_center()
                .track_focus(&self.git_filter_focus)
                .on_key_down(cx.listener(Self::git_filter_key_down))
                .relative()
                .child(self.typing(&self.git_filter_focus, Self::git_filter_key_down, cx))
                .tooltip(tooltip(
                    theme,
                    "Filter the listed files (Esc clears, ↑ ↓ move)",
                    "",
                ))
                .on_click(cx.listener(|shell, _, window, cx| {
                    window.focus(&shell.git_filter_focus, cx);
                    cx.notify();
                })),
            )
            .children(self.git_changed_by(project_id, view, theme, cx))
    }

    /// The Filter files field: typing filters the list, ↑ / ↓ move between the shown files,
    /// Esc clears the filter (then leaves the field), Enter leaves it.
    pub(crate) fn git_filter_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(project_id), Some(key)) = (self.workspace, quick_open::key(&event.keystroke))
        else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let filter = &mut self.git_view(project_id).filter;
        match key {
            quick_open::Key::Close if !filter.is_empty() => filter.clear(),
            quick_open::Key::Close | quick_open::Key::Open => {
                window.focus(&self.git_focus, cx);
            }
            quick_open::Key::Move(by) => {
                let key = if by < 0 {
                    git_view::Key::Up
                } else {
                    git_view::Key::Down
                };
                return self.git_key(key, window, cx);
            }
            quick_open::Key::Backspace => {
                filter.backspace();
            }
            quick_open::Key::DeleteWord => {
                filter.delete_word();
            }
            quick_open::Key::SelectAll => filter.select_all(),
            quick_open::Key::Paste => {
                filter.insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                filter.insert(&text);
            }
        }
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// "All ▾" (SPEC §20C.4): keeps only the files a session changed in the last 24 h;
    /// hidden while no session changed any (and in the History).
    fn git_changed_by(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let sessions = self.sidebar.sessions();
        let changers = view.changers(sessions);
        if changers.is_empty() {
            return None;
        }
        let shown = view
            .changed_by_label(sessions)
            .unwrap_or_else(|| "All".to_owned());
        let open = self.git_menu == Some(GitMenu::ChangedBy);
        Some(
            div()
                .flex_none()
                .child(
                    quiet_button(
                        "git-changed-by",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::XS))
                            .min_w_0()
                            .child(div().min_w_0().truncate().child(shown))
                            .child(icon_svg(
                                Icon::ChevronDown,
                                theme::ICON_SIZE_SMALL,
                                theme.text_muted,
                            )),
                        theme,
                        theme.hover,
                    )
                    .max_w(px(theme::GIT_CHANGED_BY_MAX_WIDTH))
                    .truncate()
                    .text_size(font_size::META)
                    .tooltip(tooltip(
                        theme,
                        "Only the files a session changed in the last 24 h",
                        "",
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.open_git_menu(GitMenu::ChangedBy, window, cx);
                    })),
                )
                .when(open, |el| {
                    el.child(self.render_git_changed_by_menu(project_id, view, changers, theme, cx))
                }),
        )
    }

    /// The "By" menu: All sessions, then AGENTS (sessions named by their agent) and OTHER
    /// SESSIONS (named or ended ones); the shown choice is checked.
    fn render_git_changed_by_menu(
        &self,
        project_id: Ulid,
        view: &GitView,
        changers: Vec<git_view::Changer>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let filtered = view.changed_by.session;
        let sessions = self.sidebar.sessions();
        let item = |id: SharedString, label: String, session: Option<Ulid>| {
            let tip = match session {
                None => "Every changed file".to_owned(),
                Some(_) => format!("Only the files {label} changed in the last 24 h"),
            };
            menu_item(id, theme)
                .tooltip(tooltip(theme, &tip, ""))
                .child(
                    div()
                        .flex_none()
                        .w(px(space::M))
                        .text_color(rgb(theme.accent_text))
                        .child(if filtered == session { "✓" } else { "" }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text))
                        .child(label),
                )
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.close_git_menu(window, cx);
                    shell.filter_git_changed_by(project_id, session, window, cx);
                }))
        };
        let heading = |text: &'static str| {
            div()
                .px(px(space::S))
                .pt(px(space::S))
                .pb(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        // An agent session without a name of its own is listed by its agent (`omp #1`).
        let (agents, others): (Vec<_>, Vec<_>) = changers.into_iter().partition(|changer| {
            sessions
                .iter()
                .find(|session| session.id == changer.session_id)
                .is_some_and(|session| history::title(session).is_none())
        });
        let rows = |changers: Vec<git_view::Changer>| {
            changers
                .into_iter()
                .map(|changer| {
                    let id = SharedString::from(format!("git-changed-by-{}", changer.session_id));
                    item(id, changer.label, Some(changer.session_id))
                })
                .collect::<Vec<_>>()
        };
        let panel = menu_panel("git-changed-by-panel", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.git_menu = None;
                cx.notify();
            }))
            .child(item(
                "git-changed-by-all".into(),
                "All sessions".into(),
                None,
            ))
            .when(!agents.is_empty(), |el| {
                el.child(heading("AGENTS")).children(rows(agents))
            })
            .when(!others.is_empty(), |el| {
                el.child(heading("OTHER SESSIONS")).children(rows(others))
            });
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }
}

/// The attention-colored dot of a repository with uncommitted changes.
pub(crate) fn changes_dot(theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .size(px(theme::FILE_CHANGES_DOT_SIZE))
        .rounded_full()
        .bg(rgb(theme.status_needs_input))
}
