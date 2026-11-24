//! Files view rendering: the tree rows and its popovers (the file pane is `file_pane.rs`).

use crate::*;

impl Shell {
    /// The right panel's Files tab (SPEC §12A): search, Ignored, the worktree chooser, a
    /// "New file…" field and the lazy tree with git markers.
    pub(crate) fn render_file_tree(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let empty = |sentence: &str, detail: Option<String>| {
            message(
                theme,
                None,
                sentence.to_owned().into(),
                detail.map(Into::into),
            )
            .flex_1()
        };
        let Some(view) = self.files.get(&project_id) else {
            return empty("Loading files…", None).into_any_element();
        };
        let ignored_keys = self.keymap.label(Command::ToggleIgnored);
        let showing_ignored = view.show_ignored;
        let ignored_tip = if showing_ignored {
            "Showing git-ignored files: click to hide them"
        } else {
            "Show git-ignored files"
        };
        let search_keys = self.keymap.label(Command::QuickOpen);
        // A button that opens the palette on its Files scope, framed like the panel's flat
        // fields (ADR 0110). The shortcut is in the tooltip: the narrow panel has no room for it.
        let search = search_box(
            div()
                .id("files-search-label")
                .truncate()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_subtle))
                .child("Search files…"),
            None,
            false,
            theme,
        )
        .id("files-search")
        .flex_1()
        .min_w_0()
        .h(theme::rems_px(theme::CONTROL_HEIGHT))
        .border_color(rgb(theme.divider))
        .pressable(Look::Chip, true, theme)
        .tooltip(tooltip(theme, "Find a file of this workspace", search_keys))
        .on_press(
            true,
            cx.listener(|shell, _, window, cx| {
                shell.open_palette_on(palette::Scope::Files, window, cx);
            }),
        );
        // An eye toggle (accent while ignored files show), not a text button: Tern's header
        // holds only the field and icons.
        let ignored = icon_button_in(
            "files-ignored",
            if showing_ignored {
                Icon::Eye
            } else {
                Icon::EyeOff
            },
            if showing_ignored {
                theme.accent
            } else {
                theme.text_muted
            },
            theme,
        )
        .tooltip(tooltip(theme, ignored_tip, ignored_keys.clone()))
        .on_click(cx.listener(move |shell, _, window, cx| {
            let requests = shell.files_view(project_id).toggle_ignored();
            shell.files_requests(project_id, requests, window, cx);
        }));
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .pb(px(space::S))
            .child(search)
            .child(ignored);
        let tree = match view.root() {
            files_view::Listing::Loading => empty("Loading files…", None).into_any_element(),
            files_view::Listing::Failed(error) => {
                empty("Cannot list the files.", Some(error.clone()))
                    .child(
                        button("files-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| shell.refresh_files(project_id, window, cx),
                        )),
                    )
                    .into_any_element()
            }
            files_view::Listing::Loaded(entries) if entries.is_empty() && view.show_ignored => {
                empty("This workspace has no files yet.", None).into_any_element()
            }
            files_view::Listing::Loaded(entries) if entries.is_empty() => {
                empty("No files here yet (ignored files are hidden).", None)
                    .child(
                        button("files-empty-ignored", "Show ignored", theme, true)
                            .tooltip(tooltip(
                                theme,
                                "Show git-ignored files",
                                ignored_keys.clone(),
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                let requests = shell.files_view(project_id).toggle_ignored();
                                shell.files_requests(project_id, requests, window, cx);
                            })),
                    )
                    .into_any_element()
            }
            files_view::Listing::Loaded(_) => {
                let rows = view.rows();
                let cursor = view.cursor.clone();
                let task = view.worktree_task_id;
                // Only a file of the folder the tree shows is the tree's open file.
                let opened = (self.focused_file(window))
                    .filter(|file| file.task == task)
                    .map(|file| file.path);
                let focused = self.files_focus.is_focused(window);
                let row_theme = theme.clone();
                uniform_list(
                    "file-tree",
                    rows.len(),
                    cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| rows.get(index))
                            .map(|row| {
                                let shown = FileRowState {
                                    cursor: cursor.as_deref(),
                                    opened: opened.as_deref(),
                                    task,
                                    focused,
                                };
                                file_row(project_id, row, &shown, &row_theme, cx)
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.files_scroll)
                .flex_1()
                .min_h_0()
                .px(px(space::S))
                .into_any_element()
            }
        };
        div()
            .id("files-tree")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .py(px(space::S))
            .key_context(FILE_TREE_CONTEXT)
            .track_focus(&self.files_focus)
            .on_key_down(cx.listener(Self::files_key_down))
            // Right click on the header or the blank part of the tree = the folder
            // itself (rows stop the event first).
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                    shell.open_file_menu(
                        project_id,
                        String::new(),
                        true,
                        event.position,
                        window,
                        cx,
                    );
                }),
            )
            .child(header)
            .children(
                self.target_chooser(project_id, view.worktree_task_id, false, theme, cx)
                    .map(|chooser| chooser.px(px(space::S)).pb(px(space::S))),
            )
            .children(self.render_new_entry(project_id, view.worktree_task_id, theme, cx))
            .child(tree)
            .into_any_element()
    }

    /// "New file…" / "New folder…": where it goes, then the name field (Enter creates, Esc
    /// cancels). Shown over the tree of the view it was asked in.
    pub(crate) fn render_new_entry(
        &self,
        project_id: Ulid,
        target: Option<Ulid>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let renaming = self.renaming.as_ref()?;
        let RenameTarget::NewEntry(entry) = &renaming.target else {
            return None;
        };
        if entry.project_id != project_id || entry.target != target {
            return None;
        }
        let kind = if entry.folder { "folder" } else { "file" };
        let place = if entry.dir.is_empty() {
            String::new()
        } else {
            format!(" in {}/", entry.dir)
        };
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .px(px(space::S))
                .pb(px(space::S))
                // Mouse downs stay in the field: the blur would create a half-typed name.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(format!(
                            "New {kind}{place} · Enter to create, Esc to cancel"
                        )),
                )
                .child(self.render_rename(&renaming.draft, theme, cx)),
        )
    }
}

