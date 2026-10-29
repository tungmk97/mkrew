//! Docs view: list, filter, read, preview and render.

use crate::*;

impl Shell {
    /// `doc.list` for the Docs view of `project_id` (also keeps the daemon watching its docs);
    /// the first listing shows its first file.
    pub(crate) fn load_docs(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let docs = self
            .docs
            .entry(project_id)
            .or_insert_with(|| Docs::new(project_id));
        if !docs.begin_list() {
            return;
        }
        let listed =
            connection.call::<Vec<DocFile>>(Request::DocList(DocListParams { project_id }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &listed {
                    tracing::warn!(%project_id, "doc.list failed: {error}");
                }
                let Some(docs) = shell.docs.get_mut(&project_id) else {
                    return;
                };
                let (read, again) = docs.listed(listed);
                if let Some(file_path) = read {
                    shell.read_doc(project_id, file_path, window, cx);
                }
                if again {
                    shell.load_docs(project_id, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Shows `file_path` in the Docs view of `project_id`, reading it if needed.
    pub(crate) fn select_doc(
        &mut self,
        project_id: Ulid,
        file_path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = self
            .docs
            .get_mut(&project_id)
            .and_then(|docs| docs.select(file_path));
        if let Some(file_path) = read {
            self.read_doc(project_id, file_path, window, cx);
        }
        cx.notify();
    }

    /// The Docs filter field: typing filters, ↑ / ↓ show the previous / next shown file,
    /// Esc clears the filter (then leaves the field), Enter leaves it.
    pub(crate) fn docs_filter_key_down(
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
        let Some(docs) = self.docs.get_mut(&project_id) else {
            return;
        };
        match key {
            quick_open::Key::Close if !docs.filter.is_empty() => docs.filter.clear(),
            quick_open::Key::Close | quick_open::Key::Open => {
                window.focus(&self.focus_handle, cx);
            }
            quick_open::Key::Move(by) => {
                if let Some(path) = docs.step_selection(by as isize) {
                    self.select_doc(project_id, &path, window, cx);
                }
            }
            quick_open::Key::Edit(key) => {
                copy_selection(&key, &docs.filter, cx);
                docs.filter.edit(&key);
            }
            quick_open::Key::Paste => {
                docs.filter
                    .insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                docs.filter.insert(&text);
            }
        }
        cx.notify();
    }

    /// `doc.read` (and `comment.list` for a newly shown file); the answer is dropped if
    /// another file got selected meanwhile.
    pub(crate) fn read_doc(
        &mut self,
        project_id: Ulid,
        file_path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let list = self
            .docs
            .get_mut(&project_id)
            .and_then(|docs| docs.comments.begin_list(&file_path));
        if let Some(file_path) = list {
            self.load_comments(project_id, CommentContext::Doc, file_path, window, cx);
        }
        let read = connection.call::<DocContent>(Request::DocRead(DocReadParams {
            project_id,
            file_path: file_path.clone(),
        }));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let read = match read.await {
                Ok(content) => Ok(background.spawn(async move { Shown::new(&content) }).await),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &read {
                    tracing::warn!(%project_id, file_path, "doc.read failed: {error}");
                }
                if let Some(docs) = shell.docs.get_mut(&project_id) {
                    docs.read(&file_path, read);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Follows `doc.changed` / `comment.updated`: re-reads a shown file that changed and
    /// re-lists the Docs view on screen (hidden ones re-list when shown again).
    pub(crate) fn doc_event(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let shown = self.workspace.filter(|_| self.view() == View::Docs);
        let reactions: Vec<_> = self
            .docs
            .values_mut()
            .map(|docs| (docs.project_id, docs.apply(event)))
            .collect();
        for (project_id, reaction) in reactions {
            if let Some(file_path) = reaction.reread {
                self.read_doc(project_id, file_path, window, cx);
                cx.notify();
            }
            if reaction.relist && shown == Some(project_id) {
                self.load_docs(project_id, window, cx);
            }
        }
    }

    /// Opens the shown doc of `project_id` in Zed; a failure shows in the Docs header.
    pub(crate) fn open_doc_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        if self.local_only(cx) {
            return;
        }
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        let Some(docs) = self.docs.get_mut(&project_id) else {
            return;
        };
        let Some(file_path) = docs.selected.as_deref() else {
            return;
        };
        // Native separators (`file_path` uses `/`), as Zed gets the path verbatim.
        let path = file_path
            .split('/')
            .fold(PathBuf::from(&project.path), |path, part| path.join(part));
        docs.zed_error = match zed::open(&path, None) {
            Ok(()) => None,
            Err(error) => {
                tracing::warn!(%project_id, file_path, "open in Zed failed: {error}");
                Some(error.to_string())
            }
        };
        cx.notify();
    }

    /// The workspace's Markdown docs (SPEC §12): the file list with open-comment counts, and
    /// the selected file rendered or as source with line numbers.
    pub(crate) fn render_docs(
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
        let Some(docs) = self.docs.get(&project_id) else {
            return empty("Loading docs…", None).into_any_element();
        };
        match &docs.files {
            Files::Loading => return empty("Loading docs…", None).into_any_element(),
            Files::Failed(error) => {
                return empty("Cannot list the docs.", Some(error.clone()))
                    .child(
                        button("docs-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| shell.load_docs(project_id, window, cx),
                        )),
                    )
                    .into_any_element();
            }
            Files::Loaded(files) if files.is_empty() => {
                let keys = self.keymap.label(Command::ShowTerminals);
                return empty(
                    "No Markdown files to review yet.",
                    Some("Plans and docs written by agents show up here.".into()),
                )
                .child(
                    button("docs-terminals", "Show terminals", theme, true)
                        .tooltip(tooltip(theme, "Show terminals", keys.clone()))
                        .child(
                            div()
                                .text_size(font_size::META)
                                .opacity(opacity::SECONDARY)
                                .child(keys),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.show_view(View::Terminals, window, cx);
                        })),
                )
                .into_any_element();
            }
            Files::Loaded(_) => {}
        }
        let selected = docs.selected.as_deref();
        let shown_files = docs.shown_files();
        let filter_focused = self.docs_filter_focus.is_focused(window);
        let list = div()
            .flex_none()
            .w(px(theme::DOC_LIST_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .child(
                div().px(px(space::S)).pb(px(space::XS)).child(
                    caret_field(
                        "docs-filter",
                        &docs.filter,
                        "Filter files…",
                        filter_focused,
                        theme,
                        cx,
                        |shell, caret| {
                            if let Some(docs) =
                                shell.workspace.and_then(|id| shell.docs.get_mut(&id))
                            {
                                docs.filter.click(caret.at, caret.extend, caret.clicks);
                            }
                        },
                    )
                    .track_focus(&self.docs_filter_focus)
                    .on_key_down(cx.listener(Self::docs_filter_key_down))
                    .relative()
                    .child(self.typing(&self.docs_filter_focus, Self::docs_filter_key_down, cx))
                    .tooltip(tooltip(
                        theme,
                        "Filter the files",
                        self.keymap.label(Command::Find),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        window.focus(&shell.docs_filter_focus, cx);
                        cx.notify();
                    })),
                ),
            )
            .child(
                div()
                    .id("doc-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .pb(px(space::S))
                    .when(shown_files.is_empty(), |el| {
                        el.child(
                            div()
                                .px(px(space::S))
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child("No files match the filter."),
                        )
                    })
                    .children(shown_files.into_iter().map(|(file, matched)| {
                        let shown = selected == Some(file.file_path.as_str());
                        doc_row(project_id, file, &matched, shown, theme, cx)
                    })),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .font_family(theme.ui_font_family.clone())
            .child(list)
            .child(self.render_doc(project_id, docs, theme, window, cx))
            .into_any_element()
    }

    /// The selected doc: its path, Preview / Source, Comment and Open in Zed, then its content
    /// (blocks / lines are picked by clicking) with the comment threads in the right margin.
    pub(crate) fn render_doc(
        &self,
        project_id: Ulid,
        docs: &Docs,
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
            .mr(px(space::S))
            .mb(px(space::S))
            .rounded(px(radius::MEDIUM))
            .overflow_hidden()
            .bg(rgb(theme.surface));
        let Some(file_path) = docs.selected.clone() else {
            return panel
                .child(message(theme, None, "Select a file to read it.".into(), None).flex_1());
        };
        let zed_keys = self.keymap.label(Command::OpenInZed);
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
                    .child(file_path.clone()),
            )
            .child(self.mode_toggle(
                "doc-mode",
                docs.mode,
                [(Mode::Preview, "Preview"), (Mode::Source, "Source")],
                "Toggle preview / source",
                self.keymap.label(Command::ToggleDocSource).into(),
                move |shell, _, _, cx| {
                    if let Some(docs) = shell.docs.get_mut(&project_id) {
                        docs.toggle_mode();
                        cx.notify();
                    }
                },
                theme,
                cx,
            ))
            .child(self.add_comment_button(
                project_id,
                CommentContext::Doc,
                matches!(docs.doc, Doc::Shown(_)) && docs.comments.selection.is_some(),
                theme,
                cx,
            ))
            .child(self.send_comments_button(
                project_id,
                CommentContext::Doc,
                matches!(docs.doc, Doc::Shown(_)).then_some(&docs.comments),
                theme,
                cx,
            ))
            .child(
                button(
                    "doc-zed",
                    icon_text(Icon::ExternalLink, "Open in Zed", button_fg(theme, false)),
                    theme,
                    false,
                )
                .flex_none()
                .tooltip(tooltip(theme, "Open this file in Zed", zed_keys))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    shell.open_doc_in_zed(project_id, cx);
                })),
            );
        let body = match &docs.doc {
            Doc::None | Doc::Loading => message(theme, None, "Loading…".into(), None)
                .flex_1()
                .into_any_element(),
            Doc::Failed(error) => message(
                theme,
                None,
                "Cannot read this file.".into(),
                Some(error.clone().into()),
            )
            .flex_1()
            .child(
                button("doc-retry", "Retry", theme, true).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        shell.select_doc(project_id, &file_path, window, cx);
                    },
                )),
            )
            .into_any_element(),
            Doc::Deleted => message(
                theme,
                None,
                "This file was deleted.".into(),
                Some("It shows again here if it is written back.".into()),
            )
            .flex_1()
            .into_any_element(),
            Doc::Shown(shown) => match docs.mode {
                mode if shown.lines.is_empty()
                    || (mode == Mode::Preview && shown.blocks.is_empty()) =>
                {
                    message(theme, None, "This file is empty.".into(), None)
                        .flex_1()
                        .into_any_element()
                }
                Mode::Source => {
                    let this = cx.entity().downgrade();
                    let on_line: preview::OnLine = Rc::new(move |line, extend, _, cx| {
                        let _ = this.update(cx, |shell, cx| {
                            shell.pick_lines(
                                project_id,
                                CommentContext::Doc,
                                line,
                                line,
                                extend,
                                cx,
                            );
                        });
                    });
                    preview::source(
                        "doc-source",
                        Rc::clone(shown),
                        docs.comments.selection,
                        on_line,
                        &self.doc_source_scroll,
                        theme,
                    )
                    .flex_1()
                    .min_h_0()
                    .px(px(space::L))
                    .pb(px(space::L))
                    .into_any_element()
                }
                Mode::Preview => {
                    // A click picks the block's lines to comment on (Shift extends); picked
                    // blocks are highlighted.
                    let this = cx.entity().downgrade();
                    let selection = docs.comments.selection;
                    let (picked_bg, hover_bg) = (theme.terminal_selection_bg, theme.surface_raised);
                    let wrap: preview::Wrap = Rc::new(
                        move |index: usize,
                              block: &mkrew_ui::markdown::Block,
                              element: gpui::AnyElement| {
                            let (start, end) = (block.lines.start, block.lines.end);
                            let picked = selection.is_some_and(|s| s.contains(start, end));
                            let this = this.clone();
                            div()
                                .id(("doc-block", index))
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .cursor_pointer()
                                .when(picked, |el| el.bg(rgb(picked_bg)))
                                .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover_bg))))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    move |event: &MouseDownEvent, _, cx| {
                                        let extend = event.modifiers.shift;
                                        // Fails only once the window (and Shell) is gone.
                                        let _ = this.update(cx, |shell, cx| {
                                            shell.pick_lines(
                                                project_id,
                                                CommentContext::Doc,
                                                start,
                                                end,
                                                extend,
                                                cx,
                                            );
                                        });
                                    },
                                )
                                .child(element)
                                .into_any_element()
                        },
                    );
                    let fonts = cx.global::<theme::Fonts>();
                    preview::page(
                        &self.doc_page,
                        docs.opening,
                        shown,
                        Some(wrap),
                        None,
                        fonts,
                        theme,
                    )
                    .into_any_element()
                }
            },
        };
        let margin = margin_shown(&docs.doc)
            .then(|| self.render_comments(project_id, &docs.comments, theme, window, cx));
        panel
            .child(header)
            .children(docs.zed_error.clone().map(|error| {
                notice(theme.text, format!("✕ {error}"))
                    .px(px(space::L))
                    .pb(px(space::S))
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(body),
                    )
                    .children(margin),
            )
    }

    /// A segmented control over two modes (Docs and Files Preview / Source, Git Unified /
    /// Split); the inactive segment runs `switch` with its mode. `id` prefixes the segments'
    /// element ids (`(id, index)`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mode_toggle<M: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        mode: M,
        options: [(M, &'static str); 2],
        tip: &'static str,
        keys: SharedString,
        switch: impl Fn(&mut Self, M, &mut Window, &mut Context<'_, Self>) + Clone + 'static,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let hover = theme.surface_raised;
        div()
            .flex_none()
            .flex()
            .gap(px(space::XS))
            .children(
                options
                    .into_iter()
                    .enumerate()
                    .map(|(index, (target, label))| {
                        let active = target == mode;
                        let switch = switch.clone();
                        div()
                            .id((id, index))
                            .px(px(space::S))
                            .py(px(space::XS))
                            .rounded(px(radius::SMALL))
                            .text_size(font_size::BODY)
                            .tooltip(tooltip(theme, tip, keys.clone()))
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
                                        switch(shell, target, window, cx);
                                    }))
                            })
                            .child(label)
                    }),
            )
    }
}

