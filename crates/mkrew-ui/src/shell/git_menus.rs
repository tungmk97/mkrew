//! Git side panel popovers: the repository tabs and their menu, the file filter, "By: …"
//! (Changed by) and the diff toolbar's "⋯".

use crate::*;

impl Shell {
    /// Opens `menu` (closing any other popover). The Repositories menu starts with an empty
    /// search that has the keyboard and its first row selected.
    pub(crate) fn open_git_menu(
        &mut self,
        menu: GitMenu,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.git_menu = Some(menu);
        if menu == GitMenu::Repos {
            self.git_repo_query.clear();
            self.finish_menu(true, window, cx);
            window.focus(&self.git_repo_query_focus, cx);
        } else {
            self.finish_menu(window.last_input_was_keyboard(), window, cx);
        }
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

    /// "Repositories": the heading, then one row per repository matching the search, with its
    /// branch and Clean / N changes; Enter or a click shows it.
    pub(crate) fn git_repo_rows(&self, project_id: Ulid, active: &Option<String>) -> Rows {
        let repos = self
            .git_repos
            .get(&project_id)
            .map_or(&[][..], Vec::as_slice);
        let mut rows: Rows = vec![MenuRow::Heading("Repositories".into())];
        let listed = git_view::repo_choices(repos, self.workspace_name(project_id))
            .into_iter()
            .filter(|(_, label)| palette::fuzzy(&self.git_repo_query, label).is_some())
            .map(|(repo, label)| {
                let summary = self.git_summary().and_then(|summary| {
                    let name = repo.as_deref().unwrap_or("");
                    summary.repos.iter().find(|summary| summary.repo == name)
                });
                let branch = summary
                    .map(|summary| summary.branch.clone().unwrap_or_else(|| "detached".into()))
                    .unwrap_or_default();
                let state = match summary.map(RepoSummary::changed) {
                    Some(0) => "Clean".to_owned(),
                    Some(n) => format!("{n} changes"),
                    None => String::new(),
                };
                let shown = *active == repo;
                let item = MenuItem::new(
                    label.to_owned(),
                    act(move |shell, window, cx| {
                        shell.set_git_repo(project_id, repo.clone(), window, cx);
                    }),
                )
                .checked(shown)
                .detail(state);
                if branch.is_empty() {
                    item
                } else {
                    item.sub(branch)
                }
                .row()
            })
            .collect::<Vec<_>>();
        if listed.is_empty() {
            rows.push(MenuRow::Note("No repository matches.".into()));
        }
        rows.extend(listed);
        rows
    }

    fn render_git_repo_menu(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = self.git_repo_query_focus.is_focused(window);
        let search = caret_field(
            "git-repos-search",
            &self.git_repo_query,
            "Search repositories",
            focused,
            theme,
            cx,
            |shell, caret| {
                shell
                    .git_repo_query
                    .click(caret.at, caret.extend, caret.clicks);
            },
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
        }))
        .into_any_element();
        render_menu(
            self.menu_state(),
            "git-repos-panel",
            self.git_repo_rows(project_id, &view.repo),
            Some(search),
            Place::Dropdown,
            theme,
            cx,
        )
    }

    /// The Repositories search: typing filters (the first match is selected), ↑ / ↓ move the
    /// selection through the filtered rows, Enter shows the selected one, Esc clears the search
    /// (then closes the menu).
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
        let active = self.git.get(&project_id).and_then(|view| view.repo.clone());
        match key {
            quick_open::Key::Close if !self.git_repo_query.is_empty() => {
                self.git_repo_query.clear();
            }
            quick_open::Key::Close => return self.dismiss_menus(window, cx),
            quick_open::Key::Open => {
                let rows = self.git_repo_rows(project_id, &active);
                let action = self
                    .menu_selected
                    .and_then(|at| menu_model::action_of(&rows, at))
                    .cloned();
                self.dismiss_menus(window, cx);
                if let Some(action) = action {
                    action(self, 0, window, cx);
                }
                return;
            }
            quick_open::Key::Move(by) => {
                let rows = self.git_repo_rows(project_id, &active);
                self.menu_selected = menu_model::step(&rows, self.menu_selected, by as isize);
                return cx.notify();
            }
            quick_open::Key::Edit(edit) => {
                copy_selection(&edit, &self.git_repo_query, cx);
                self.git_repo_query.edit(&edit);
            }
            quick_open::Key::Paste => {
                self.git_repo_query
                    .insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                self.git_repo_query.insert(&text);
            }
        }
        let rows = self.git_repo_rows(project_id, &active);
        self.menu_selected = menu_model::first(&rows);
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
                caret_field(
                    "git-filter",
                    &view.filter,
                    "Filter files",
                    focused,
                    theme,
                    cx,
                    move |shell, caret| {
                        shell.git_view(project_id).filter.click(
                            caret.at,
                            caret.extend,
                            caret.clicks,
                        );
                    },
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
            quick_open::Key::Edit(edit) => {
                copy_selection(&edit, filter, cx);
                if !filter.edit(&edit) {
                    return cx.notify();
                }
            }
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
                    el.child(render_menu(
                        self.menu_state(),
                        "git-changed-by-panel",
                        self.git_changed_by_rows(project_id, view),
                        None,
                        Place::Dropdown,
                        theme,
                        cx,
                    ))
                }),
        )
    }

    /// The "By" menu: All sessions, then AGENTS (sessions named by their agent) and OTHER
    /// SESSIONS (named or ended ones); the shown choice is checked.
    pub(crate) fn git_changed_by_rows(&self, project_id: Ulid, view: &GitView) -> Rows {
        let filtered = view.changed_by.session;
        let sessions = self.sidebar.sessions();
        let item = |label: String, session: Option<Ulid>| {
            let tip = match session {
                None => "Every changed file".to_owned(),
                Some(_) => format!("Only the files {label} changed in the last 24 h"),
            };
            MenuItem::new(
                label,
                act(move |shell, window, cx| {
                    shell.filter_git_changed_by(project_id, session, window, cx);
                }),
            )
            .checked(filtered == session)
            .tip(tip)
            .row()
        };
        // An agent session without a name of its own is listed by its agent (`omp #1`).
        let (agents, others): (Vec<_>, Vec<_>) =
            view.changers(sessions).into_iter().partition(|changer| {
                sessions
                    .iter()
                    .find(|session| session.id == changer.session_id)
                    .is_some_and(|session| history::title(session).is_none())
            });
        let mut rows: Rows = vec![item("All sessions".into(), None)];
        for (heading, list) in [("AGENTS", agents), ("OTHER SESSIONS", others)] {
            if list.is_empty() {
                continue;
            }
            rows.push(MenuRow::Heading(heading.into()));
            rows.extend(
                list.into_iter()
                    .map(|changer| item(changer.label, Some(changer.session_id))),
            );
        }
        rows
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