/// Whether the Docs comment margin shows next to `doc`: with a shown file, and while it is
/// re-read after a change (so an open draft keeps its field).
pub(crate) fn margin_shown(doc: &Doc) -> bool {
    matches!(doc, Doc::Shown(_) | Doc::Loading)
}

/// A file of the Docs list: its name, its folder below, and its open-comment count.
/// What a Files tree row needs to know about the view.
pub(crate) struct FileRowState<'a> {
    /// The tree cursor's path.
    pub(crate) cursor: Option<&'a str>,
    /// The opened file's path.
    pub(crate) opened: Option<&'a str>,
    /// The folder the tree shows: the task worktree, `None` = the project's.
    pub(crate) task: Option<Ulid>,
    /// The tree has the keyboard (the cursor shows a focus ring).
    pub(crate) focused: bool,
}

/// Payload of a file tree row dragged onto a pane (ADR 0081): its path relative to the
/// tree's root.
#[derive(Debug, Clone)]
pub(crate) struct DraggedPath {
    pub(crate) project_id: Ulid,
    pub(crate) path: String,
    /// The folder the tree row is of (`FileKey::task`).
    pub(crate) task: Option<Ulid>,
    pub(crate) is_dir: bool,
    pub(crate) name: SharedString,
}

impl DraggedPath {
    /// The file this row opens.
    pub(crate) fn file(&self) -> FileKey {
        FileKey::new(self.path.clone(), self.task)
    }
}

