//! The pane of an open file (SPEC §12A): header, find bar, editor, preview or split.

use crate::*;

impl Shell {
    /// The pane of an open file (SPEC §12A): the header (a breadcrumb, `Ln, Col`, Markdown
    /// modes, icon buttons Review, Save with a dot while unsaved, Find, Open in Zed, close),
    /// the find bar, then the editor, the preview or the split, or why it is not shown.
    /// Lives in `pane_frame`.
    pub(crate) fn render_file_pane(
        &self,
        project_id: Ulid,
        path: &FileKey,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (Some(doc), Some(view)) = (self.doc(project_id, path), self.doc_view(project_id, path))
        else {
            // The doc is made by the next `sync_panes`, which also starts its read.
            return message(theme, None, "Loading…".into(), None)
                .size_full()
                .into_any_element();
        };
        let panel = div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family(theme.ui_font_family.clone())
            .bg(rgb(theme.surface));
        let path = path.clone();
        let dirty = doc.is_dirty();
        let editable = doc
            .shown()
            .is_some_and(|shown| !shown.code.binary && !shown.code.too_large);
        let zed_keys = self.keymap.label(Command::OpenInZed);
        let zed_tip = match doc.line() {
            Some(line) => format!("Open this file in Zed at line {line}"),
            None => "Open this file in Zed".to_owned(),
        };
        let mode_toggle = (doc.is_markdown() && editable).then(|| {
            let preview_keys: SharedString = self.keymap.label(Command::ToggleFilePreview).into();
            let split_keys: SharedString = self.keymap.label(Command::ToggleFileSplit).into();
            // Icons: three text segments would squeeze the file name out of a narrow header.
            let segment = |mode, label, icon, tip, keys| crate::shell::docs::Segment {
                mode,
                label,
                icon: Some(icon),
                tip,
                keys,
            };
            self.mode_toggle(
                "file-mode",
                doc.mode,
                [
                    segment(
                        MdMode::Preview,
                        "Preview",
                        Icon::Eye,
                        "Preview: the rendered page",
                        preview_keys.clone(),
                    ),
                    segment(
                        MdMode::Split,
                        "Split",
                        Icon::Columns2,
                        "Split: source and live preview side by side",
                        split_keys,
                    ),
                    segment(
                        MdMode::Source,
                        "Source",
                        Icon::Code,
                        "Source: the Markdown text",
                        preview_keys,
                    ),
                ],
                {
                    let path = path.clone();
                    move |shell, mode, window, cx| {
                        shell.set_file_mode(project_id, &path, mode, window, cx);
                    }
                },
                theme,
                cx,
            )
        });
        let markdown = doc.is_markdown();
        let worktree = worktree_target::label(
            path.task,
            project_id,
            self.board.tasks(),
            self.board.is_loaded(),
        );
        let review_path = path.path.clone();
        let (folders, name) = crumbs(&path.path);
        // A quiet icon button with its tooltip and keys.
        let tool = |id: &'static str, icon: Icon, tip: &str, keys: String| {
            icon_button(id, icon, theme).tooltip(tooltip(theme, tip, keys))
        };
        let save_tip = if dirty {
            "Save this file (it has unsaved changes)"
        } else {
            "Save this file"
        };
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
                    .min_w(px(theme::FILE_NAME_MIN_WIDTH))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(space::XS))
                            // The folders are cut on their left side first, the name last.
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap(px(space::XS))
                                    .children(folders.flat_map(|segment| {
                                        [
                                            crumb(segment.to_owned(), theme.text_subtle),
                                            crumb("›", theme.subtle),
                                        ]
                                    })),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(font_size::BODY)
                                    .font_weight(FontWeight(weight::MEDIUM))
                                    .text_color(rgb(theme.text))
                                    .child(name.to_owned()),
                            ),
                    )
                    // A worktree's file says which worktree: it looks like the project's otherwise.
                    // `flex_1` (basis 0): a long title is cut first, the path only when it alone does not fit.
                    .children(worktree.map(|worktree| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_subtle))
                            .child(worktree)
                    })),
            )
            // The buttons give way first: in a narrow pane the left ones are cut, the name stays.
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::XS))
                    .min_w_0()
                    .overflow_hidden()
                    .children(doc.shown().filter(|_| editable).and_then(|shown| {
                        let at = shown.editor.cursor();
                        // The caret is always inside the text (the editor clamps it).
                        let line = shown.editor.lines().get(at.line)?;
                        let col = editor::display_col(line, at.col);
                        // The terminal font: its digits are all as wide (tabular).
                        Some(
                            div()
                                .flex_none()
                                .mr(px(space::S))
                                .font_family(theme.terminal_font_family.clone())
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_subtle))
                                .child(format!("Ln {}, Col {}", at.line + 1, col + 1)),
                        )
                    }))
                    .children(mode_toggle)
                    // The Docs view reviews the project folder's files, not a worktree's.
                    .when(
                        markdown && path.task.is_none() && self.features.docs,
                        |el| {
                            el.child(
                                tool(
                                    "file-review",
                                    Icon::FileText,
                                    "Review in Docs: review and comment on this file",
                                    String::new(),
                                )
                                .on_click(cx.listener(
                                    move |shell, _, window, cx| {
                                        shell.review_in_docs(
                                            project_id,
                                            review_path.clone(),
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            )
                        },
                    )
                    .when(editable, |el| {
                        el.child(
                            tool(
                                "file-save",
                                Icon::Save,
                                save_tip,
                                self.keymap.label(Command::SaveFile),
                            )
                            .relative()
                            // Unsaved: a dot on the icon's top right corner.
                            .when(dirty, |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .top(px(space::XS))
                                        .right(px(space::XS))
                                        .size(px(theme::FILE_CHANGES_DOT_SIZE))
                                        .rounded_full()
                                        .bg(rgb(theme.accent)),
                                )
                            })
                            .on_click({
                                let path = path.clone();
                                cx.listener(move |shell, _, window, cx| {
                                    shell.save_file(project_id, path.clone(), None, window, cx);
                                })
                            }),
                        )
                    })
                    .child(
                        tool(
                            "file-find",
                            Icon::Search,
                            "Find in this file",
                            self.keymap.label(Command::Find),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.open_file_find(false, window, cx)
                        })),
                    )
                    .child(
                        tool("file-zed", Icon::ExternalLink, &zed_tip, zed_keys.clone()).on_click(
                            {
                                let path = path.clone();
                                cx.listener(move |shell, _, _, cx| {
                                    shell.open_file_in_zed(project_id, &path, cx);
                                })
                            },
                        ),
                    )
                    .child(
                        icon_button("file-close", Icon::X, theme)
                            .tooltip(tooltip(
                                theme,
                                "Close file",
                                self.keymap.label(Command::CloseFileTab),
                            ))
                            .on_click({
                                let path = path.clone();
                                cx.listener(move |shell, _, window, cx| {
                                    shell.close_file_pane(project_id, path.clone(), window, cx);
                                })
                            }),
                    ),
            );
        let find_bar = doc.find.as_ref().filter(|_| !doc.previewing()).map(|find| {
            let focused = view.find_focus.is_focused(window);
            let find_row = div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::L))
                .pb(px(space::S))
                .child(
                    icon_button(
                        "file-find-toggle-replace",
                        if find.replace.is_some() {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        },
                        theme,
                    )
                    .tooltip(tooltip(
                        theme,
                        "Toggle replace",
                        self.keymap.label(Command::ReplaceInFile),
                    ))
                    .on_click({
                        let path = path.clone();
                        cx.listener(move |shell, _, window, cx| {
                            let shown = (shell.doc(project_id, &path))
                                .and_then(|doc| doc.find.as_ref())
                                .is_some_and(|find| find.replace.is_some());
                            if !shown {
                                return shell.open_file_find(true, window, cx);
                            }
                            if let Some(find) =
                                (shell.doc_mut(project_id, &path)).and_then(|doc| doc.find.as_mut())
                            {
                                find.replace = None;
                            }
                            shell.focus_doc(project_id, &path, DocFocus::Find, window, cx);
                            cx.notify();
                        })
                    }),
                )
                .child(
                    caret_field(
                        "file-find-field",
                        &find.query,
                        "Find in file",
                        focused,
                        theme,
                        cx,
                        {
                            let path = path.clone();
                            move |shell, caret| {
                                if let Some(find) = (shell.doc_mut(project_id, &path))
                                    .and_then(|doc| doc.find.as_mut())
                                {
                                    find.query.click(caret.at, caret.extend, caret.clicks);
                                }
                            }
                        },
                    )
                    .w(px(theme::FIND_INPUT_WIDTH))
                    .track_focus(&view.find_focus)
                    .on_key_down(cx.listener(Self::file_find_key_down))
                    .relative()
                    .child(self.typing(&view.find_focus, Self::file_find_key_down, cx))
                    .on_click({
                        let path = path.clone();
                        cx.listener(move |shell, _, window, cx| {
                            shell.focus_doc(project_id, &path, DocFocus::Find, window, cx);
                            cx.notify();
                        })
                    }),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(find.label()),
                )
                .child(
                    icon_button("file-find-previous", Icon::ArrowUp, theme)
                        .tooltip(tooltip(theme, "Previous match", "Shift+Enter"))
                        .on_click({
                            let path = path.clone();
                            cx.listener(move |shell, _, window, cx| {
                                shell.focus_doc(project_id, &path, DocFocus::Find, window, cx);
                                shell.step_file_find(project_id, &path, false, cx);
                            })
                        }),
                )
                .child(
                    icon_button("file-find-next", Icon::ArrowDown, theme)
                        .tooltip(tooltip(theme, "Next match", "Enter"))
                        .on_click({
                            let path = path.clone();
                            cx.listener(move |shell, _, window, cx| {
                                shell.focus_doc(project_id, &path, DocFocus::Find, window, cx);
                                shell.step_file_find(project_id, &path, true, cx);
                            })
                        }),
                )
                .child(
                    icon_button("file-find-close", Icon::X, theme)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click({
                            let path = path.clone();
                            cx.listener(move |shell, _, window, cx| {
                                shell.close_file_find(project_id, &path, window, cx);
                            })
                        }),
                );
            let replacing = view.replace_focus.is_focused(window);
            let replace_row = find.replace.as_ref().map(|with| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::L))
                    .pb(px(space::S))
                    .child(
                        caret_field(
                            "file-replace-field",
                            with,
                            "Replace",
                            replacing,
                            theme,
                            cx,
                            {
                                let path = path.clone();
                                move |shell, caret| {
                                    let find = (shell.doc_mut(project_id, &path))
                                        .and_then(|doc| doc.find.as_mut());
                                    if let Some(with) = find.and_then(|find| find.replace.as_mut())
                                    {
                                        with.click(caret.at, caret.extend, caret.clicks);
                                    }
                                }
                            },
                        )
                        .w(px(theme::FIND_INPUT_WIDTH))
                        .track_focus(&view.replace_focus)
                        .on_key_down(cx.listener(Self::file_replace_key_down))
                        .relative()
                        .child(self.typing(&view.replace_focus, Self::file_replace_key_down, cx))
                        .on_click({
                            let path = path.clone();
                            cx.listener(move |shell, _, window, cx| {
                                shell.focus_doc(project_id, &path, DocFocus::Replace, window, cx);
                                cx.notify();
                            })
                        }),
                    )
                    .child(
                        button("file-replace-one", "Replace", theme, false)
                            .tooltip(tooltip(
                                theme,
                                "Replace this match and go to the next",
                                "Enter",
                            ))
                            .on_click({
                                let path = path.clone();
                                cx.listener(move |shell, _, window, cx| {
                                    shell.focus_doc(
                                        project_id,
                                        &path,
                                        DocFocus::Replace,
                                        window,
                                        cx,
                                    );
                                    shell.replace_in_file(project_id, &path, window, cx);
                                })
                            }),
                    )
                    .child(
                        button("file-replace-all", "Replace all", theme, false)
                            .tooltip(tooltip(
                                theme,
                                "Replace every match",
                                mkrew_ui::os_text!("Ctrl+Alt+Enter", "⌥⌘↩"),
                            ))
                            .on_click({
                                let path = path.clone();
                                cx.listener(move |shell, _, window, cx| {
                                    shell.focus_doc(
                                        project_id,
                                        &path,
                                        DocFocus::Replace,
                                        window,
                                        cx,
                                    );
                                    shell.replace_all_in_file(project_id, &path, window, cx);
                                })
                            }),
                    )
            });
            div()
                .flex_none()
                .flex()
                .flex_col()
                .child(find_row)
                .children(replace_row)
        });
        let body = match &doc.content {
            documents::Content::Loading => message(theme, None, "Loading…".into(), None)
                .flex_1()
                .into_any_element(),
            documents::Content::Failed(_) if doc.removed() => {
                message(theme, None, "This file was removed.".into(), None)
                    .flex_1()
                    .child(
                        button("file-removed-close", "Close", theme, true).on_click({
                            let path = path.clone();
                            cx.listener(move |shell, _, window, cx| {
                                shell.close_file_pane(project_id, path.clone(), window, cx);
                            })
                        }),
                    )
                    .into_any_element()
            }
            documents::Content::Failed(error) => message(
                theme,
                None,
                "Cannot read this file.".into(),
                Some(error.clone().into()),
            )
            .flex_1()
            .child(button("file-retry", "Retry", theme, true).on_click({
                let path = path.clone();
                cx.listener(move |shell, _, window, cx| {
                    shell.retry_read(project_id, &path, window, cx);
                })
            }))
            .into_any_element(),
            documents::Content::Shown(shown) if shown.code.binary || shown.code.too_large => {
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
                        format!("{}. Open it in Zed instead.", size_label(shown.code.size)).into(),
                    ),
                )
                .flex_1()
                .child(
                    button(
                        "file-zed-notice",
                        icon_text(Icon::ExternalLink, "Open in Zed", button_fg(theme, true)),
                        theme,
                        true,
                    )
                    .flex_none()
                    .tooltip(tooltip(theme, &zed_tip, zed_keys.clone()))
                    .on_click({
                        let path = path.clone();
                        cx.listener(move |shell, _, _, cx| {
                            shell.open_file_in_zed(project_id, &path, cx)
                        })
                    }),
                )
                .into_any_element()
            }
            documents::Content::Shown(_) if doc.previewing() => {
                self.render_md_preview(project_id, doc, false, window, theme, cx)
            }
            documents::Content::Shown(shown) if doc.splitting() => {
                self.render_md_split(project_id, doc, shown, window, theme, cx)
            }
            documents::Content::Shown(shown) => {
                self.render_shown_file(project_id, doc, shown, window, theme, cx)
            }
        };
        let disk_banner = doc
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
                            .child("⚠ Changed on disk: Reload · Keep mine"),
                    )
                    .child(
                        button("file-reload", "Reload", theme, false)
                            .tooltip(tooltip(
                                theme,
                                "Show the file on disk; your changes are dropped",
                                "",
                            ))
                            .on_click({
                                let path = path.clone();
                                cx.listener(move |shell, _, window, cx| {
                                    shell.reload_file(project_id, &path, window, cx);
                                })
                            }),
                    )
                    .child(
                        button("file-keep-mine", "Keep mine", theme, true)
                            .tooltip(tooltip(
                                theme,
                                "Keep your text; Save overwrites the file on disk",
                                "",
                            ))
                            .on_click({
                                let path = path.clone();
                                cx.listener(move |shell, _, _, cx| {
                                    if let Some(doc) = shell.doc_mut(project_id, &path) {
                                        doc.keep_mine();
                                    }
                                    cx.notify();
                                })
                            }),
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
                doc.shown()
                    .and_then(|shown| shown.save_error.clone())
                    .map(|error| {
                        notice(theme.text, format!("✕ {error}"))
                            .px(px(space::L))
                            .pb(px(space::S))
                    }),
            )
            .child(body)
            .into_any_element()
    }

    /// The Files editor (§12A, ADR 0028): the lines with the caret, selection, syntax colors
    /// and find matches. A click places the caret; Shift+click and drag select.
    pub(crate) fn render_shown_file(
        &self,
        project_id: Ulid,
        doc: &documents::Doc,
        shown: &documents::Shown,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(view) = self.doc_view(project_id, &doc.file) else {
            return div().into_any_element();
        };
        let path = doc.file.clone();
        let path_list = path.clone();
        let focused = view.editor_focus.is_focused(window);
        let cell_width = mkrew_ui::terminal::view::cell_size(window, theme).0;
        let total = shown.editor.lines().len();
        // Widest row in screen cells, kept by the editor per text change.
        let longest = Some(shown.editor.longest_line());
        let text_left = Rc::clone(&view.text_left);
        let list = uniform_list(
            "file-code",
            total,
            cx.processor(move |shell, range: std::ops::Range<usize>, _, cx| {
                let Some(doc) = shell.doc(project_id, &path_list) else {
                    return Vec::new();
                };
                let Some(shown) = doc.shown() else {
                    return Vec::new();
                };
                let theme = cx.theme();
                let (editor, code) = (&shown.editor, &shown.code);
                let hits = doc.find.as_ref().map(TextFind::hits);
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
                            // Marks of an older text could sit on moved lines: only when it matches.
                            change: code.changes.get(index).copied().flatten().filter(|_| fresh),
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
                            row.on_mouse_down(MouseButton::Left, {
                                let path = path_list.clone();
                                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                                    let x = event.position.x;
                                    let extend = event.modifiers.shift;
                                    shell.click_file_text(
                                        project_id, &path, index, x, extend, window, cx,
                                    );
                                })
                            })
                            .on_mouse_move({
                                let path = path_list.clone();
                                cx.listener(move |shell, event: &MouseMoveEvent, window, cx| {
                                    let dragging = event.pressed_button == Some(MouseButton::Left)
                                        && shell.doc_focused(
                                            project_id,
                                            &path,
                                            DocFocus::Editor,
                                            window,
                                        );
                                    if dragging {
                                        let x = event.position.x;
                                        shell.click_file_text(
                                            project_id, &path, index, x, true, window, cx,
                                        );
                                    }
                                })
                            })
                            .into_any_element(),
                        )
                    })
                    .collect()
            }),
        )
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(longest)
        .track_scroll(&view.code_scroll)
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
            .track_focus(&view.editor_focus)
            .on_key_down(cx.listener(Self::file_editor_key_down))
            // Below the last line too: a click there gives the editor the keyboard.
            .on_mouse_down(MouseButton::Left, {
                let path = path.clone();
                cx.listener(move |shell, _: &MouseDownEvent, window, cx| {
                    shell.focus_doc(project_id, &path, DocFocus::Editor, window, cx);
                    cx.notify();
                })
            })
            .child(self.typing(&view.editor_focus, Self::file_editor_key_down, cx))
            .child(list)
            .child(scrollbar::bar(
                scrollbar::Source::Handle(view.code_scroll.0.borrow().base_handle.clone()),
                Rc::clone(&view.code_drag),
                theme,
            ))
            .into_any_element()
    }
}

