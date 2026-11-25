//! The Markdown Preview / Split / Source switch of the Files editor and the Split view (SPEC
//! §12A): the source editor and its live page side by side with a draggable divider, the page
//! following the source's scroll. Also what a click on a page asks for: open a link, tick a
//! task box, copy a code block.

use crate::*;
use mkrew_ui::documents::StaleEdit;
use mkrew_ui::markdown::{self, LinkTarget};
use mkrew_ui::preview::{CopyCode, OpenLink, ToggleTask};

/// Payload of a drag of the Split view's divider (a resize, nothing is dropped).
pub(crate) struct MdSplitGrip;

/// The page following the source's scroll: the opening and source line it last followed, and
/// whether the source's next move came from a press on the page (not followed back, so the
/// page does not jump under the pointer).
#[derive(Debug, Default)]
pub(crate) struct Sync {
    followed: Cell<Option<(u64, usize)>>,
    hold: Cell<bool>,
}

impl Shell {
    /// Shows the Markdown file `path` in `mode` (the Preview / Split / Source switch); the next
    /// Markdown file opens in it too (`[ui] md_mode`). Nothing for other files.
    pub(crate) fn set_file_mode(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        mode: MdMode,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(doc) = self.doc_mut(project_id, path) else {
            return;
        };
        let editable = doc
            .shown()
            .is_some_and(|shown| !shown.code.binary && !shown.code.too_large);
        if !(doc.is_markdown() && editable) || doc.mode == mode {
            return;
        }
        doc.set_mode(mode);
        if let Some(view) = self.doc_views.get_mut(&(project_id, path.clone())) {
            view.split_sync.followed.set(None);
        }
        // The leaf remembers the mode across restarts.
        let key = PaneKey::File(path.clone());
        let write = (self.tabs.get_mut(&project_id))
            .and_then(|tabs| tabs.update_leaf(&key, |leaf| documents::set_leaf_mode(leaf, mode)));
        if let Some(write) = write {
            self.save_layout(project_id, write, window, cx);
        }
        if self.md_mode != mode {
            self.follow_md_mode(mode);
            let value = format!("\"{}\"", mode.name());
            if let Err(error) =
                mkrew_core::settings::write_ui_value(&self.settings_file, "md_mode", &value)
            {
                self.settings_write_failed("the Markdown mode", error, cx);
            }
        }
        self.build_preview(project_id, path, window, cx);
        // The find fields are not drawn in Preview, nor the split page elsewhere: the keyboard
        // goes where the text is.
        let fields = self.doc_focused(project_id, path, DocFocus::Find, window)
            || self.doc_focused(project_id, path, DocFocus::Replace, window);
        if mode == MdMode::Preview && fields {
            window.focus(&self.focus_handle, cx);
        } else if mode != MdMode::Split
            && self.doc_focused(project_id, path, DocFocus::Preview, window)
        {
            self.focus_doc(project_id, path, DocFocus::Editor, window, cx);
        }
        cx.notify();
    }

    /// Ctrl+Shift+V: Preview ↔ Source (Split goes to Preview).
    pub(crate) fn toggle_file_preview(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(mode) = self.doc(project_id, path).map(|doc| doc.mode) else {
            return;
        };
        let next = match mode {
            MdMode::Preview => MdMode::Source,
            MdMode::Split | MdMode::Source => MdMode::Preview,
        };
        self.set_file_mode(project_id, path, next, window, cx);
    }

    /// Ctrl+Alt+V: Split, or back to Source from it.
    pub(crate) fn toggle_file_split(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(mode) = self.doc(project_id, path).map(|doc| doc.mode) else {
            return;
        };
        let next = match mode {
            MdMode::Split => MdMode::Source,
            MdMode::Preview | MdMode::Source => MdMode::Split,
        };
        self.set_file_mode(project_id, path, next, window, cx);
    }

    /// `[ui] md_mode` is `mode` (switched to, or edited in the settings): the next Markdown
    /// file opens in it.
    pub(crate) fn follow_md_mode(&mut self, mode: MdMode) {
        self.md_mode = mode;
    }