/// One Files tree row: indented by depth, a chevron on folders, the name (muted when
/// ignored), and on the right the git marker letter (files) or a dot (folders with changes).
/// Every row is one body line high (the tree is a uniform list).
pub(crate) fn file_row(
    project_id: Ulid,
    row: &files_view::Row,
    state: &FileRowState<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let (entry, depth, expanded) = match row {
        files_view::Row::Note { depth, text } => {
            return div()
                .w_full()
                .h(theme::rems_px(theme::TREE_ROW_HEIGHT))
                .flex()
                .items_center()
                .pl(px(space::S + theme::FILE_TREE_INDENT * *depth as f32))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(text.clone())
                .into_any_element();
        }
        files_view::Row::Entry {
            entry,
            depth,
            expanded,
        } => (entry, *depth, *expanded),
    };
    let at_cursor = state.cursor == Some(entry.path.as_str());
    let opened = state.opened == Some(entry.path.as_str());
    let chevron = match (entry.is_dir, expanded) {
        (false, _) => None,
        (true, false) => Some(Icon::ChevronRight),
        (true, true) => Some(Icon::ChevronDown),
    };
    let marker = entry.marker.map(|marker| match marker {
        GitMarker::Modified => ("M", theme.git_modified, "Modified"),
        GitMarker::Added => ("A", theme.diff_add_text, "Added"),
        GitMarker::Deleted => ("D", theme.diff_del_text, "Deleted"),
        GitMarker::Untracked => ("?", theme.diff_add_text, "Untracked"),
    });
    let path = entry.path.clone();
    let style = PopupStyle::of(theme);
    let accent = theme.accent;
    let (menu_path, is_dir) = (entry.path.clone(), entry.is_dir);
    // The border is always there (so rows never shift); it only shows as the focus ring.
    let ring: gpui::Hsla = if at_cursor && state.focused {
        rgb(theme.pane_focus_stripe).into()
    } else {
        gpui::transparent_black()
    };
    div()
        .id(SharedString::from(format!("file-{}", entry.path)))
        .w_full()
        .h(theme::rems_px(theme::TREE_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::XS))
        .pl(px(space::S + theme::FILE_TREE_INDENT * depth as f32))
        .pr(px(space::S))
        .rounded(px(radius::CHIP))
        .border(px(theme::BORDER_WIDTH))
        .border_color(ring)
        .text_size(font_size::BODY)
        .when(at_cursor || opened, |el| el.bg(rgb(theme.selected)))
        .pressable_unless(at_cursor || opened, Look::Row, theme)
        .when(at_cursor, |el| {
            el.relative()
                .child(record_bounds(Anchor::FileRow(entry.path.clone()), cx))
        })
        .child(
            div()
                .flex_none()
                .w(theme::rems_px(theme::TREE_CHEVRON_SIZE))
                .children(
                    chevron.map(|icon| icon_svg(icon, theme::TREE_CHEVRON_SIZE, theme.text_subtle)),
                ),
        )
        .child(if entry.is_dir {
            icon_svg(Icon::Folder, theme::TREE_ICON_SIZE, theme.folder)
        } else {
            icon_svg(Icon::File, theme::TREE_ICON_SIZE, theme.text_subtle)
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(if entry.ignored || entry.name.starts_with('.') {
                    theme.text_disabled
                } else {
                    theme.text
                }))
                .when(opened, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(entry.name.clone()),
        )
        .children(marker.map(|(letter, color, what)| {
            div()
                .id(SharedString::from(format!("file-marker-{}", entry.path)))
                .flex_none()
                .text_size(font_size::OVERLINE)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(color))
                .tooltip(tooltip(theme, what, ""))
                .child(letter)
        }))
        .when(entry.is_dir && entry.has_changes, |el| {
            el.child(
                div()
                    .id(SharedString::from(format!("file-changes-{}", entry.path)))
                    .flex_none()
                    .size(px(theme::FILE_CHANGES_DOT_SIZE))
                    .rounded_full()
                    .bg(rgb(theme.git_modified))
                    .tooltip(tooltip(theme, "Contains changes", "")),
            )
        })
        .on_drag(
            DraggedPath {
                project_id,
                path: entry.path.clone(),
                task: state.task,
                is_dir: entry.is_dir,
                name: entry.name.clone().into(),
            },
            move |dragged, offset, _, cx| {
                cx.new(|_| DragChip {
                    style: style.clone(),
                    accent,
                    title: dragged.name.clone(),
                    offset,
                })
            },
        )
        .on_press(
            true,
            cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                let double = event.click_count() >= 2;
                shell.click_file(project_id, &path, double, window, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                shell.open_file_menu(
                    project_id,
                    menu_path.clone(),
                    is_dir,
                    event.position,
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
}

/// A file size for people: bytes, KB or MB (1024-based, one decimal).
pub(crate) fn size_label(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let bytes_f = bytes as f64;
    if bytes == 1 {
        "1 byte".to_owned()
    } else if bytes_f < KB {
        format!("{bytes} bytes")
    } else if bytes_f < KB * KB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{:.1} MB", bytes_f / (KB * KB))
    }
}
