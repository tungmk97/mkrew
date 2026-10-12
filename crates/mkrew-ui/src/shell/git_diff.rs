//! Git diff rendering: the toolbar, unified and split rows (wrapped or not), hunk headers and
//! the bar under picked lines.

use crate::*;

impl Shell {
    /// The selected file's diff: the toolbar (file nav, status, path, counts, layout, actions),
    /// then the diff (or why it is not shown), with the comment margin on its right.
    pub(crate) fn render_git_diff(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let panel = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .ml(px(space::S))
            .mb(px(space::S))
            .rounded(px(radius::MEDIUM))
            .overflow_hidden()
            .bg(rgb(theme.surface));
        let Some(selected) = view.selected.clone() else {
            let history = view.history.as_ref();
            let (sentence, detail) = match history.map(|history| &history.detail) {
                Some(git_history::Detail::Loading) => ("Loading this commit's files…", None),
                Some(git_history::Detail::Loaded(_)) => ("This commit changed no files.", None),
                Some(git_history::Detail::Failed(error)) => {
                    ("Cannot list this commit's files.", Some(error.clone()))
                }
                Some(git_history::Detail::None) => (
                    "Select a commit to see its files.",
                    Some("Use ↑ ↓ (or j k) to move between commits.".to_owned()),
                ),
                None => (
                    "Select a file to see its changes.",
                    Some("Use ↑ ↓ (or j k) to move between files.".to_owned()),
                ),
            };
            let retry = history
                .filter(|history| matches!(history.detail, git_history::Detail::Failed(_)))
                .and_then(|history| history.selected.clone());
            return panel.child(
                message(theme, None, sentence.into(), detail.map(Into::into))
                    .flex_1()
                    .children(retry.map(|hash| {
                        button("git-show-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| {
                                shell.select_git_commit(project_id, hash.clone(), window, cx);
                            },
                        ))
                    })),
            );
        };
        let toolbar = self.git_diff_toolbar(project_id, view, &selected, theme, cx);
        let body = match &view.diff {
            git_view::Diff::None | git_view::Diff::Loading => {
                message(theme, None, "Loading…".into(), None)
                    .flex_1()
                    .into_any_element()
            }
            git_view::Diff::Failed(error) => message(
                theme,
                None,
                "Cannot show this diff.".into(),
                Some(error.clone().into()),
            )
            .flex_1()
            .child(
                button("git-diff-retry", "Retry", theme, true).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        let diff = shell.git_view(project_id).select(selected.clone());
                        let requests = git_view::Requests {
                            diff,
                            ..git_view::Requests::default()
                        };
                        shell.git_requests(project_id, requests, window, cx);
                    },
                )),
            )
            .into_any_element(),
            git_view::Diff::Shown(shown) if shown.diff.binary || shown.diff.too_large => {
                let sentence = if shown.diff.binary {
                    "This is a binary file."
                } else {
                    "This diff is too large to show here."
                };
                message(
                    theme,
                    None,
                    sentence.into(),
                    Some("Open it in Zed instead.".into()),
                )
                .flex_1()
                .child(self.zed_button("git-zed-notice", true, project_id, theme, cx))
                .into_any_element()
            }
            git_view::Diff::Shown(shown) if shown.rows.is_empty() => message(
                theme,
                None,
                "No line changes to show.".into(),
                Some("Only the file's mode or name changed.".into()),
            )
            .flex_1()
            .into_any_element(),
            git_view::Diff::Shown(shown) => {
                self.git_diff_lines(project_id, view, shown, theme, window, cx)
            }
        };
        panel
            .child(toolbar)
            .children(view.zed_error.clone().map(|error| {
                notice(theme.text, format!("✕ {error}"))
                    .px(px(space::L))
                    .py(px(space::S))
            }))
            .children(view.stage_error.clone().map(|error| {
                notice(theme.text, format!("✕ Staging failed: {error}"))
                    .px(px(space::L))
                    .py(px(space::S))
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    // Thread cards never touch the tinted diff rows.
                    .gap(px(space::M))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(body),
                    )
                    .children(view.comments_shown().then(|| {
                        self.render_comments(project_id, &view.comments, theme, window, cx)
                    })),
            )
    }

    /// "Open in Zed" for the selected file (a failure shows under the toolbar).
    fn zed_button(
        &self,
        id: &'static str,
        primary: bool,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        button(
            id,
            icon_text(Icon::ExternalLink, "Open in Zed", button_fg(theme, primary)),
            theme,
            primary,
        )
        .flex_none()
        .tooltip(tooltip(
            theme,
            "Open this file in Zed",
            self.keymap.label(Command::OpenInZed),
        ))
        .on_click(cx.listener(move |shell, _, _, cx| {
            shell.open_git_file_in_zed(project_id, cx);
        }))
    }

    /// The diff toolbar (wraps when narrow): previous / next file with "1 / 6", the status
    /// badge, `folder / name`, +/− counts and the current hunk; then Unified | Split and Wrap,
    /// Open in Zed, Discard…, Stage file and "⋯" (comment, send to agent).
    fn git_diff_toolbar(
        &self,
        project_id: Ulid,
        view: &GitView,
        selected: &git_view::Target,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let shown = match &view.diff {
            git_view::Diff::Shown(shown) => Some(&**shown),
            _ => None,
        };
        let position = view.file_position();
        let nav = |id: &'static str, glyph: Icon, tip: &'static str, keys, key| {
            icon_button(id, glyph, theme)
                .tooltip(tooltip(theme, tip, keys))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    window.focus(&shell.git_focus, cx);
                    shell.git_key(key, window, cx);
                }))
        };
        let file_nav = div()
            .flex_none()
            .flex()
            .items_center()
            .child(nav(
                "git-file-previous",
                Icon::ChevronLeft,
                "Previous file",
                "↑ / k",
                git_view::Key::Up,
            ))
            .child(nav(
                "git-file-next",
                Icon::ChevronRight,
                "Next file",
                "↓ / j",
                git_view::Key::Down,
            ))
            .children(position.map(|(at, count, _)| {
                div()
                    .pl(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{at} / {count}"))
            }));
        let badge = position.map(|(_, _, kind)| {
            let (letter, color, meaning) = change_letter(kind, theme);
            div()
                .id("git-diff-status")
                .flex_none()
                .px(px(space::XS))
                .rounded(px(radius::SMALL))
                .bg(rgb(theme.surface_raised))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(color))
                .tooltip(tooltip(theme, meaning, ""))
                .child(letter)
        });
        let (folder, name) = match selected.path.rsplit_once('/') {
            Some((folder, name)) => (Some(folder), name),
            None => (None, selected.path.as_str()),
        };
        let path = div()
            .id("git-diff-path")
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .text_size(font_size::BODY)
            .tooltip(tooltip(theme, &selected.path, ""))
            .children(folder.map(|folder| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{folder} /"))
            }))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(name.to_owned()),
            );
        let renamed = shown
            .and_then(|shown| shown.diff.old_path.clone())
            .map(|old| meta_line(theme, format!("from {old}").into()).flex_none());
        let counts = shown
            .filter(|shown| !shown.diff.binary)
            .map(|shown| change_counts(shown.diff.added, shown.diff.removed, theme));
        let hunk_label = shown.and_then(|shown| {
            let count = shown.diff.hunks.len();
            let label = match view.hunk {
                Some(hunk) => format!("Hunk {} of {count}", hunk + 1),
                None if count > 1 => format!("{count} hunks"),
                None => return None,
            };
            let keys = format!(
                "{} / {}",
                self.keymap.label(Command::GitNextHunk),
                self.keymap.label(Command::GitPreviousHunk)
            );
            Some(
                div()
                    .id("git-hunks")
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, "Next / previous hunk", keys))
                    .child(label),
            )
        });
        // Unified | Split and Wrap (global, remembered): only for a diff with lines to lay out.
        let has_lines = shown.is_some_and(|shown| {
            !shown.diff.binary && !shown.diff.too_large && !shown.rows.is_empty()
        });
        let layout = has_lines.then(|| {
            let wrap = self.diff_wrap;
            let hover = theme.surface_raised;
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .child(self.mode_toggle(
                    "git-diff-mode",
                    self.diff_mode,
                    [(DiffMode::Unified, "Unified"), (DiffMode::Split, "Split")],
                    "Unified (one column) or Split (old | new) diff",
                    self.keymap.label(Command::GitToggleDiffMode).into(),
                    |shell, mode, window, cx| {
                        window.focus(&shell.git_focus, cx);
                        shell.set_diff_mode(mode, cx);
                    },
                    theme,
                    cx,
                ))
                .child(
                    div()
                        .id("git-diff-wrap")
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .text_size(font_size::BODY)
                        .map(|el| {
                            if wrap {
                                el.bg(rgb(theme.surface_raised))
                                    .font_weight(FontWeight(weight::MEDIUM))
                                    .text_color(rgb(theme.text))
                            } else {
                                el.text_color(rgb(theme.text_muted))
                                    .hover(move |style| style.bg(rgb(hover)))
                            }
                        })
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(
                            theme,
                            if wrap {
                                "Long lines wrap (click to cut them at the edge)"
                            } else {
                                "Long lines are cut at the edge (click to wrap them)"
                            },
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.set_diff_wrap(!wrap, cx);
                        }))
                        .child("Wrap"),
                )
        });
        let discard = view.discard(selected).map(|_| {
            let target = selected.clone();
            let red = theme.diff_del_text;
            quiet_button(
                "git-discard",
                icon_text(Icon::Undo2, "Discard…", red),
                theme,
                theme.hover,
            )
            .flex_none()
            .text_color(rgb(red))
            .tooltip(tooltip(
                theme,
                "Discard this file's changes (asks first)",
                "",
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_git_discard(project_id, target.clone(), window, cx);
            }))
        });
        let stage = view.file_change(selected).map(|change| {
            let (label, tip, command) = match change.stage {
                git_view::Stage::Stage => ("Stage file", "Stage the whole file", Command::GitStage),
                git_view::Stage::Unstage => (
                    "Unstage file",
                    "Unstage the whole file",
                    Command::GitUnstage,
                ),
            };
            button("git-stage-file", label, theme, true)
                .flex_none()
                .tooltip(tooltip(theme, tip, self.keymap.label(command)))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    window.focus(&shell.git_focus, cx);
                    shell.stage_git(project_id, change.clone(), window, cx);
                }))
        });
        div()
            .flex_none()
            .min_h(theme::rems_px(theme::GIT_DIFF_TOOLBAR_HEIGHT))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(space::S))
            .gap_y(px(space::XS))
            .px(px(space::M))
            .py(px(space::XS))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .child(file_nav)
            .children(badge)
            .child(path)
            .children(renamed)
            .children(counts)
            .children(hunk_label)
            .child(div().flex_1())
            .children(layout)
            .child(
                div()
                    .flex_none()
                    .w(px(theme::BORDER_WIDTH))
                    .h(px(space::L))
                    .bg(rgb(theme.divider)),
            )
            .child(self.zed_button("git-zed", false, project_id, theme, cx))
            .children(discard)
            .children(stage)
            // No comments on a past commit's lines: no "⋯" (all comment actions) in the History.
            .when(view.can_comment(), |el| {
                el.child(self.git_diff_more(project_id, view, theme, cx))
            })
    }

    /// "⋯": Comment on the selected lines and Send to agent (both also on the bar under picked
    /// lines and on their shortcuts).
    fn git_diff_more(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let open = self.git_menu == Some(GitMenu::More);
        div()
            .flex_none()
            .child(
                icon_button("git-diff-more", Icon::Ellipsis, theme)
                    .tooltip(tooltip(theme, "More actions", ""))
                    // An open menu already closed on this mouse down (outside it): the click
                    // reopens it.
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.open_git_menu(GitMenu::More, window, cx);
                    })),
            )
            .when(open, |el| {
                el.child(self.render_git_more_menu(project_id, view, theme, cx))
            })
    }

    fn render_git_more_menu(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let can_comment =
            matches!(view.diff, git_view::Diff::Shown(_)) && view.comments.selection.is_some();
        let sendable = match &view.comments.list {
            comments::List::Loaded(list) => {
                doc_dialog::sendable(list, view.comments.selection).len()
            }
            _ => 0,
        };
        let label = |text: String, keys: SharedString| {
            [
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme.text))
                    .child(text),
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(keys),
            ]
        };
        // A row that cannot act now: faded, no hover, no click (no dead buttons).
        let disabled = |text: &'static str, keys: SharedString| {
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .py(px(space::XS))
                .opacity(opacity::DISABLED)
                .text_size(font_size::BODY)
                .children(label(text.to_owned(), keys))
                .into_any_element()
        };
        let comment_keys: SharedString = self.keymap.label(Command::AddComment).into();
        let send_keys: SharedString = self.keymap.label(Command::SendComments).into();
        let comment = if can_comment {
            menu_item("git-more-comment".into(), theme)
                .children(label("Comment on selected lines".into(), comment_keys))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.close_git_menu(window, cx);
                    shell.start_comment(project_id, CommentContext::Diff, window, cx);
                }))
                .into_any_element()
        } else {
            disabled("Comment (select lines first)", comment_keys)
        };
        let send = if sendable > 0 {
            let text = if sendable == 1 {
                "Send 1 open comment to agent…".to_owned()
            } else {
                format!("Send {sendable} open comments to agent…")
            };
            menu_item("git-more-send".into(), theme)
                .children(label(text, send_keys))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.close_git_menu(window, cx);
                    shell.open_send_dialog(project_id, CommentContext::Diff, window, cx);
                }))
                .into_any_element()
        } else {
            disabled("Send to agent (no open comments)", send_keys)
        };
        let panel = menu_panel("git-diff-more-panel", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.git_menu = None;
                cx.notify();
            }))
            .child(comment)
            .child(send);
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }

    /// The diff rows of `shown` in a variable-height list (wrapped lines are taller): reset for
    /// another diff or mode, remeasured when Wrap or the picked lines change, scrolled to the
    /// row asked for by [`Self::scroll_diff_to`]. With Wrap off the rows keep their longest
    /// line's width and the list scrolls sideways.
    fn git_diff_lines(
        &self,
        project_id: Ulid,
        view: &GitView,
        shown: &Rc<git_view::Shown>,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let mode = self.diff_mode;
        let wrap = self.diff_wrap;
        let picked = view.comments.selection;
        let can_comment = view.can_comment();
        let rows = shown.list_len(mode);
        let layout = (Rc::as_ptr(shown) as usize, mode, rows);
        if self.git_diff_layout.get() != Some(layout) {
            self.git_diff_layout.set(Some(layout));
            self.git_diff_looks.set(Some((wrap, picked)));
            self.git_diff_list.reset(rows);
        } else if self.git_diff_looks.get() != Some((wrap, picked)) {
            self.git_diff_looks.set(Some((wrap, picked)));
            self.git_diff_list.remeasure();
        }
        if let Some(row) = self.git_diff_scroll_to.take() {
            self.git_diff_list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
        }
        let shown = Rc::clone(shown);
        let current = view.hunk;
        // Hunk headers get a stage / unstage action when the file can go by hunk.
        let hunk_stage = view.hunk_change(0).map(|change| change.stage);
        let row_theme = theme.clone();
        let width = (!wrap).then(|| unwrapped_width(&shown, mode, theme, window));
        let list = list(
            self.git_diff_list.clone(),
            cx.processor(move |shell, index: usize, _, cx| {
                let theme = &row_theme;
                let pickable =
                    |el: gpui::Div, number: Option<u32>, cx: &mut Context<'_, Shell>| match number
                        .filter(|_| can_comment)
                    {
                        Some(number) => {
                            pick_on_click(el, project_id, number, picked, theme.border, cx)
                        }
                        None => el,
                    };
                let span = |at: usize, cx: &mut Context<'_, Shell>| {
                    span_row(&shown, at, current, hunk_stage, project_id, theme, cx)
                };
                let (row, new_number) = match mode {
                    DiffMode::Unified => match shown.rows.get(index) {
                        Some(git_view::DiffRow::Line(line)) => {
                            let row = unified_line(&shown, index, line, picked, wrap, theme);
                            (pickable(row, line.new_line, cx), line.new_line)
                        }
                        Some(_) => (span(index, cx), None),
                        None => return div().into_any_element(),
                    },
                    DiffMode::Split => match shown.split.get(index) {
                        Some(git_view::SplitRow::Span(at)) => (span(*at, cx), None),
                        // Only the new (right) half is pickable: comments anchor on new-side
                        // lines.
                        Some(row) => {
                            let mut new_number = None;
                            let row = git_split_row(&shown, row, picked, wrap, theme, |half, n| {
                                new_number = n;
                                pickable(half, n, cx)
                            });
                            (row, new_number)
                        }
                        None => return div().into_any_element(),
                    },
                };
                // The bar sits under the last picked line.
                match picked.filter(|picked| can_comment && new_number == Some(picked.end)) {
                    Some(picked) => div()
                        .flex()
                        .flex_col()
                        .child(row)
                        .child(shell.git_pick_bar(project_id, picked, theme, cx))
                        .into_any_element(),
                    None => row.into_any_element(),
                }
            }),
        )
        .pb(px(space::L));
        match width {
            None => list.flex_1().min_h_0().into_any_element(),
            Some(width) => div()
                .id("git-diff-sideways")
                .flex_1()
                .min_h_0()
                .overflow_x_scroll()
                .child(list.h_full().min_w_full().w(width))
                .into_any_element(),
        }
    }

    /// The bar under picked lines: "Line 117 selected", Comment (with its shortcut), Send to
    /// agent, Clear (Esc).
    fn git_pick_bar(
        &self,
        project_id: Ulid,
        picked: Selection,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let keys = self.keymap.label(Command::AddComment);
        let comments = self.git.get(&project_id).map(|view| &view.comments);
        let hover = theme.hover;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(space::S))
            .mx(px(space::L))
            .my(px(space::XS))
            .px(px(space::M))
            .py(px(space::XS))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.accent_bg))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.pane_focus_border))
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::BODY)
            .whitespace_nowrap()
            .child(div().text_color(rgb(theme.text)).child(format!(
                "{} selected",
                comments::lines_label(picked.start, picked.end)
            )))
            .child(
                button(
                    "git-pick-comment",
                    icon_text(Icon::MessageSquarePlus, "Comment", button_fg(theme, true)),
                    theme,
                    true,
                )
                .flex_none()
                .child(kbd(keys.clone(), theme, true))
                .tooltip(tooltip(theme, "Comment on the selected lines", keys))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.start_comment(project_id, CommentContext::Diff, window, cx);
                })),
            )
            .child(self.send_comments_button(project_id, CommentContext::Diff, comments, theme, cx))
            .child(div().flex_1())
            .child(
                quiet_button("git-pick-clear", "Clear (Esc)", theme, hover)
                    .flex_none()
                    .tooltip(tooltip(theme, "Unselect the lines", "Esc"))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.clear_diff_pick(project_id, cx);
                    })),
            )
    }

    /// Drops the picked diff lines (the bar under them closes).
    pub(crate) fn clear_diff_pick(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        if let Some(view) = self.git.get_mut(&project_id) {
            view.comments.selection = None;
        }
        cx.notify();
    }
}

