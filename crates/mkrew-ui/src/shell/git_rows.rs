//! Git list rows: changed files and commits.

use crate::*;

impl Shell {
    /// The changed-file list (in the History: the commits, the expanded one with its files).
    pub(crate) fn git_list(
        &self,
        project_id: Ulid,
        view: &GitView,
        rows: Vec<git_view::ListRow>,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let rows = Rc::new(rows);
        let selected = view.selected.clone();
        let focused = self.git_focus.is_focused(window);
        let row_theme = theme.clone();
        let now = status::now_ms();
        uniform_list(
            "git-file-list",
            rows.len(),
            cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                range
                    .filter_map(|index| rows.get(index))
                    .map(|row| {
                        git_file_row(
                            project_id,
                            row,
                            selected.as_ref(),
                            focused,
                            now,
                            &row_theme,
                            cx,
                        )
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.git_files_scroll)
        .flex_1()
        .min_h_0()
        .px(px(space::S))
        .into_any_element()
    }
}

/// "+3 −1" in the diff colors.
pub(crate) fn change_counts(added: u32, removed: u32, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .gap(px(space::XS))
        .text_size(font_size::META)
        .child(
            div()
                .text_color(rgb(theme.diff_add_text))
                .child(format!("+{added}")),
        )
        .child(
            div()
                .text_color(rgb(theme.diff_del_text))
                .child(format!("−{removed}")),
        )
}

/// One row of the Git changed-file list: a group title with its count, a file (change letter,
/// name, folder muted, +/− counts; indented under its History commit), a History commit
/// ([`git_commit_row`], ages at `now`) or a note under it. Every row is one body line high
/// (uniform list); the selected file is raised, with a focus ring while the list has the
/// keyboard.
pub(crate) fn git_file_row(
    project_id: Ulid,
    row: &git_view::ListRow,
    selected: Option<&git_view::Target>,
    focused: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let (target, file) = match row {
        git_view::ListRow::Header(group, count) => {
            // The same box as a file row (uniform list: rows are sized from the first one).
            return div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .px(px(space::S))
                .py(px(space::XS))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.bg))
                .text_size(font_size::BODY)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(group.title())
                .child(count.to_string())
                .into_any_element();
        }
        git_view::ListRow::Note(note) => {
            // Indented under its commit, like the commit's files.
            return div()
                .w_full()
                .flex()
                .items_center()
                .px(px(space::S))
                .pl(px(space::S + space::M))
                .py(px(space::XS))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.bg))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(note.clone())
                .into_any_element();
        }
        git_view::ListRow::File(target, file) => (target.clone(), file),
        git_view::ListRow::Commit { commit, expanded } => {
            // Raised while none of its files is selected (they are listing).
            let picked = *expanded && selected.is_none();
            return git_commit_row(
                project_id, commit, *expanded, picked, focused, now, theme, cx,
            );
        }
    };
    let group = target.group;
    let picked = selected == Some(&target);
    let (letter, color, what) = match file.kind {
        ChangeKind::Added => ("A", theme.diff_add_text, "Added"),
        ChangeKind::Modified => ("M", theme.accent, "Modified"),
        ChangeKind::Deleted => ("D", theme.diff_del_text, "Deleted"),
        ChangeKind::Renamed => ("R", theme.accent, "Renamed"),
        ChangeKind::Copied => ("C", theme.accent, "Copied"),
        ChangeKind::TypeChanged => ("T", theme.accent, "File type changed"),
        ChangeKind::Untracked => ("?", theme.diff_add_text, "Untracked"),
        ChangeKind::Conflicted => ("!", theme.diff_del_text, "Conflicted"),
    };
    let (dir, name) = match file.path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir.to_owned()), name.to_owned()),
        None => (None, file.path.clone()),
    };
    let fill = if picked {
        theme.surface_raised
    } else {
        theme.bg
    };
    let hover = theme.surface_raised;
    let id = format!("git-file-{group:?}-{}", file.path);
    let id = SharedString::from(id);
    div()
        .id(id.clone())
        .group(id.clone())
        .w_full()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        // Indented under its commit inside the full-width row (a margin would overflow it).
        .when(group == git_view::Group::Commit, |el| {
            el.pl(px(space::S + space::M))
        })
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        // The border is always there (so rows never shift); it only shows as the focus ring.
        .border_color(rgb(if picked && focused {
            theme.pane_focus_border
        } else {
            fill
        }))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(picked, |el| el.bg(rgb(fill)))
        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(
            div()
                .id(SharedString::from(format!("{id}-kind")))
                .flex_none()
                .w(px(space::M))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(color))
                .tooltip(tooltip(theme, what, ""))
                .child(letter),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .gap(px(space::XS))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text))
                        .when(picked, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(name),
                )
                .children(dir.map(|dir| {
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(dir)
                })),
        )
        .children(
            git_view::counts(file).map(|(added, removed)| change_counts(added, removed, theme)),
        )
        // Shown on hover and on the selected file (SPEC §14 progressive disclosure).
        .children(git_view::Stage::of(group).map(|stage| {
            let target = target.clone();
            row_action(
                SharedString::from(format!("{id}-stage")),
                stage.label(),
                theme,
            )
            .when(!picked, |el| {
                el.opacity(opacity::HIDDEN)
                    .group_hover(id.clone(), |style| style.opacity(opacity::FULL))
            })
            .tooltip(tooltip(
                theme,
                match stage {
                    git_view::Stage::Stage => "Stage this file",
                    git_view::Stage::Unstage => "Unstage this file",
                },
                "",
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on the row (which would select it).
                cx.stop_propagation();
                window.focus(&shell.git_focus, cx);
                if let Some(change) = shell.git_view(project_id).file_change(&target) {
                    shell.stage_git(project_id, change, window, cx);
                }
            }))
        }))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_git_file(project_id, target.clone(), window, cx);
        }))
        .into_any_element()
}

