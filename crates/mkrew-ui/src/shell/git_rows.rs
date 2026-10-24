//! Git list rows: group headers, changed files and commits.

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
        let collapsed = view.collapsed.clone();
        let focused = self.git_focus.is_focused(window);
        let row_theme = theme.clone();
        let now = status::now_ms();
        let lanes = view.graph_lanes();
        let viewed = Rc::clone(&view.viewed);
        uniform_list(
            "git-file-list",
            rows.len(),
            cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                range
                    .filter_map(|index| rows.get(index).map(|row| (index, row)))
                    .map(|(index, row)| {
                        let folded = match row {
                            git_view::ListRow::Header(group, _) => collapsed.contains(group),
                            _ => false,
                        };
                        let graph = (lanes > 0)
                            .then(|| git_view::graph_cell(&rows, index))
                            .flatten()
                            .map(|cell| (cell, lanes));
                        let is_viewed = matches!(
                            row,
                            git_view::ListRow::File(target, _)
                                if viewed.contains(target.group, &target.path)
                        );
                        git_file_row(
                            project_id,
                            row,
                            selected.as_ref(),
                            folded,
                            is_viewed,
                            focused,
                            graph,
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

/// "+3 −1" in the diff colors, in the mono font.
pub(crate) fn change_counts(added: u32, removed: u32, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .gap(px(space::XS))
        .font_family(theme.terminal_font_family.clone())
        .text_size(font_size::META)
        .child(
            div()
                .text_color(rgb(theme.success_text))
                .child(format!("+{added}")),
        )
        .child(
            div()
                .text_color(rgb(theme.danger_text))
                .child(format!("−{removed}")),
        )
}

/// The change letter of a file, its color and what it means (tooltip).
pub(crate) fn change_letter(kind: ChangeKind, theme: &Theme) -> (&'static str, u32, &'static str) {
    match kind {
        ChangeKind::Added => ("A", theme.diff_add_text, "Added"),
        ChangeKind::Modified => ("M", theme.git_modified, "Modified"),
        ChangeKind::Deleted => ("D", theme.diff_del_text, "Deleted"),
        ChangeKind::Renamed => ("R", theme.git_modified, "Renamed"),
        ChangeKind::Copied => ("C", theme.git_modified, "Copied"),
        ChangeKind::TypeChanged => ("T", theme.git_modified, "File type changed"),
        ChangeKind::Untracked => ("?", theme.git_untracked, "Untracked"),
        ChangeKind::Conflicted => ("!", theme.diff_del_text, "Conflicted"),
    }
}

/// The box every row of the changed-file list shares (uniform list: rows are sized from the
/// first one).
fn list_row(id: SharedString) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w_full()
        .h(theme::rems_px(theme::GIT_FILE_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::transparent_black())
}

/// One row of the Git changed-file list: a group header (overline title, count, folds on
/// click; "Stage all" / "Unstage all" on hover), the empty Staged hint (a dashed box), a file
/// (change-letter badge, name, folder muted, +/− counts; indented under its History commit),
/// a History commit ([`git_commit_row`], ages at `now`) or a note under it. The selected file
/// has the `hover` fill and an accent ring while the list has the keyboard; in the History,
/// `graph` is the row's column of the lane graph.
/// A viewed file is dimmed and its box is checked.
#[allow(clippy::too_many_arguments)]
pub(crate) fn git_file_row(
    project_id: Ulid,
    row: &git_view::ListRow,
    selected: Option<&git_view::Target>,
    folded: bool,
    viewed: bool,
    focused: bool,
    graph: Option<(GraphCell, usize)>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let (target, file) = match row {
        git_view::ListRow::Header(group, count) => {
            return group_header(project_id, *group, *count, folded, theme, cx);
        }
        git_view::ListRow::Hint(hint) => {
            // A dashed box filling the row, the message centered in it.
            return list_row(SharedString::from("git-staged-hint"))
                .child(
                    div()
                        .size_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::MEDIUM))
                        .border(px(theme::BORDER_WIDTH))
                        .border_dashed()
                        .border_color(rgb(theme.selected))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_subtle))
                        .child(div().min_w_0().truncate().child(*hint)),
                )
                .into_any_element();
        }
        git_view::ListRow::Note(note) => {
            // Indented under its commit, like the commit's files.
            return list_row(SharedString::from(format!("git-note-{note}")))
                .when(graph.is_none(), |el| el.child(under_commit()))
                .when_some(graph, |el, (cell, lanes)| {
                    el.child(graph_column(cell, lanes, theme))
                        .child(under_commit())
                })
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(div().min_w_0().truncate().child(note.clone()))
                .into_any_element();
        }
        git_view::ListRow::File(target, file) => (target.clone(), file),
        git_view::ListRow::Commit {
            commit, expanded, ..
        } => {
            // Raised while none of its files is selected (they are listing).
            let picked = *expanded && selected.is_none();
            return git_commit_row(
                project_id, commit, *expanded, picked, focused, graph, now, theme, cx,
            );
        }
    };
    let group = target.group;
    let picked = selected == Some(&target);
    let (letter, color, what) = change_letter(file.kind, theme);
    let (dir, name) = match file.path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir.to_owned()), name.to_owned()),
        None => (None, file.path.clone()),
    };
    let hover = theme.hover;
    let id = SharedString::from(format!("git-file-{group:?}-{}", file.path));
    let dim = viewed && !picked;
    list_row(id.clone())
        .group(id.clone())
        // Indented under its commit inside the full-width row (a margin would overflow it).
        .when(group == git_view::Group::Commit && graph.is_none(), |el| {
            el.child(under_commit())
        })
        .when_some(graph, |el, (cell, lanes)| {
            el.child(graph_column(cell, lanes, theme))
                .child(under_commit())
        })
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(picked, |el| {
            el.bg(rgb(theme.hover))
                .when(focused, |el| el.border_color(rgb(theme.accent)))
        })
        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(
            div()
                .id(SharedString::from(format!("{id}-kind")))
                .flex_none()
                .size(theme::rems_px(theme::GIT_BADGE_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::TAG))
                .bg(theme.tint(color))
                .text_size(font_size::OVERLINE)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(color))
                // A viewed row steps back (its content, not the ✓ box); the selected one stays readable.
                .when(dim, |el| el.opacity(opacity::VIEWED))
                .tooltip(tooltip(theme, what, ""))
                .child(letter),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .overflow_hidden()
                .when(dim, |el| el.opacity(opacity::VIEWED))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .child(name),
                )
                .children(dir.map(|dir| {
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_subtle))
                        .child(dir)
                })),
        )
        .children(git_view::counts(file).map(|(added, removed)| {
            change_counts(added, removed, theme).when(dim, |el| el.opacity(opacity::VIEWED))
        }))
        // Not for a History commit's files (never marked).
        .when(group != git_view::Group::Commit, |el| {
            let target = target.clone();
            el.child(
                viewed_box(viewed, theme)
                    .id(SharedString::from(format!("{id}-viewed")))
                    .cursor_pointer()
                    .tooltip(tooltip(
                        theme,
                        if viewed {
                            "Mark as not viewed"
                        } else {
                            "Mark as viewed"
                        },
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        // Not a click on the row (which would select it).
                        cx.stop_propagation();
                        window.focus(&shell.git_focus, cx);
                        shell.toggle_git_viewed(project_id, target.clone(), window, cx);
                    })),
            )
        })
        // Shown on hover and on the selected file (SPEC §14 progressive disclosure).
        .children(git_view::Stage::of(group).map(|stage| {
            let target = target.clone();
            icon_button(
                SharedString::from(format!("{id}-stage")),
                match stage {
                    git_view::Stage::Stage => Icon::Plus,
                    git_view::Stage::Unstage => Icon::Minus,
                },
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

/// A Working group header: chevron, overline title and count (11/600 upper case); a click
/// folds or unfolds the group. "Stage all" / "Unstage all" shows on hover while the group
/// lists files.
fn group_header(
    project_id: Ulid,
    group: git_view::Group,
    count: usize,
    folded: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let id = SharedString::from(format!("git-group-{group:?}"));
    let hover = theme.hover;
    list_row(id.clone())
        .group(id.clone())
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(
            theme,
            if folded {
                "Show this group's files"
            } else {
                "Hide this group's files"
            },
            "",
        ))
        .on_click(cx.listener(move |shell, _, _, cx| {
            shell.git_view(project_id).toggle_collapsed(group);
            cx.notify();
        }))
        .child(icon_svg(
            if folded {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            },
            theme::ICON_SIZE_SMALL,
            theme.chevron_color(),
        ))
        .child(overline(group.title(), theme))
        .child(overline(&count.to_string(), theme))
        .child(div().flex_1())
        .children(
            git_view::Stage::of(group)
                .filter(|_| count > 0)
                .map(|stage| {
                    let (label, tip) = match stage {
                        git_view::Stage::Stage => ("Stage all", "Stage every file of this group"),
                        git_view::Stage::Unstage => {
                            ("Unstage all", "Unstage every file of this group")
                        }
                    };
                    row_action(SharedString::from(format!("{id}-stage")), label, theme)
                        .opacity(opacity::HIDDEN)
                        .group_hover(id.clone(), |style| style.opacity(opacity::FULL))
                        .tooltip(tooltip(theme, tip, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            // Not a click on the header (which would fold it).
                            cx.stop_propagation();
                            window.focus(&shell.git_focus, cx);
                            if let Some(change) = shell.git_view(project_id).group_change(group) {
                                shell.stage_git(project_id, change, window, cx);
                            }
                        }))
                }),
        )
        .into_any_element()
}

/// One History commit row: its lane graph column, a chevron (open on the expanded commit), the
/// summary, then its short hash and age muted; the full message and author in the tooltip.
/// Selected (with the accent ring while the list has the keyboard) while it is `picked`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn git_commit_row(
    project_id: Ulid,
    commit: &Commit,
    expanded: bool,
    picked: bool,
    focused: bool,
    graph: Option<(GraphCell, usize)>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let hover = theme.hover;
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
    list_row(SharedString::from(format!("git-commit-{}", commit.hash)))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(picked, |el| {
            el.bg(rgb(theme.hover))
                .when(focused, |el| el.border_color(rgb(theme.accent)))
        })
        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .tooltip(tooltip(theme, &tip, ""))
        .when_some(graph, |el, (cell, lanes)| {
            el.child(graph_column(cell, lanes, theme))
        })
        .child(icon_svg(
            if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            theme::ICON_SIZE_SMALL,
            theme.chevron_color(),
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(theme.text))
                .when(expanded, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(commit.summary.clone()),
        )
        .child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_subtle))
                .child(meta),
        )
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_git_commit(project_id, hash.clone(), window, cx);
        }))
        .into_any_element()
}