/// The width the diff rows take with Wrap off: the longest line (tabs expanded) in the diff
/// font, plus the gutters (twice in Split).
fn unwrapped_width(
    shown: &git_view::Shown,
    mode: DiffMode,
    theme: &Theme,
    window: &Window,
) -> Pixels {
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&gpui::font(theme.terminal_font_family.clone()));
    let font_size = px(theme::DIFF_FONT_SIZE);
    let char_width = text_system
        .advance(font_id, font_size, 'm')
        .map_or(font_size, |size| size.width);
    // ponytail: rescans every line each frame while Wrap is off; cache it per diff if a
    // large diff lags.
    let longest = shown
        .rows
        .iter()
        .filter_map(|row| match row {
            git_view::DiffRow::Line(line) => Some(
                line.text.chars().count()
                    + line.text.matches('\t').count() * (highlight::TAB.len() - 1),
            ),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    // Numbers, sign and their gaps / paddings, in chars.
    let gutter = shown.number_width * 2 + 6;
    let chars = match mode {
        DiffMode::Unified => longest + gutter,
        DiffMode::Split => (longest + gutter) * 2,
    };
    char_width * chars as f32 + px(space::XL)
}

/// "Stage hunk" / "Unstage hunk" at the right end of a hunk header.
pub(crate) fn hunk_stage_button(
    project_id: Ulid,
    hunk: usize,
    stage: git_view::Stage,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let (label, tip) = match stage {
        git_view::Stage::Stage => ("Stage hunk", "Stage this hunk"),
        git_view::Stage::Unstage => ("Unstage hunk", "Unstage this hunk"),
    };
    row_action(
        SharedString::from(format!("git-hunk-stage-{hunk}")),
        label,
        theme,
    )
    .ml_auto()
    // No key: `s` / `u` act on the current hunk (n / p), not the hovered one.
    .tooltip(tooltip(theme, tip, ""))
    .on_click(cx.listener(move |shell, _, window, cx| {
        window.focus(&shell.git_focus, cx);
        if let Some(change) = shell.git_view(project_id).hunk_change(hunk) {
            shell.stage_git(project_id, change, window, cx);
        }
    }))
}

/// A row of the Discard dialog's preview: Unified, unwrapped, nothing to pick or stage.
pub(crate) fn git_diff_preview_row(
    shown: &git_view::Shown,
    index: usize,
    project_id: Ulid,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Div {
    match shown.rows.get(index) {
        Some(git_view::DiffRow::Line(line)) => unified_line(shown, index, line, None, false, theme),
        _ => span_row(shown, index, None, None, project_id, theme, cx),
    }
}

/// The diff font, line height and wrapping every diff row shares.
fn diff_frame(wrap: bool, theme: &Theme) -> gpui::Div {
    div()
        .w_full()
        .min_h(px(theme::DIFF_LINE_HEIGHT))
        .flex()
        .font_family(theme.terminal_font_family.clone())
        .text_size(px(theme::DIFF_FONT_SIZE))
        .line_height(px(theme::DIFF_LINE_HEIGHT))
        .when(!wrap, |el| el.whitespace_nowrap())
}

/// A row spanning the whole width (both Split halves): a collapsed gap of unchanged lines the
/// diff does not include, or a hunk header (`@@ … @@` in the accent, ▸ on the current hunk,
/// its section muted, Stage hunk on the right).
fn span_row(
    shown: &git_view::Shown,
    index: usize,
    current: Option<usize>,
    hunk_stage: Option<git_view::Stage>,
    project_id: Ulid,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Div {
    let frame = diff_frame(false, theme)
        .items_center()
        .gap(px(space::S))
        .px(px(space::L));
    match shown.rows.get(index) {
        Some(git_view::DiffRow::Gap(count)) => {
            let lines = if *count == 1 { "line" } else { "lines" };
            frame
                .justify_center()
                .py(px(space::XS))
                .bg(rgb(theme.surface_raised))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(format!("⋯ {count} unchanged {lines}"))
        }
        Some(git_view::DiffRow::Hunk(at)) => {
            let Some(hunk) = shown.diff.hunks.get(*at) else {
                return frame;
            };
            let is_current = current == Some(*at);
            frame
                .bg(rgb(theme.diff_hunk_header))
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.accent_text))
                        .when(is_current, |el| {
                            el.font_weight(FontWeight(weight::SEMIBOLD))
                        })
                        // The current hunk gets a marker, not only weight.
                        .child(format!(
                            "{}@@ -{},{} +{},{} @@",
                            if is_current { "▸ " } else { "" },
                            hunk.old_start,
                            hunk.old_lines,
                            hunk.new_start,
                            hunk.new_lines,
                        )),
                )
                // Shrinks, so the hunk action after it stays on screen.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text_muted))
                        .child(hunk.header.clone()),
                )
                .children(
                    hunk_stage.map(|stage| hunk_stage_button(project_id, *at, stage, theme, cx)),
                )
        }
        _ => frame,
    }
}