/// One History commit row: a chevron (open on the expanded commit), the summary, then its
/// short hash and age muted; the full message and author in the tooltip. Raised (with the
/// focus ring while the list has the keyboard) while it is `picked`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn git_commit_row(
    project_id: Ulid,
    commit: &Commit,
    expanded: bool,
    picked: bool,
    focused: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let fill = if picked {
        theme.surface_raised
    } else {
        theme.bg
    };
    let hover = theme.surface_raised;
    let short = git_history::short_hash(&commit.hash);
    let meta = format!("{short} · {}", git_history::age(commit.authored_at, now));
    let message = if commit.body.is_empty() {
        commit.summary.clone()
    } else {
        format!("{}\n\n{}", commit.summary, commit.body.trim_end())
    };
    let tip = format!(
        "{message}\n\n{} <{}> · {short}",
        commit.author_name, commit.author_email
    );
    let hash = commit.hash.clone();
    div()
        .id(SharedString::from(format!("git-commit-{}", commit.hash)))
        .w_full()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if picked && focused {
            theme.pane_focus_border
        } else {
            fill
        }))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(picked, |el| el.bg(rgb(fill)))
        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .tooltip(tooltip(theme, &tip, ""))
        .child(
            div()
                .flex_none()
                .w(px(space::M))
                .text_color(rgb(theme.text_muted))
                .child(if expanded { "▾" } else { "▸" }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(theme.text))
                .when(expanded, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(commit.summary.clone()),
        )
        .child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(meta),
        )
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_git_commit(project_id, hash.clone(), window, cx);
        }))
        .into_any_element()
}

/// A small text action inside a list row ("Stage" on a file row, "Stage hunk" on a hunk
/// header): no vertical padding, so the row keeps its height.
pub(crate) fn row_action(
    id: SharedString,
    label: &'static str,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    // One step above the rows' own `surface_raised` hover / selection fill.
    let hover = theme.border;
    div()
        .id(id)
        .flex_none()
        .px(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .font_family(theme.ui_font_family.clone())
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(label)
}
