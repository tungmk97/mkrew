//! Files view rendering: tree rows, the file pane and the editor.

use crate::*;

impl Shell {
    /// The workspace's files (SPEC §12A): the lazy tree with git markers, and the opened file
    /// read-only with line numbers and syntax colors.
    pub(crate) fn render_files(
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
        let (ignored_label, ignored_tip) = if view.show_ignored {
            ("Hide ignored", "Hide git-ignored files")
        } else {
            ("Show ignored", "Show git-ignored files")
        };
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .pb(px(space::XS))
            .child(
                div()
                    .id("files-search")
                    .flex_1()
                    .min_w_0()
                    .cursor_pointer()
                    .child(text_field(
                        "files-search-field",
                        "",
                        "Search files…",
                        false,
                        false,
                        theme,
                    ))
                    .tooltip(tooltip(
                        theme,
                        "Find a file of this workspace",
                        self.keymap.label(Command::QuickOpen),
                    ))
                    .on_click(
                        cx.listener(|shell, _, window, cx| shell.open_quick_open(window, cx)),
                    ),
            )
            .child(
                button("files-ignored", ignored_label, theme, false)
                    .flex_none()
                    .tooltip(tooltip(theme, ignored_tip, ignored_keys.clone()))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        let requests = shell.files_view(project_id).toggle_ignored();
                        shell.files_requests(project_id, requests, window, cx);
                    })),
            );
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
                let opened = view.file.clone();
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
            .flex_1()
            .min_h_0()
            .flex()
            .font_family(theme.ui_font_family.clone())
            .child(
                div()
                    .id("files-tree")
                    .flex_none()
                    .w(px(theme::FILE_TREE_WIDTH))
                    .h_full()
                    .flex()
                    .flex_col()
                    .pb(px(space::S))
                    .track_focus(&self.files_focus)
                    .on_key_down(cx.listener(Self::files_key_down))
                    // Right click on the header or the blank part of the tree = the folder
                    // itself (rows stop the event first).
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |shell, event: &gpui::MouseDownEvent, _, cx| {
                            shell.open_file_menu(
                                project_id,
                                String::new(),
                                true,
                                event.position,
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
                    .child(tree),
            )
            .child(self.render_file(project_id, view, window, theme, cx))
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

    /// The opened file: its path (with `●` while unsaved), Save, Review in Docs (Markdown) and
    /// Open in Zed, then the editor, or why it is not shown.
    pub(crate) fn render_file(
        &self,
        project_id: Ulid,
        view: &FilesView,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let panel = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .mr(px(space::S))
            .mb(px(space::S))
            .rounded(px(radius::MEDIUM))
            .overflow_hidden()
            .bg(rgb(theme.surface));
        let Some(path) = view.file.clone() else {
            return panel.child(
                message(
                    theme,
                    None,
                    "Select a file to view it.".into(),
                    Some(format!(
                        "Use ↑ ↓ to move, → to open a folder, Enter to open a file · {} to find a file.",
                        self.keymap.label(Command::QuickOpen)
                    ).into()),
                )
                .flex_1(),
            );
        };
        let dirty = view.unsaved().is_some();
        let editable = view
            .shown()
            .is_some_and(|shown| !shown.code.binary && !shown.code.too_large);
        let zed_keys = self.keymap.label(Command::OpenInZed);
        let zed_tip = match view.line() {
            Some(line) => format!("Open this file in Zed at line {line}"),
            None => "Open this file in Zed".to_owned(),
        };
        let mode_toggle = (view.is_markdown() && editable).then(|| {
            self.mode_toggle(
                "file-mode",
                view.mode,
                [(Mode::Preview, "Preview"), (Mode::Source, "Source")],
                "Toggle preview / source",
                "".into(),
                move |shell, _, window, cx| {
                    shell.files_view(project_id).toggle_mode();
                    shell.build_preview(project_id, window, cx);
                    // The find fields are not drawn in Preview: give the keyboard back.
                    if shell.files_view(project_id).previewing()
                        && (shell.file_find_focus.is_focused(window)
                            || shell.file_replace_focus.is_focused(window))
                    {
                        window.focus(&shell.focus_handle, cx);
                    }
                    cx.notify();
                },
                theme,
                cx,
            )
        });
        let zed_button = |id: &'static str, primary: bool| {
            button(id, "Open in Zed", theme, primary)
                .flex_none()
                .tooltip(tooltip(theme, &zed_tip, zed_keys.clone()))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    shell.open_file_in_zed(project_id, cx);
                }))
        };
        let markdown = view.is_markdown();
        let review_path = path.clone();
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
                    .child(format!("{}{path}", if dirty { "● " } else { "" })),
            )
            .children(view.shown().filter(|_| editable).and_then(|shown| {
                let at = shown.editor.cursor();
                // The caret is always inside the text (the editor clamps it).
                let line = shown.editor.lines().get(at.line)?;
                let col = editor::display_col(line, at.col);
                Some(
                    div()
                        .flex_none()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(format!("Ln {}, Col {}", at.line + 1, col + 1)),
                )
            }))
            .children(mode_toggle)
            // The Docs view reviews the project folder's files, not a worktree's.
            .when(markdown && view.worktree_task_id.is_none(), |el| {
                el.child(
                    button("file-review", "Review in Docs", theme, false)
                        .flex_none()
                        .tooltip(tooltip(theme, "Review and comment on this file", ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.review_in_docs(project_id, review_path.clone(), window, cx);
                        })),
                )
            })
            .when(editable, |el| {
                el.child(
                    button("file-save", "Save", theme, dirty)
                        .flex_none()
                        .tooltip(tooltip(
                            theme,
                            "Save this file",
                            self.keymap.label(Command::SaveFile),
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.save_file(project_id, None, window, cx);
                        })),
                )
            })
            .child(
                button("file-find", "Find", theme, false)
                    .flex_none()
                    .tooltip(tooltip(
                        theme,
                        "Find in this file",
                        self.keymap.label(Command::Find),
                    ))
                    .on_click(
                        cx.listener(|shell, _, window, cx| shell.open_file_find(false, window, cx)),
                    ),
            )
            .child(zed_button("file-zed", false));
        let find_bar = view
            .find
            .as_ref()
            .filter(|_| !view.previewing())
            .map(|find| {
                let focused = self.file_find_focus.is_focused(window);
                let find_row = div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::L))
                    .pb(px(space::S))
                    .child(
                        button(
                            "file-find-toggle-replace",
                            if find.replace.is_some() { "▾" } else { "▸" },
                            theme,
                            false,
                        )
                        .tooltip(tooltip(
                            theme,
                            "Toggle replace",
                            self.keymap.label(Command::ReplaceInFile),
                        ))
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                let shown = (shell.files_view(project_id).find.as_ref())
                                    .is_some_and(|find| find.replace.is_some());
                                if !shown {
                                    return shell.open_file_find(true, window, cx);
                                }
                                if let Some(find) = &mut shell.files_view(project_id).find {
                                    find.replace = None;
                                }
                                window.focus(&shell.file_find_focus, cx);
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        text_field(
                            "file-find-field",
                            &find.query,
                            "Find in file",
                            focused,
                            find.query.all_selected(),
                            theme,
                        )
                        .w(px(theme::FIND_INPUT_WIDTH))
                        .track_focus(&self.file_find_focus)
                        .on_key_down(cx.listener(Self::file_find_key_down))
                        .relative()
                        .child(self.typing(&self.file_find_focus, Self::file_find_key_down, cx))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            window.focus(&shell.file_find_focus, cx);
                            cx.notify();
                        })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(find.label()),
                    )
                    .child(
                        button("file-find-previous", "↑", theme, false)
                            .tooltip(tooltip(theme, "Previous match", "Shift+Enter"))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.file_find_focus, cx);
                                shell.step_file_find(project_id, false, cx);
                            })),
                    )
                    .child(
                        button("file-find-next", "↓", theme, false)
                            .tooltip(tooltip(theme, "Next match", "Enter"))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.file_find_focus, cx);
                                shell.step_file_find(project_id, true, cx);
                            })),
                    )
                    .child(
                        button("file-find-close", "✕", theme, false)
                            .tooltip(tooltip(theme, "Close", "Esc"))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.close_file_find(project_id, window, cx);
                            })),
                    );
                let replacing = self.file_replace_focus.is_focused(window);
                let replace_row = find.replace.as_ref().map(|with| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .px(px(space::L))
                        .pb(px(space::S))
                        .child(
                            text_field(
                                "file-replace-field",
                                with,
                                "Replace",
                                replacing,
                                with.all_selected(),
                                theme,
                            )
                            .w(px(theme::FIND_INPUT_WIDTH))
                            .track_focus(&self.file_replace_focus)
                            .on_key_down(cx.listener(Self::file_replace_key_down))
                            .relative()
                            .child(self.typing(
                                &self.file_replace_focus,
                                Self::file_replace_key_down,
                                cx,
                            ))
                            .on_click(cx.listener(
                                |shell, _, window, cx| {
                                    window.focus(&shell.file_replace_focus, cx);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            button("file-replace-one", "Replace", theme, false)
                                .tooltip(tooltip(
                                    theme,
                                    "Replace this match and go to the next",
                                    "Enter",
                                ))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    window.focus(&shell.file_replace_focus, cx);
                                    shell.replace_in_file(project_id, window, cx);
                                })),
                        )
                        .child(
                            button("file-replace-all", "Replace all", theme, false)
                                .tooltip(tooltip(theme, "Replace every match", "Ctrl+Alt+Enter"))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    window.focus(&shell.file_replace_focus, cx);
                                    shell.replace_all_in_file(project_id, window, cx);
                                })),
                        )
                });
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .child(find_row)
                    .children(replace_row)
            });
        let body = match &view.content {
            Content::None | Content::Loading => message(theme, None, "Loading…".into(), None)
                .flex_1()
                .into_any_element(),
            Content::Failed(error) => message(
                theme,
                None,
                "Cannot read this file.".into(),
                Some(error.clone().into()),
            )
            .flex_1()
            .child(
                button("file-retry", "Retry", theme, true).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        let read = shell.files_view(project_id).open(&path);
                        let requests = Requests { list: vec![], read };
                        shell.files_requests(project_id, requests, window, cx);
                    },
                )),
            )
            .into_any_element(),
            Content::Shown(shown) if shown.code.binary || shown.code.too_large => {
                let sentence = if shown.code.binary {
                    "This is a binary file."
                } else {
                    "This file is too large to show here."
                };
                message(
                    theme,
                    None,
                    sentence.into(),
                    Some(
                        format!("{} — open it in Zed instead.", size_label(shown.code.size)).into(),
                    ),
                )
                .flex_1()
                .child(zed_button("file-zed-notice", true))
                .into_any_element()
            }
            Content::Shown(_) if view.previewing() => {
                self.render_md_preview(view, cx.global::<theme::Fonts>(), theme)
            }
            Content::Shown(shown) => self.render_editor(project_id, shown, window, theme, cx),
        };
        let disk_banner = view
            .shown()
            .filter(|shown| shown.disk_changed.is_some())
            .map(|_| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::L))
                    .pb(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text))
                            .child("⚠ Changed on disk — Reload · Keep mine"),
                    )
                    .child(
                        button("file-reload", "Reload", theme, false)
                            .tooltip(tooltip(
                                theme,
                                "Show the file on disk; your changes are dropped",
                                "",
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                let read = shell.files_view(project_id).reload();
                                let requests = files_view::Requests { list: vec![], read };
                                shell.files_requests(project_id, requests, window, cx);
                            })),
                    )
                    .child(
                        button("file-keep-mine", "Keep mine", theme, true)
                            .tooltip(tooltip(
                                theme,
                                "Keep your text; Save overwrites the file on disk",
                                "",
                            ))
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                shell.files_view(project_id).keep_mine();
                                cx.notify();
                            })),
                    )
            });
        panel
            .child(header)
            .children(find_bar)
            .children(disk_banner)
            .children(view.zed_error.clone().map(|error| {
                notice(theme.text, format!("✕ {error}"))
                    .px(px(space::L))
                    .pb(px(space::S))
            }))
            .children(
                view.shown()
                    .and_then(|shown| shown.save_error.clone())
                    .map(|error| {
                        notice(theme.text, format!("✕ {error}"))
                            .px(px(space::L))
                            .pb(px(space::S))
                    }),
            )
            .child(body)
    }

    /// The Files editor (§12A, ADR 0028): the lines with the caret, selection, syntax colors
    /// and find matches. A click places the caret; Shift+click and drag select.
    pub(crate) fn render_editor(
        &self,
        project_id: Ulid,
        shown: &files_view::Shown,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = self.file_editor_focus.is_focused(window);
        let cell_width = mkrew_ui::terminal::view::cell_size(window, theme).0;
        let total = shown.editor.lines().len();
        // Widest row in screen cells, kept by the editor per text change.
        let longest = Some(shown.editor.longest_line());
        let text_left = Rc::clone(&self.file_text_left);
        let list = uniform_list(
            "file-code",
            total,
            cx.processor(move |shell, range: std::ops::Range<usize>, _, cx| {
                let Some(view) = shell.files.get(&project_id) else {
                    return Vec::new();
                };
                let Some(shown) = view.shown() else {
                    return Vec::new();
                };
                let theme = cx.theme();
                let (editor, code) = (&shown.editor, &shown.code);
                let hits = view.find.as_ref().map(TextFind::hits);
                let (cursor, selection) = (editor.cursor(), editor.selection());
                range
                    .filter_map(|index| {
                        let line = editor.lines().get(index)?;
                        let text = editor::display_line(line);
                        // Spans of an older text could point past the line: only when it matches.
                        let fresh = code.lines.get(index).is_some_and(|old| *old == *text);
                        let spans = (code.spans.get(index))
                            .filter(|_| fresh)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]);
                        let found = (hits.as_ref())
                            .map(|hits| hits.on_line(index))
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(range, current)| {
                                let start = editor::display_byte(line, range.start);
                                (start..editor::display_byte(line, range.end), current)
                            })
                            .collect();
                        let marks = preview::RowMarks {
                            caret: (focused && cursor.line == index)
                                .then(|| editor::display_col(line, cursor.col)),
                            selected: (selection.as_ref()).and_then(|selection| {
                                editor::selected_cells(line, index, selection)
                            }),
                            found,
                        };
                        let row = preview::code_row(
                            index,
                            total,
                            text.into_owned(),
                            spans,
                            &marks,
                            cell_width,
                            &text_left,
                            theme,
                        );
                        Some(
                            row.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                                    let x = event.position.x;
                                    let extend = event.modifiers.shift;
                                    shell.click_file_text(project_id, index, x, extend, window, cx);
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                move |shell, event: &MouseMoveEvent, window, cx| {
                                    let dragging = event.pressed_button == Some(MouseButton::Left)
                                        && shell.file_editor_focus.is_focused(window);
                                    if dragging {
                                        let x = event.position.x;
                                        shell.click_file_text(
                                            project_id, index, x, true, window, cx,
                                        );
                                    }
                                },
                            ))
                            .into_any_element(),
                        )
                    })
                    .collect()
            }),
        )
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(longest)
        .track_scroll(&self.file_code_scroll)
        .flex_1()
        .min_h_0()
        .px(px(space::L))
        .pb(px(space::L))
        .font_family(theme.terminal_font_family.clone())
        .text_size(px(theme.terminal_font_size));
        div()
            .id("file-editor")
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .track_focus(&self.file_editor_focus)
            .on_key_down(cx.listener(Self::file_editor_key_down))
            // Below the last line too: a click there gives the editor the keyboard.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _: &MouseDownEvent, window, cx| {
                    window.focus(&shell.file_editor_focus, cx);
                    cx.notify();
                }),
            )
            .child(self.typing(&self.file_editor_focus, Self::file_editor_key_down, cx))
            .child(list)
            .child(scrollbar::bar(
                scrollbar::Source::Handle(self.file_code_scroll.0.borrow().base_handle.clone()),
                Rc::clone(&self.file_code_drag),
                theme,
            ))
            .into_any_element()
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
    /// The tree has the keyboard (the cursor shows a focus ring).
    pub(crate) focused: bool,
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
                .pl(px(space::S + theme::FILE_TREE_INDENT * *depth as f32))
                .py(px(space::XS))
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
    let hover = theme.surface_raised;
    let fill = if at_cursor || opened {
        theme.surface_raised
    } else {
        theme.bg
    };
    let chevron = match (entry.is_dir, expanded) {
        (false, _) => "",
        (true, false) => "▸",
        (true, true) => "▾",
    };
    let marker = entry.marker.map(|marker| match marker {
        GitMarker::Modified => ("M", theme.accent, "Modified"),
        GitMarker::Added => ("A", theme.diff_add_text, "Added"),
        GitMarker::Deleted => ("D", theme.diff_del_text, "Deleted"),
        GitMarker::Untracked => ("?", theme.diff_add_text, "Untracked"),
    });
    let path = entry.path.clone();
    let (menu_path, is_dir) = (entry.path.clone(), entry.is_dir);
    div()
        .id(SharedString::from(format!("file-{}", entry.path)))
        .w_full()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .pl(px(space::S + theme::FILE_TREE_INDENT * depth as f32))
        .pr(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        // The border is always there (so rows never shift); it only shows as the focus ring.
        .border_color(rgb(if at_cursor && state.focused {
            theme.pane_focus_border
        } else {
            fill
        }))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(at_cursor || opened, |el| el.bg(rgb(fill)))
        .when(!at_cursor && !opened, |el| {
            el.hover(move |style| style.bg(rgb(hover)))
        })
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(
            div()
                .flex_none()
                .w(px(space::M))
                .text_color(rgb(theme.text_muted))
                .child(chevron),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(if entry.ignored {
                    theme.text_muted
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
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
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
                    .bg(rgb(theme.accent))
                    .tooltip(tooltip(theme, "Contains changes", "")),
            )
        })
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.click_file(project_id, &path, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &gpui::MouseDownEvent, _, cx| {
                shell.open_file_menu(project_id, menu_path.clone(), is_dir, event.position, cx);
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
