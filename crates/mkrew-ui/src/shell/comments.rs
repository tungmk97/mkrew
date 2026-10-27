//! Review comments: drafts, line picks, threads, submit and status.

use crate::*;

impl Shell {
    /// The comments of `project_id`'s Docs view (doc comments) or Git view (diff comments).
    pub(crate) fn comments_mut(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
    ) -> Option<&mut Comments> {
        match context {
            CommentContext::Doc => self
                .docs
                .get_mut(&project_id)
                .map(|docs| &mut docs.comments),
            CommentContext::Diff => self.git.get_mut(&project_id).map(|view| &mut view.comments),
        }
    }

    /// Whose comments the shown view has: the Docs view's or the Git view's.
    pub(crate) fn comment_context(&self) -> Option<CommentContext> {
        match self.view() {
            View::Docs => Some(CommentContext::Doc),
            View::Diff => Some(CommentContext::Diff),
            _ => None,
        }
    }

    /// `comment.list` of the shown doc or diff; `comment.updated` keeps it live afterwards.
    pub(crate) fn load_comments(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        file_path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let listed = connection.call::<Vec<DocComment>>(Request::CommentList(CommentListParams {
            project_id,
            file_path: Some(file_path.clone()),
            context: Some(context),
        }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &listed {
                    tracing::warn!(%project_id, file_path, "comment.list failed: {error}");
                }
                if let Some(comments) = shell.comments_mut(project_id, context) {
                    comments.listed(&file_path, listed);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The comment or reply being typed in the active workspace's Docs or Git view, while its
    /// field is on screen (the Docs margin shows with the doc, also while it is re-read).
    pub(crate) fn comment_draft(&self) -> Option<&comments::Draft> {
        let project_id = self.workspace?;
        match self.comment_context()? {
            CommentContext::Doc => {
                let docs = self.docs.get(&project_id)?;
                if !margin_shown(&docs.doc) {
                    return None;
                }
                docs.comments.draft.as_ref()
            }
            CommentContext::Diff => {
                let view = self.git.get(&project_id)?;
                if !view.comments_shown() {
                    return None;
                }
                view.comments.draft.as_ref()
            }
        }
    }

    /// A click on lines `start..=end` of the shown doc or diff (Shift extends the selection).
    pub(crate) fn pick_lines(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        start: u32,
        end: u32,
        extend: bool,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(comments) = self.comments_mut(project_id, context) {
            comments.pick(start, end, extend);
            cx.notify();
        }
    }

    /// A click on a diff line (new-side line `line`): picks it, and the Git list takes the
    /// keyboard so Ctrl+Alt+M comments (an open draft keeps it, so typing never stages).
    pub(crate) fn pick_diff_line(
        &mut self,
        project_id: Ulid,
        line: u32,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.git_view(project_id).comments.draft.is_none() {
            window.focus(&self.git_focus, cx);
        }
        self.pick_lines(project_id, CommentContext::Diff, line, line, extend, cx);
    }

    /// "Add comment" (Ctrl+Alt+M): a draft in the margin for the selected lines.
    pub(crate) fn start_comment(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let started = match context {
            CommentContext::Doc => match self.docs.get_mut(&project_id) {
                Some(Docs {
                    doc: Doc::Shown(shown),
                    comments,
                    ..
                }) => comments.start_comment(&shown.lines),
                _ => false,
            },
            CommentContext::Diff => self
                .git
                .get_mut(&project_id)
                .is_some_and(GitView::start_comment),
        };
        if started {
            window.focus(&self.comment_focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn start_reply(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        root: CommentId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(comments) = self.comments_mut(project_id, context) {
            comments.start_reply(root);
            window.focus(&self.comment_focus, cx);
            cx.notify();
        }
    }

    pub(crate) fn close_comment_draft(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let shown = self.workspace.zip(self.comment_context());
        if let Some(comments) = shown.and_then(|(id, context)| self.comments_mut(id, context)) {
            comments.draft = None;
        }
        if self.view() == View::Diff {
            window.focus(&self.git_focus, cx);
        } else {
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }

    /// Keys typed in a comment draft: Enter saves, Esc drops it.
    pub(crate) fn comment_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let (Some(project_id), Some(context)) = (self.workspace, self.comment_context()) else {
            return;
        };
        let Some(draft) = self
            .comments_mut(project_id, context)
            .and_then(|comments| comments.draft.as_mut())
        else {
            return;
        };
        copy_selection(&key, &draft.text, cx);
        if let Some(text) = paste {
            draft.insert(&text);
        }
        match draft.key(key) {
            comments::Outcome::Edited => cx.notify(),
            comments::Outcome::Cancel => self.close_comment_draft(window, cx),
            comments::Outcome::Submit => self.submit_comment(project_id, context, window, cx),
        }
    }

    /// A press or drag in the comment draft: the caret goes there.
    fn click_comment_draft(&mut self, caret: Caret) {
        let (Some(project_id), Some(context)) = (self.workspace, self.comment_context()) else {
            return;
        };
        if let Some(draft) = self
            .comments_mut(project_id, context)
            .and_then(|comments| comments.draft.as_mut())
        {
            draft.text.click(caret.at, caret.extend, caret.clicks);
        }
    }

    /// `comment.create` / `comment.reply` of the draft; saved → the draft closes.
    pub(crate) fn submit_comment(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(comments) = self.comments_mut(project_id, context) else {
            return;
        };
        let Some(serial) = comments.draft.as_ref().map(|draft| draft.serial) else {
            return;
        };
        let Some(request) = comments.submit(project_id) else {
            return;
        };
        cx.notify();
        let saved = connection.call::<DocComment>(request);
        cx.spawn_in(window, async move |this, cx| {
            let saved = saved.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &saved {
                    tracing::warn!(%project_id, "saving a comment failed: {error}");
                }
                let ok = saved.is_ok();
                if let Some(comments) = shell.comments_mut(project_id, context) {
                    let saved = saved.map_err(|error| format!("Cannot save: {error}"));
                    comments.submitted(serial, saved);
                }
                if ok && shell.comment_focus.is_focused(window) && shell.comment_draft().is_none() {
                    if shell.view() == View::Diff {
                        window.focus(&shell.git_focus, cx);
                    } else {
                        window.focus(&shell.focus_handle, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Resolve (or Reopen) the thread of `root`; the answer updates it like `comment.updated`.
    pub(crate) fn set_comment_status(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        root: CommentId,
        resolve: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let params = CommentRef { comment_id: root };
        let request = if resolve {
            Request::CommentResolve(params)
        } else {
            Request::CommentReopen(params)
        };
        let changed = connection.call::<DocComment>(request);
        cx.spawn_in(window, async move |this, cx| {
            let changed = changed.await;
            let _ = this.update(cx, |shell, cx| {
                let Some(comments) = shell.comments_mut(project_id, context) else {
                    return;
                };
                match changed {
                    Ok(comment) => {
                        comments.error = None;
                        comments.apply_event(project_id, &comment);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, %root, "comment status change failed: {error:#}");
                        let what = if resolve { "resolve" } else { "reopen" };
                        comments.error = Some(format!("Cannot {what}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Comment" (Ctrl+Alt+M): enabled once lines are selected.
    pub(crate) fn add_comment_button(
        &self,
        project_id: Ulid,
        context: CommentContext,
        ready: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let keys = self.keymap.label(Command::AddComment);
        if !ready {
            return disabled_button(
                "doc-comment",
                icon_text(Icon::MessageSquarePlus, "Comment", theme.text_muted),
                theme,
                false,
            )
            .flex_none()
            .tooltip(tooltip(theme, "Select lines first, then comment", keys));
        }
        button(
            "doc-comment",
            icon_text(Icon::MessageSquarePlus, "Comment", button_fg(theme, false)),
            theme,
            false,
        )
        .flex_none()
        .tooltip(tooltip(theme, "Comment on the selected lines", keys))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.start_comment(project_id, context, window, cx);
        }))
    }

    /// "Send to agent" (Ctrl+Shift+S): enabled while the file has open comments; names how
    /// many go (the selected ones if the selection covers some).
    pub(crate) fn send_comments_button(
        &self,
        project_id: Ulid,
        context: CommentContext,
        comments: Option<&Comments>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let keys = self.keymap.label(Command::SendComments);
        let count = match comments.map(|comments| (&comments.list, comments.selection)) {
            Some((comments::List::Loaded(list), selection)) => {
                doc_dialog::sendable(list, selection).len()
            }
            _ => 0,
        };
        if count == 0 {
            return disabled_button(
                "doc-send",
                icon_text(Icon::Send, "Send to agent", theme.text_muted),
                theme,
                false,
            )
            .flex_none()
            .tooltip(tooltip(theme, "No open comments to send", keys));
        }
        let what = if count == 1 {
            "Send 1 open comment to an agent".to_owned()
        } else {
            format!("Send {count} open comments to an agent")
        };
        button(
            "doc-send",
            icon_text(Icon::Send, "Send to agent", button_fg(theme, false)),
            theme,
            false,
        )
        .flex_none()
        .tooltip(tooltip(theme, &what, keys))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_send_dialog(project_id, context, window, cx);
        }))
    }

    /// The right margin (SPEC §12, §12B): the new-comment draft, then every thread of the file
    /// in anchor order with its replies, Reply and Resolve / Reopen.
    pub(crate) fn render_comments(
        &self,
        project_id: Ulid,
        comments: &Comments,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let context = comments.context;
        let focused = self.comment_focus.is_focused(window);
        let margin = div()
            .id("doc-comments")
            .flex_none()
            .w(px(theme::DOC_MARGIN_WIDTH))
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .pr(px(space::L))
            .pb(px(space::L))
            .text_size(font_size::BODY);
        let new_draft = comments
            .draft
            .as_ref()
            .and_then(|draft| match draft.target {
                comments::Target::New(selection) => Some((draft, selection)),
                comments::Target::Reply(_) => None,
            });
        let margin = margin.children(new_draft.map(|(draft, selection)| {
            div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .p(px(space::M))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(comments::lines_label(selection.start, selection.end)),
                )
                .child(self.comment_field(
                    draft,
                    "Add a comment… (Enter saves)",
                    focused,
                    theme,
                    cx,
                ))
        }));
        let list =
            match &comments.list {
                comments::List::Loading => {
                    return margin.child(meta_line(theme, "Loading comments…".into()));
                }
                comments::List::Failed(error) => {
                    let file_path = comments.file_path.clone().unwrap_or_default();
                    return margin
                        .child(notice(
                            theme.text,
                            format!("✕ Cannot load comments: {error}"),
                        ))
                        .child(button("comments-retry", "Retry", theme, false).on_click(
                            cx.listener(move |shell, _, window, cx| {
                                let list = shell
                                    .comments_mut(project_id, context)
                                    .and_then(|comments| comments.begin_list(&file_path));
                                if let Some(file_path) = list {
                                    shell.load_comments(project_id, context, file_path, window, cx);
                                }
                            }),
                        ));
                }
                comments::List::Loaded(list) => list,
            };
        let threads = comments::threads(list);
        let margin = margin.children(
            comments
                .error
                .clone()
                .map(|error| notice(theme.text, format!("✕ {error}"))),
        );
        if threads.is_empty() && new_draft.is_none() {
            let keys = self.keymap.label(Command::AddComment);
            let hint = match comments.selection {
                Some(picked) => format!(
                    "{} selected. Comment on them ({keys}).",
                    comments::lines_label(picked.start, picked.end)
                ),
                None => format!("Select lines, then comment ({keys})."),
            };
            return margin.child(meta_line(theme, hint.into()));
        }
        margin.children(
            threads.into_iter().map(|thread| {
                self.comment_thread(project_id, &thread, comments, focused, theme, cx)
            }),
        )
    }

    /// One thread: anchor (click selects its lines), status, messages, then Reply and
    /// Resolve / Reopen (revealed on hover, like other secondary actions).
    pub(crate) fn comment_thread(
        &self,
        project_id: Ulid,
        thread: &Thread<'_>,
        comments: &Comments,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let context = comments.context;
        let root = thread.root;
        let root_id = root.id;
        let group = SharedString::from(format!("thread-{root_id}"));
        let (icon, word) = comments::status_label(root.status);
        let resolved = root.status == mkrew_protocol::CommentStatus::Resolved;
        let lines = root.start_line.zip(root.end_line);
        let anchor = lines.map_or_else(String::new, |(start, end)| {
            comments::lines_label(start, end)
        });
        let picked =
            lines.is_some_and(|(start, end)| comments.selection == Some(Selection { start, end }));
        let reply_draft = comments
            .draft
            .as_ref()
            .filter(|draft| draft.target == comments::Target::Reply(root_id));
        // The icon and word carry the status; only "outdated" warns (like conflict badges).
        let status_color = match root.status {
            mkrew_protocol::CommentStatus::Outdated => theme.status_needs_input,
            _ => theme.text_muted,
        };
        let entry = |comment: &DocComment| {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(if comment.author == DocComment::USER {
                            "You".to_owned()
                        } else {
                            comment.author.clone()
                        }),
                )
                .child(
                    div()
                        .text_color(rgb(theme.text))
                        .child(comment.body.clone()),
                )
        };
        let actions = div()
            .flex()
            .gap(px(space::XS))
            // Also shown on the picked thread (its anchor was clicked) and while replying.
            .opacity(if picked || reply_draft.is_some() {
                opacity::FULL
            } else {
                opacity::HIDDEN
            })
            .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
            .when(!resolved && reply_draft.is_none(), |el| {
                el.child(
                    button("comment-reply", "Reply", theme, false).on_click(cx.listener(
                        move |shell, _, window, cx| {
                            shell.start_reply(project_id, context, root_id, window, cx)
                        },
                    )),
                )
            })
            // Diff comments have no whole-file text to compare with the snapshot.
            .when(
                root.sent_snapshot.is_some() && context == CommentContext::Doc,
                |el| {
                    el.child(
                        button("comment-changes", "View changes", theme, false)
                            .tooltip(tooltip(theme, "Compare the file as sent with now", ""))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.open_changes(project_id, root_id, window, cx);
                            })),
                    )
                },
            )
            .child(
                button(
                    "comment-status",
                    if comments::can_reopen(root.status) {
                        "Reopen"
                    } else {
                        "Resolve"
                    },
                    theme,
                    false,
                )
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.set_comment_status(project_id, context, root_id, !resolved, window, cx);
                })),
            );
        div()
            .id(group.clone())
            .group(group)
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(if picked {
                theme.pane_focus_border
            } else {
                theme.surface_raised
            }))
            .when(resolved, |el| el.opacity(opacity::SECONDARY))
            .child(
                div()
                    .id("comment-anchor")
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .text_size(font_size::META)
                    .cursor_pointer()
                    .when_some(lines, |el, (start, end)| {
                        el.on_click(cx.listener(move |shell, _, _, cx| {
                            shell.pick_lines(project_id, context, start, end, false, cx);
                        }))
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text_muted))
                            .child(anchor),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(status_color))
                            .child(format!("{icon} {word}")),
                    ),
            )
            .children(root.quote.clone().filter(|q| !q.is_empty()).map(|quote| {
                div()
                    .pl(px(space::S))
                    .border_l(px(theme::DOC_QUOTE_BAR_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(quote.lines().next().unwrap_or_default().to_owned())
            }))
            .child(entry(root))
            .children(thread.replies.iter().map(|reply| entry(reply)))
            .children(
                reply_draft.map(|draft| {
                    self.comment_field(draft, "Reply… (Enter sends)", focused, theme, cx)
                }),
            )
            .child(actions)
    }

    /// The draft's input (takes the keyboard when clicked) and its saving / failure line.
    pub(crate) fn comment_field(
        &self,
        draft: &comments::Draft,
        placeholder: &'static str,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let status = if draft.busy {
            Some(("…", theme.text_muted, "Saving".to_owned()))
        } else {
            draft
                .error
                .clone()
                .map(|error| ("✕", theme.status_exited, error))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .child(
                caret_field(
                    "comment-draft",
                    &draft.text,
                    placeholder,
                    focused,
                    theme,
                    cx,
                    Shell::click_comment_draft,
                )
                .bg(rgb(theme.surface))
                .track_focus(&self.comment_focus)
                .on_key_down(cx.listener(Shell::comment_key_down))
                .relative()
                .child(self.typing(&self.comment_focus, Shell::comment_key_down, cx))
                .on_click(cx.listener(|shell, _, window, cx| {
                    window.focus(&shell.comment_focus, cx);
                    cx.notify();
                })),
            )
            .children(status.map(|(icon, color, text)| {
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(div().text_color(rgb(color)).child(icon))
                    .child(div().text_color(rgb(theme.text)).child(text))
            }))
    }
}