/// A changed line's row fill, gutter fill, sign color and sign. `modified` = a line changed
/// in place (a removed and an added line paired): amber instead of red / green.
fn line_colors(
    kind: DiffLineKind,
    modified: bool,
    theme: &Theme,
) -> (Option<u32>, Option<u32>, u32, &'static str) {
    match (kind, modified) {
        (DiffLineKind::Add, false) => (
            Some(theme.diff_add_bg),
            Some(theme.diff_add_gutter),
            theme.diff_add_text,
            "+",
        ),
        (DiffLineKind::Del, false) => (
            Some(theme.diff_del_bg),
            Some(theme.diff_del_gutter),
            theme.diff_del_text,
            "−",
        ),
        (DiffLineKind::Add, true) => (
            Some(theme.diff_mod_bg),
            Some(theme.diff_mod_gutter),
            theme.diff_mod_text,
            "+",
        ),
        (DiffLineKind::Del, true) => (
            Some(theme.diff_mod_bg),
            Some(theme.diff_mod_gutter),
            theme.diff_mod_text,
            "−",
        ),
        (DiffLineKind::Context, _) => (None, None, theme.text, " "),
    }
}

/// Whether new-side line `number` is among the `picked` ones.
fn is_picked(number: Option<u32>, picked: Option<Selection>) -> bool {
    number
        .zip(picked)
        .is_some_and(|(n, picked)| picked.contains(n, n))
}