/// A small action inside a list row ("Stage" on a file row, "Stage hunk" on a hunk header, or
/// an [`icon_svg`]): no vertical padding, so the row keeps its height.
pub(crate) fn row_action(
    id: SharedString,
    label: impl IntoElement,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    // One step above the rows' own `hover` / `selected` fill.
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

/// Lane colors of the History graph, from the workspace palette (≥ 3:1 on every surface, ADR
/// 0020); red and yellow are left to deletions and warnings.
const LANE_COLORS: [&str; 6] = ["blue", "purple", "teal", "orange", "pink", "green"];

/// The graph column of a History row (SPEC §12B), `lanes` wide: a commit's lines and dot, or
/// the lanes running straight through a row listed under a commit.
fn graph_column(cell: GraphCell, lanes: usize, theme: &Theme) -> gpui::Div {
    let colors = LANE_COLORS.map(|name| theme.workspace_color(name).unwrap_or(theme.accent));
    let color = move |lane: usize| rgb(colors[lane % colors.len()]);
    div()
        .flex_none()
        .h_full()
        .w(px(lanes as f32 * theme::GIT_GRAPH_LANE_WIDTH))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, (), window, _| {
                    let x = |lane: usize| {
                        bounds.left() + px(theme::GIT_GRAPH_LANE_WIDTH) * (lane as f32 + 0.5)
                    };
                    // Rows meet at their 1 px borders: draw over them so lines join.
                    let top = bounds.top() - px(theme::BORDER_WIDTH);
                    let bottom = bounds.bottom() + px(theme::BORDER_WIDTH);
                    let mid = bounds.center().y;
                    let shown = |lane: usize| lane < lanes;
                    let line = |window: &mut Window,
                                from: Point<Pixels>,
                                to: Point<Pixels>,
                                lane: usize| {
                        let mut path = PathBuilder::stroke(px(theme::GIT_GRAPH_LINE_WIDTH));
                        path.move_to(from);
                        path.line_to(to);
                        if let Ok(path) = path.build() {
                            window.paint_path(path, color(lane));
                        }
                    };
                    match &cell {
                        GraphCell::Through(through) => {
                            for &lane in through.iter().filter(|&&lane| shown(lane)) {
                                line(window, point(x(lane), top), point(x(lane), bottom), lane);
                            }
                        }
                        GraphCell::Commit(row) => {
                            for &(from, to) in &row.up {
                                if shown(from) && shown(to) {
                                    line(window, point(x(from), top), point(x(to), mid), from);
                                }
                            }
                            for &(from, to) in &row.down {
                                if shown(from) && shown(to) {
                                    line(window, point(x(from), mid), point(x(to), bottom), to);
                                }
                            }
                            let lane = row.lane.min(lanes.saturating_sub(1));
                            let dot = px(theme::GIT_GRAPH_DOT_SIZE);
                            let at = point(x(lane) - dot / 2.0, mid - dot / 2.0);
                            window.paint_quad(
                                fill(Bounds::new(at, size(dot, dot)), color(lane))
                                    .corner_radii(dot / 2.0),
                            );
                        }
                    }
                },
            )
            .size_full(),
        )
}