/// A file's path (`/` separated, relative to its folder) as the header's breadcrumb: its
/// folder segments, then its name.
fn crumbs(path: &str) -> (impl Iterator<Item = &str>, &str) {
    let (folders, name) = path.rsplit_once('/').unwrap_or(("", path));
    (
        folders.split('/').filter(|segment| !segment.is_empty()),
        name,
    )
}

/// One crumb or its `›`: META, never shrinks.
fn crumb(text: impl Into<SharedString>, color: u32) -> gpui::Div {
    div()
        .flex_none()
        .text_size(font_size::META)
        .text_color(rgb(color))
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use super::crumbs;

    #[test]
    fn a_path_splits_into_its_folders_and_its_name() {
        let (folders, name) = crumbs("docs/superpowers/plans/x.md");
        assert_eq!(
            folders.collect::<Vec<_>>(),
            ["docs", "superpowers", "plans"]
        );
        assert_eq!(name, "x.md");
        let (folders, name) = crumbs("README.md");
        assert_eq!(folders.count(), 0, "a root file has no crumbs");
        assert_eq!(name, "README.md");
        let (folders, name) = crumbs("a//b.rs");
        assert_eq!(
            folders.collect::<Vec<_>>(),
            ["a"],
            "empty segments are dropped"
        );
        assert_eq!(name, "b.rs");
    }
}