/// A line's number (right-aligned to `width` digits; blanks when the side has none).
fn line_number(number: Option<u32>, width: usize) -> String {
    match number {
        Some(n) => format!("{n:>width$}"),
        None => " ".repeat(width),
    }
}

/// The gutter of a line: its own fill (the accent while picked), numbers and sign.
fn gutter(fill: Option<u32>, picked: bool, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .gap(px(space::S))
        .px(px(space::S))
        .text_color(rgb(if picked {
            theme.accent_text
        } else if fill.is_some() {
            theme.text_dim
        } else {
            theme.text_muted
        }))
        .map(|el| match (picked, fill) {
            (true, _) => el.bg(rgb(theme.accent_bg)),
            (false, Some(fill)) => el.bg(rgb(fill)),
            (false, None) => el,
        })
}

/// One Unified line: old and new numbers and the sign in the gutter, then the text. Picked
/// lines get the accent bar and gutter.
fn unified_line(
    shown: &git_view::Shown,
    index: usize,
    line: &mkrew_protocol::DiffLine,
    picked: Option<Selection>,
    wrap: bool,
    theme: &Theme,
) -> gpui::Div {
    let modified = shown.modified.get(index).copied().unwrap_or(false);
    let (bg, fill, fg, sign) = line_colors(line.kind, modified, theme);
    let is_picked = is_picked(line.new_line, picked);
    let width = shown.number_width;
    diff_frame(wrap, theme)
        .border_l(px(theme::DIFF_PICK_BAR_WIDTH))
        .border_color(if is_picked {
            rgb(theme.accent).into()
        } else {
            gpui::transparent_black()
        })
        .when_some(bg, |el, bg| el.bg(rgb(bg)))
        .child(
            gutter(fill, is_picked, theme)
                .child(line_number(line.old_line, width))
                .child(line_number(line.new_line, width))
                .child(div().text_color(rgb(fg)).child(sign)),
        )
        .child(diff_text(shown, index, line, fg, theme))
}