    /// The Split view: the editor and the page, `md_split` of the width for the editor, the
    /// divider between them. The page follows the editor's scroll.
    pub(crate) fn render_md_split(
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
        self.follow_source_scroll(doc, view, cx);
        let bounds = Rc::clone(&view.split_bounds);
        let path = doc.file.clone();
        let source = self.render_shown_file(project_id, doc, shown, window, theme, cx);
        let page = self.render_md_preview(project_id, doc, true, window, theme, cx);
        let divider = theme.divider;
        let grip = grip_hover(div().id("md-split-grip"), theme)
            .flex_none()
            .w(px(theme::GRIP_WIDTH))
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .child(div().w(px(theme::BORDER_WIDTH)).h_full().bg(rgb(divider)))
            .tooltip(tooltip(
                theme,
                "Drag to resize · double-click for half and half",
                "",
            ))
            .on_drag(MdSplitGrip, |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(cx.listener(
                move |shell, event: &DragMoveEvent<MdSplitGrip>, window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                    let Some(bounds) =
                        (shell.doc_view(project_id, &path)).map(|view| view.split_bounds.get())
                    else {
                        return;
                    };
                    shell.md_split = files_view::md_split_share_at(
                        f32::from(bounds.origin.x),
                        f32::from(bounds.size.width),
                        f32::from(event.event.position.x),
                    );
                    shell.md_split_dragging = true;
                    cx.notify();
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, event: &MouseDownEvent, _, cx| {
                    if event.click_count == 2 {
                        shell.md_split =
                            files_view::md_split_share(mkrew_core::settings::MD_SPLIT_DEFAULT);
                        shell.md_split_dragging = true;
                        shell.end_md_split_drag(cx);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|shell, _, _, cx| shell.end_md_split_drag(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, _, _, cx| shell.end_md_split_drag(cx)),
            );
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .child(
                canvas(move |painted, _, _| bounds.set(painted), |_, (), _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .flex_none()
                    .w(relative(self.md_split))
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(source),
            )
            .child(grip)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(page),
            )
            .into_any_element()
    }

    /// The divider was released: saves the share as `[ui] md_split` (percent).
    fn end_md_split_drag(&mut self, cx: &mut Context<'_, Self>) {
        if !std::mem::take(&mut self.md_split_dragging) {
            return;
        }
        // Clamped to 0.2–0.8 while dragged: a whole percent fits a u8.
        let percent = ((self.md_split * 100.0).round() as u8).to_string();
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "md_split", &percent)
        {
            self.settings_write_failed("the split width", error, cx);
        }
    }

    /// The first source line the editor shows (0-based), from its scroll offset.
    fn file_code_top_line(view: &DocView, cx: &App) -> usize {
        let theme = cx.theme();
        let row = theme.terminal_font_size * theme.code_line_height;
        let scrolled = -f32::from(view.code_scroll.0.borrow().base_handle.offset().y);
        if row > 0.0 {
            (scrolled / row).floor().max(0.0) as usize
        } else {
            0
        }
    }

    /// Scrolls the Split view's page to the block the editor shows at its top, once per move
    /// of the editor (the page can be scrolled on its own meanwhile).
    fn follow_source_scroll(&self, doc: &documents::Doc, view: &DocView, cx: &App) {
        let Some(page) = doc.preview() else {
            return;
        };
        let at = Some((doc.opening, Self::file_code_top_line(view, cx)));
        let sync = &view.split_sync;
        if sync.followed.get() == at {
            return;
        }
        sync.followed.set(at);
        if sync.hold.replace(false) {
            return;
        }
        let line = at.map_or(0, |(_, line)| line);
        // Shown first: a new opening resets the list, which would drop the scroll.
        view.page.show(doc.opening, page);
        let line = u32::try_from(line + 1).unwrap_or(u32::MAX);
        let (block, into) = markdown::block_at_line(&page.blocks, line).unwrap_or((0, 0.0));
        view.page.scroll_to_block(block, into);
    }

    /// A press on block `block` of the Split view's page: the editor's caret goes to its first
    /// line, scrolled into view when it is not shown (that move is not followed back).
    pub(crate) fn split_caret_to_block(
        &mut self,
        project_id: Ulid,
        path: &FileKey,
        block: usize,
        cx: &App,
    ) {
        let rows = self.file_page_lines(project_id, path, cx);
        let Some(top) =
            (self.doc_view(project_id, path)).map(|view| Self::file_code_top_line(view, cx))
        else {
            return;
        };
        let Some(doc) = self.doc_mut(project_id, path) else {
            return;
        };
        let line = (doc.preview())
            .and_then(|page| page.blocks.get(block))
            .map(|block| (block.lines.start as usize).saturating_sub(1));
        let (Some(line), Some(editor)) = (line, doc.editor_mut()) else {
            return;
        };
        editor.set_cursor(Pos::new(line, 0), false);
        if !(top..top + rows).contains(&line)
            && let Some(view) = self.doc_view(project_id, path)
        {
            view.code_scroll
                .scroll_to_item(line, ScrollStrategy::Center);
            view.split_sync.hold.set(true);
        }
    }

    /// A click on a link in a preview: a web link opens in the browser, `#heading` scrolls the
    /// page to it, a file of the folder opens as a file tab (at `#L12`'s line). A link outside
    /// a page (a thread message) resolves against the folder root and has no headings.
    pub(crate) fn open_preview_link(
        &mut self,
        action: &OpenLink,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace_in(window) else {
            return;
        };
        let shown_target = self.files.get(&project_id).and_then(|v| v.worktree_task_id);
        // The file a page's link is in (the focused file pane or the Docs file), the folder it is
        // in, its page and blocks.
        let file = self.focused_doc(window).map(|(_, file)| file);
        let (from, target, page) = match (action.on_page, self.view(self.space_host(window))) {
            (true, View::Terminals) if file.is_some() => {
                let doc = file.as_ref().and_then(|file| self.doc(project_id, file));
                let view = file
                    .as_ref()
                    .and_then(|file| self.doc_view(project_id, file));
                let page = doc
                    .zip(view)
                    .and_then(|(doc, view)| Some((&view.page, doc.preview()?.clone())));
                let target = file.as_ref().and_then(|file| file.task);
                (file.map(|file| file.path), target, page)
            }
            (true, View::Docs) => {
                let docs = self.docs.get(&project_id);
                let page = docs.and_then(|docs| match &docs.doc {
                    Doc::Shown(shown) => Some((
                        &self.host_view(self.space_host(window)).doc_page,
                        Rc::clone(shown),
                    )),
                    _ => None,
                });
                (docs.and_then(|docs| docs.selected.clone()), None, page)
            }
            _ => (Some(String::new()), shown_target, None),
        };
        let Some(from) = from else {
            return;
        };
        match markdown::link_target(&from, &action.dest) {
            Some(LinkTarget::Web(url)) => {
                self.open_web(url, action.session.map(PaneKey::Session), window, cx)
            }
            Some(LinkTarget::Anchor(anchor)) => {
                let heading = (page.as_ref()).and_then(|(page, shown)| {
                    Some((page, markdown::heading_for(&shown.blocks, &anchor)?))
                });
                match heading {
                    Some((page, block)) => {
                        page.scroll_to_block(block, 0.0);
                        cx.notify();
                    }
                    None => self.toast_info(format!("No heading #{anchor} here"), cx),
                }
            }
            // A file opens as a tab of the main window.
            Some(LinkTarget::File { .. }) if self.host_of(window).space().is_none() => {
                let action = action.clone();
                self.in_window(self.primary_host(), cx, move |shell, window, cx| {
                    window.activate_window();
                    shell.open_preview_link(&action, window, cx);
                });
            }
            // The linked file is of the folder the link is in.
            Some(LinkTarget::File { path, line }) => {
                self.open_file_at(project_id, target, path, line, window, cx);
            }
            None => {
                let dest = &action.dest;
                self.toast_info(format!("Mkrew does not open this link: {dest}"), cx);
            }
        }
    }

    /// A click on a task box of a file pane's preview: ticks or unticks it in the source (unsaved,
    /// like any edit; Ctrl+Z takes it back).
    pub(crate) fn toggle_preview_task(
        &mut self,
        action: &ToggleTask,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((project_id, path)) = self.focused_doc(window) else {
            return;
        };
        let toggled = (self.doc_mut(project_id, &path)).map(|doc| doc.toggle_task(action.0));
        match toggled {
            Some(Ok(true)) => {
                self.file_edited(project_id, &path, window, cx);
                cx.notify();
            }
            Some(Err(StaleEdit)) => self.toast_error("The file changed; click again", cx),
            Some(Ok(false)) | None => {}
        }
    }

    /// "Copy" on a code block of a preview: the text goes to the clipboard and the button
    /// says "Copied" for [`theme::COPIED_FEEDBACK`].
    pub(crate) fn copy_preview_code(
        &mut self,
        action: &CopyCode,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.0.clone()));
        let key = preview::code_key(&action.0);
        preview::set_copied(Some(key));
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(theme::COPIED_FEEDBACK).await;
            let _ = this.update(cx, |_, cx| {
                if preview::copied() == Some(key) {
                    preview::set_copied(None);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A press on a path pill of a Chat message: the file opens as a file tab of the
    /// session's workspace and worktree, at its `:line`. A path that leaves the workspace
    /// opens nothing. A file that does not exist gets the Files view's own message.
    pub(crate) fn open_chat_path(
        &mut self,
        action: &preview::OpenPath,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(pill) = markdown::path_pill(&action.text) else {
            return;
        };
        let Some((project_id, target, base)) = self.link_scope(action.session) else {
            return;
        };
        let Some(path) = mkrew_ui::chat::files_path(&base.to_string_lossy(), pill.path) else {
            self.toast_error("That file is outside this workspace", cx);
            return;
        };
        let line = pill.line;
        if self.host_of(window).space().is_none() {
            return self.in_window(self.primary_host(), cx, move |shell, window, cx| {
                window.activate_window();
                shell.open_file_at(project_id, target, path, line, window, cx);
            });
        }
        self.open_file_at(project_id, target, path, line, window, cx);
    }
}
