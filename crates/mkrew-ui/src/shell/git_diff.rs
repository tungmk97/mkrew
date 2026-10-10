//! Git diff rendering: unified and split rows, hunk buttons, line picks.

use crate::*;

impl Shell {
    /// The selected file's diff: its path, counts, current hunk and Open in Zed, then the
    /// unified diff (or why it is not shown).
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
        let comment_button = self.add_comment_button(
            project_id,
            CommentContext::Diff,
            matches!(view.diff, git_view::Diff::Shown(_)) && view.comments.selection.is_some(),
            theme,
            cx,
        );
        let send_button = self.send_comments_button(
            project_id,
            CommentContext::Diff,
            Some(&view.comments),
            theme,
            cx,
        );
        let diff_toggle = self.mode_toggle(
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
        );
        let zed_keys = self.keymap.label(Command::OpenInZed);
        let zed_button = |id: &'static str, primary: bool| {
            button(id, "Open in Zed", theme, primary)
                .flex_none()
                .tooltip(tooltip(theme, "Open this file in Zed", zed_keys.clone()))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    shell.open_git_file_in_zed(project_id, cx);
                }))
        };
        let shown = match &view.diff {
            git_view::Diff::Shown(shown) => Some(Rc::clone(shown)),
            _ => None,
        };
        let renamed = shown
            .as_ref()
            .and_then(|shown| shown.diff.old_path.clone())
            .map(|old| format!("from {old}"));
        let hunk_label = shown.as_ref().and_then(|shown| {
            let count = shown.diff.hunks.len();
            match view.hunk {
                Some(hunk) => Some(format!("Hunk {} of {count}", hunk + 1)),
                None if count > 1 => Some(format!("{count} hunks")),
                None => None,
            }
        });
        let counts = shown
            .as_ref()
            .filter(|shown| !shown.diff.binary)
            .map(|shown| (shown.diff.added, shown.diff.removed));
        // Unified | Split (global, remembered): only for a diff that has lines to lay out.
        let has_lines = shown.as_ref().is_some_and(|shown| {
            !shown.diff.binary && !shown.diff.too_large && !shown.rows.is_empty()
        });
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::S))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(selected.path.clone()),
            )
            .children(renamed.map(|renamed| meta_line(theme, renamed.into()).flex_none()))
            .children(counts.map(|(added, removed)| change_counts(added, removed, theme)))
            .children(hunk_label.map(|label| {
                let keys = format!(
                    "{} / {}",
                    self.keymap.label(Command::GitNextHunk),
                    self.keymap.label(Command::GitPreviousHunk)
                );
                div()
                    .id("git-hunks")
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, "Next / previous hunk", keys))
                    .child(label)
            }))
            .when(has_lines, |header| header.child(diff_toggle))
            .children(view.file_change(&selected).map(|change| {
                let (label, tip, command) = match change.stage {
                    git_view::Stage::Stage => {
                        ("Stage file", "Stage the whole file", Command::GitStage)
                    }
                    git_view::Stage::Unstage => (
                        "Unstage file",
                        "Unstage the whole file",
                        Command::GitUnstage,
                    ),
                };
                button("git-stage-file", label, theme, false)
                    .flex_none()
                    .tooltip(tooltip(theme, tip, self.keymap.label(command)))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        window.focus(&shell.git_focus, cx);
                        shell.stage_git(project_id, change.clone(), window, cx);
                    }))
            }))
            .children(view.discard(&selected).map(|_| {
                let target = selected.clone();
                button("git-discard", "Discard…", theme, false)
                    .flex_none()
                    .tooltip(tooltip(
                        theme,
                        "Discard this file's changes (asks first)",
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.open_git_discard(project_id, target.clone(), window, cx);
                    }))
            }))
            // No comments on a past commit's lines: no dead buttons in the History.
            .when(view.can_comment(), |header| {
                header.child(comment_button).child(send_button)
            })
            .child(zed_button("git-zed", false));
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
                .child(zed_button("git-zed-notice", true))
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
                let shown = Rc::clone(shown);
                let mode = self.diff_mode;
                let current = view.hunk;
                // Hunk headers get a stage / unstage action when the file can go by hunk.
                let hunk_stage = view.hunk_change(0).map(|change| change.stage);
                let picked = view.comments.selection;
                let can_comment = view.can_comment();
                let line_height = theme.terminal_font_size * theme.terminal_line_height;
                let row_theme = theme.clone();
                let rows = shown.list_len(mode);
                uniform_list(
                    "git-diff",
                    rows,
                    cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| {
                                Some(match mode {
                                    DiffMode::Unified => {
                                        let row = shown.rows.get(index)?;
                                        let frame = git_diff_row(
                                            &shown,
                                            index,
                                            current,
                                            picked,
                                            line_height,
                                            &row_theme,
                                        );
                                        match (row, hunk_stage) {
                                            (git_view::DiffRow::Hunk(hunk), Some(stage)) => frame
                                                .child(hunk_stage_button(
                                                    project_id, *hunk, stage, &row_theme, cx,
                                                ))
                                                .into_any_element(),
                                            // New-side lines can be picked for a comment; hover
                                            // previews the pick bar.
                                            (git_view::DiffRow::Line(line), _) => {
                                                match line.new_line.filter(|_| can_comment) {
                                                    Some(number) => pick_on_click(
                                                        frame,
                                                        project_id,
                                                        number,
                                                        picked,
                                                        row_theme.border,
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                    None => frame.into_any_element(),
                                                }
                                            }
                                            _ => frame.into_any_element(),
                                        }
                                    }
                                    DiffMode::Split => match shown.split.get(index)? {
                                        // Gap and hunk rows span both halves.
                                        git_view::SplitRow::Span(at) => {
                                            let frame = git_diff_row(
                                                &shown,
                                                *at,
                                                current,
                                                picked,
                                                line_height,
                                                &row_theme,
                                            );
                                            match (shown.rows.get(*at)?, hunk_stage) {
                                                (git_view::DiffRow::Hunk(hunk), Some(stage)) => {
                                                    frame
                                                        .child(hunk_stage_button(
                                                            project_id, *hunk, stage, &row_theme,
                                                            cx,
                                                        ))
                                                        .into_any_element()
                                                }
                                                _ => frame.into_any_element(),
                                            }
                                        }
                                        // Only the new (right) half is pickable: comments
                                        // anchor on new-side lines.
                                        row => git_split_row(
                                            &shown,
                                            row,
                                            picked,
                                            line_height,
                                            &row_theme,
                                            |half, number| match number.filter(|_| can_comment) {
                                                Some(number) => pick_on_click(
                                                    half,
                                                    project_id,
                                                    number,
                                                    picked,
                                                    row_theme.border,
                                                    cx,
                                                ),
                                                None => half,
                                            },
                                        )
                                        .into_any_element(),
                                    },
                                })
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.git_diff_scroll)
                .flex_1()
                .min_h_0()
                .pb(px(space::L))
                .font_family(theme.terminal_font_family.clone())
                .text_size(px(theme.terminal_font_size))
                .into_any_element()
            }
        };
        panel
            .child(header)
            .children(view.zed_error.clone().map(|error| {
                notice(theme.text, format!("✕ {error}"))
                    .px(px(space::L))
                    .pb(px(space::S))
            }))
            .children(view.stage_error.clone().map(|error| {
                notice(theme.text, format!("✕ Staging failed: {error}"))
                    .px(px(space::L))
                    .pb(px(space::S))
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

/// One row of the Git diff viewer, `line_height` high: a collapsed gap of unchanged lines, a
/// hunk header (the current hunk in the accent), or a line with its old / new numbers, its
/// `+` / `−` sign, the diff colors and syntax colors. Lines whose new-side number is `picked`
/// (for a comment) get an accent bar and an accent number.
pub(crate) fn git_diff_row(
    shown: &git_view::Shown,
    index: usize,
    current: Option<usize>,
    picked: Option<Selection>,
    line_height: f32,
    theme: &Theme,
) -> gpui::Div {
    // Every row has the bar's width, so the gutters line up.
    let frame = div()
        .w_full()
        .h(px(line_height))
        .flex()
        .items_center()
        .gap(px(space::M))
        .px(px(space::L))
        .border_l(px(theme::DIFF_PICK_BAR_WIDTH))
        .border_color(gpui::transparent_black())
        .whitespace_nowrap();
    let Some(row) = shown.rows.get(index) else {
        return frame;
    };
    let width = shown.number_width;
    let number = |n: Option<u32>| match n {
        Some(n) => format!("{n:>width$}"),
        None => " ".repeat(width),
    };
    match row {
        git_view::DiffRow::Gap(count) => {
            let lines = if *count == 1 { "line" } else { "lines" };
            frame
                .text_color(rgb(theme.text_muted))
                .child(format!("⋯ {count} unchanged {lines}"))
        }
        git_view::DiffRow::Hunk(index) => {
            let Some(hunk) = shown.diff.hunks.get(*index) else {
                return frame;
            };
            let at = current == Some(*index);
            // The current hunk gets a marker, not only the accent color.
            let text = format!(
                "{} @@ -{},{} +{},{} @@ {}",
                if at { "▸" } else { " " },
                hunk.old_start,
                hunk.old_lines,
                hunk.new_start,
                hunk.new_lines,
                hunk.header
            );
            frame
                .bg(rgb(theme.diff_hunk_header))
                .text_color(rgb(if at { theme.accent } else { theme.text_muted }))
                // Shrinks, so a hunk action after it stays on screen.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(text),
                )
        }
        git_view::DiffRow::Line(line) => {
            let modified = shown.modified.get(index).copied().unwrap_or(false);
            let (bg, fg, sign) = match (line.kind, modified) {
                (DiffLineKind::Add, false) => (Some(theme.diff_add_bg), theme.diff_add_text, "+"),
                (DiffLineKind::Del, false) => (Some(theme.diff_del_bg), theme.diff_del_text, "−"),
                (DiffLineKind::Add, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "+"),
                (DiffLineKind::Del, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "−"),
                (DiffLineKind::Context, _) => (None, theme.text, " "),
            };
            let is_picked = line
                .new_line
                .zip(picked)
                .is_some_and(|(n, picked)| picked.contains(n, n));
            frame
                .when_some(bg, |el, bg| el.bg(rgb(bg)))
                .when(is_picked, |el| el.border_color(rgb(theme.accent)))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .gap(px(space::S))
                        .text_color(rgb(theme.text_muted))
                        .child(number(line.old_line))
                        .child(
                            div()
                                .when(is_picked, |el| el.text_color(rgb(theme.accent)))
                                .child(number(line.new_line)),
                        ),
                )
                .child(diff_code(shown, index, line, sign, fg, theme))
                .when(line.no_newline, |el| {
                    el.child(
                        div()
                            .text_color(rgb(theme.text_muted))
                            .child("(no newline at end of file)"),
                    )
                })
        }
    }
}

/// A diff line's `sign` and text (tabs expanded, syntax colors of `shown.spans[index]`): the
/// part of a row after the numbers, in Unified and in each Split half. In a highlighted diff,
/// text outside tokens is plain text (the sign and background still tell added from removed);
/// unhighlighted diffs keep the diff text color `fg`.
pub(crate) fn diff_code(
    shown: &git_view::Shown,
    index: usize,
    line: &mkrew_protocol::DiffLine,
    sign: &'static str,
    fg: u32,
    theme: &Theme,
) -> gpui::Div {
    let spans = shown.spans.get(index).map(Vec::as_slice).unwrap_or(&[]);
    let text_color = if shown.highlighted { theme.text } else { fg };
    div()
        .flex()
        .gap(px(space::S))
        .child(div().text_color(rgb(fg)).child(sign))
        .child(div().text_color(rgb(text_color)).child(preview::code_line(
            line.text.replace('\t', highlight::TAB),
            spans,
            &[],
            theme,
        )))
}

/// Which half of a Split row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffSide {
    Old,
    New,
}