pub(crate) fn doc_row(
    project_id: Ulid,
    file: &DocFile,
    matched: &[u32],
    selected: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let (folder, name) = match file.file_path.rsplit_once('/') {
        Some((folder, name)) => (Some(folder), name),
        None => (None, file.file_path.as_str()),
    };
    let hover = theme.surface_raised;
    let file_path = file.file_path.clone();
    // `matched` indexes chars of the whole path: split it over the folder and the name.
    let name_from = folder.map_or(0, |folder| folder.chars().count() + 1);
    let name_from = u32::try_from(name_from).unwrap_or(u32::MAX);
    let name_hits: Vec<u32> = matched
        .iter()
        .filter_map(|i| i.checked_sub(name_from))
        .collect();
    let folder_hits: Vec<u32> = matched.iter().copied().filter(|i| *i < name_from).collect();
    let marked = |text: &str, hits: &[u32]| {
        let style = HighlightStyle {
            color: Some(Hsla::from(rgb(theme.accent))),
            font_weight: Some(FontWeight(weight::SEMIBOLD)),
            ..HighlightStyle::default()
        };
        StyledText::new(text.to_owned()).with_highlights(
            quick_open::match_ranges(text, hits)
                .into_iter()
                .map(move |range| (range, style)),
        )
    };
    let line = |el: gpui::Div| el.overflow_hidden().text_ellipsis().whitespace_nowrap();
    div()
        .id(SharedString::from(format!("doc-{}", file.file_path)))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .cursor_pointer()
        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
        .when(!selected, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(line(
                    div()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .when(selected, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(marked(name, &name_hits)),
                ))
                .children(folder.map(|folder| {
                    line(
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(marked(folder, &folder_hits)),
                    )
                })),
        )
        .when(file.open_comments > 0, |el| {
            el.child(
                div()
                    .flex_none()
                    .px(px(space::XS))
                    .rounded(px(radius::SMALL))
                    // Stands out on the selected row's raised background too.
                    .bg(rgb(if selected {
                        theme.bg
                    } else {
                        theme.surface_raised
                    }))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{} open", file.open_comments)),
            )
        })
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_doc(project_id, &file_path, window, cx);
        }))
}