/// The "viewed" checkbox of a file: a 16 px square, filled with the accent and a ✓ while viewed.
pub(crate) fn viewed_box(on: bool, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .size(px(space::L))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .when(on, |el| {
            el.bg(rgb(theme.accent))
                .border_color(rgb(theme.accent))
                .child(icon_svg(
                    Icon::Check,
                    theme::ICON_SIZE_SMALL,
                    theme.on_accent,
                ))
        })
        .when(!on, |el| el.border_color(rgb(theme.text_subtle)))
}

/// `Viewed 3/10` in the side panel header (✓ once every file is viewed); none in the History
/// or while nothing is listed.
pub(crate) fn git_viewed_line(view: &GitView, theme: &Theme) -> Option<gpui::Stateful<gpui::Div>> {
    let (viewed, listed) = view.viewed_counter()?;
    Some(
        div()
            .id("git-viewed-count")
            .px(px(space::M))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .tooltip(tooltip(
                theme,
                "Files you marked as viewed; a mark clears when the file's diff changes",
                "",
            ))
            .child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_subtle))
                    .child(format!("Viewed {viewed}/{listed}")),
            )
            .when(viewed == listed, |el| {
                el.child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.accent))
                        .child("✓"),
                )
            }),
    )
}

/// The chevron's column (group header, History commit) as a spacer, so rows listed under them
/// (the Staged hint, a commit's files and notes) start where the header's text does.
fn under_commit() -> gpui::Div {
    div().flex_none().w(theme::rems_px(theme::ICON_SIZE_SMALL))
}