/// A diff line's text (tabs expanded, syntax colors of `shown.spans[index]`), then the "no
/// newline at end of file" pill when it has none. In a highlighted diff, text outside tokens
/// is plain text (the sign and fills still tell added from removed); unhighlighted diffs keep
/// the diff text color `fg`.
fn diff_text(
    shown: &git_view::Shown,
    index: usize,
    line: &mkrew_protocol::DiffLine,
    fg: u32,
    theme: &Theme,
) -> gpui::Div {
    let spans = shown.spans.get(index).map(Vec::as_slice).unwrap_or(&[]);
    let text_color = if shown.highlighted { theme.text } else { fg };
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(space::S))
        .px(px(space::S))
        .child(
            div()
                .min_w_0()
                .text_color(rgb(text_color))
                .child(preview::code_line(
                    line.text.replace('\t', highlight::TAB),
                    spans,
                    &[],
                    theme,
                )),
        )
        .when(line.no_newline, |el| {
            el.child(
                div()
                    .flex_none()
                    .px(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .bg(rgb(theme.surface_raised))
                    .font_family(theme.ui_font_family.clone())
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("no newline at end of file"),
            )
        })
}

/// Which half of a Split row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffSide {
    Old,
    New,
}

/// One half of a Split row: the old (left) or new (right) line `index` of `shown.rows`, or a
/// hatched cell where that side has no line. `modified` = the row pairs a removed with an
/// added line (amber on both halves). Only the new side shows the picked bar.
fn split_half(
    shown: &git_view::Shown,
    index: Option<usize>,
    side: DiffSide,
    modified: bool,
    picked: Option<Selection>,
    theme: &Theme,
) -> gpui::Div {
    let half = div()
        .flex_1()
        .min_w_0()
        .flex()
        .border_l(px(theme::DIFF_PICK_BAR_WIDTH))
        .border_color(gpui::transparent_black());
    let Some((at, git_view::DiffRow::Line(line))) =
        index.and_then(|at| Some((at, shown.rows.get(at)?)))
    else {
        return half.bg(pattern_slash(
            rgb(theme.border),
            theme::DIFF_HATCH_WIDTH,
            theme::DIFF_HATCH_INTERVAL,
        ));
    };
    let (bg, fill, fg, sign) = line_colors(line.kind, modified, theme);
    let number = match side {
        DiffSide::Old => line.old_line,
        DiffSide::New => line.new_line,
    };
    let is_picked = side == DiffSide::New && is_picked(line.new_line, picked);
    half.when_some(bg, |el, bg| el.bg(rgb(bg)))
        .when(is_picked, |el| el.border_color(rgb(theme.accent)))
        .child(
            gutter(fill, is_picked, theme)
                .child(line_number(number, shown.number_width))
                .child(div().text_color(rgb(fg)).child(sign)),
        )
        .child(diff_text(shown, at, line, fg, theme))
}