/// One half of a Split row: the old (left) or new (right) line `index` of `shown.rows`, or an
/// empty padding cell. `modified` = the row pairs a removed with an added line (amber on both
/// halves); otherwise removed / added lines keep the delete / add colors. Both halves have the
/// pick bar's width, so the columns line up; only the new side shows the picked bar.
pub(crate) fn split_half(
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
        .h_full()
        .overflow_hidden()
        .flex()
        .items_center()
        .gap(px(space::M))
        .px(px(space::M))
        .border_l(px(theme::DIFF_PICK_BAR_WIDTH))
        .border_color(gpui::transparent_black())
        .whitespace_nowrap();
    let Some((at, git_view::DiffRow::Line(line))) =
        index.and_then(|at| Some((at, shown.rows.get(at)?)))
    else {
        return half;
    };
    let (bg, fg, sign) = match (line.kind, modified) {
        (DiffLineKind::Add, false) => (Some(theme.diff_add_bg), theme.diff_add_text, "+"),
        (DiffLineKind::Del, false) => (Some(theme.diff_del_bg), theme.diff_del_text, "−"),
        (DiffLineKind::Add, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "+"),
        (DiffLineKind::Del, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "−"),
        (DiffLineKind::Context, _) => (None, theme.text, " "),
    };
    let number = match side {
        DiffSide::Old => line.old_line,
        DiffSide::New => line.new_line,
    };
    let is_picked = side == DiffSide::New
        && line
            .new_line
            .zip(picked)
            .is_some_and(|(n, picked)| picked.contains(n, n));
    half.when_some(bg, |el, bg| el.bg(rgb(bg)))
        .when(is_picked, |el| el.border_color(rgb(theme.accent)))
        .child(
            div()
                .flex_none()
                .text_color(rgb(if is_picked {
                    theme.accent
                } else {
                    theme.text_muted
                }))
                .child(match number {
                    Some(n) => format!("{n:>width$}", width = shown.number_width),
                    None => " ".repeat(shown.number_width),
                }),
        )
        .child(diff_code(shown, at, line, sign, fg, theme))
        .when(line.no_newline, |el| {
            el.child(
                div()
                    .text_color(rgb(theme.text_muted))
                    .child("(no newline at end of file)"),
            )
        })
}

/// A Split row that pairs lines (`Same` / `Change`; spans are drawn by [`git_diff_row`]):
/// old half, a 1 px divider, new half. `on_new_half` gets the new half and its new-side line
/// number (`None` for padding) so the caller can make it pickable for a comment.
pub(crate) fn git_split_row(
    shown: &git_view::Shown,
    row: &git_view::SplitRow,
    picked: Option<Selection>,
    line_height: f32,
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
    div()
        .w_full()
        .h(px(line_height))
        .flex()
        .items_center()
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
                .h_full()
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