/// A Split row that pairs lines (`Same` / `Change`; spans are [`span_row`]s): old half, a
/// 1 px divider, new half, in one row so wrapped halves stay the same height. `on_new_half`
/// gets the new half and its new-side line number (`None` for a hatched cell) so the caller
/// can make it pickable for a comment.
pub(crate) fn git_split_row(
    shown: &git_view::Shown,
    row: &git_view::SplitRow,
    picked: Option<Selection>,
    wrap: bool,
    theme: &Theme,
    on_new_half: impl FnOnce(gpui::Div, Option<u32>) -> gpui::Div,
) -> gpui::Div {
    let (old, new, modified) = match row {
        git_view::SplitRow::Same(at) => (Some(*at), Some(*at), false),
        git_view::SplitRow::Change { old, new } => (*old, *new, old.is_some() && new.is_some()),
        git_view::SplitRow::Span(_) => (None, None, false),
    };
    let new_number = new.and_then(|at| match shown.rows.get(at) {
        Some(git_view::DiffRow::Line(line)) => line.new_line,
        _ => None,
    });
    diff_frame(wrap, theme)
        .child(split_half(
            shown,
            old,
            DiffSide::Old,
            modified,
            picked,
            theme,
        ))
        .child(
            div()
                .flex_none()
                .w(px(theme::BORDER_WIDTH))
                .bg(rgb(theme.border)),
        )
        .child(on_new_half(
            split_half(shown, new, DiffSide::New, modified, picked, theme),
            new_number,
        ))
}

/// Makes `el` (a diff row, or the new half of a Split row) pick new-side line `number` for a
/// comment on click; hover previews the pick bar unless the line is already picked.
pub(crate) fn pick_on_click(
    el: gpui::Div,
    project_id: Ulid,
    number: u32,
    picked: Option<Selection>,
    border: u32,
    cx: &Context<'_, Shell>,
) -> gpui::Div {
    el.cursor_pointer()
        .when(!picked.is_some_and(|p| p.contains(number, number)), |el| {
            el.hover(move |style| style.border_color(rgb(border)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                shell.pick_diff_line(project_id, number, event.modifiers.shift, window, cx);
            }),
        )
}
