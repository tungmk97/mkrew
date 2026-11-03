//! The Markdown Preview / Split / Source switch of the Files editor and the Split view (SPEC
//! §12A): the source editor and its live page side by side with a draggable divider, the page
//! following the source's scroll. Also what a click on a page asks for: open a link, tick a
//! task box, copy a code block.

use crate::*;
use mkrew_ui::files_view::StaleEdit;
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
    /// Shows the active Markdown tab in `mode` (the Preview / Split / Source switch); the next
    /// Markdown file opens in it too (`[ui] md_mode`). Nothing for other files.
    pub(crate) fn set_file_mode(
        &mut self,
        project_id: Ulid,
        mode: MdMode,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tab) = self.active_file_tab(project_id) else {
            return;
        };
        let editable = tab
            .shown()
            .is_some_and(|shown| !shown.code.binary && !shown.code.too_large);
        if !(tab.is_markdown() && editable) || tab.mode == mode {
            return;
        }
        tab.set_mode(mode);
        self.md_split_sync.followed.set(None);
        if self.md_mode != mode {
            self.follow_md_mode(mode);
            let value = format!("\"{}\"", mode.name());
            if let Err(error) =
                mkrew_core::settings::write_ui_value(&self.settings_file, "md_mode", &value)
            {
                self.settings_write_failed("the Markdown mode", error, cx);
            }
        }
        self.build_preview(project_id, window, cx);
        // The find fields are not drawn in Preview, nor the split page elsewhere: the keyboard
        // goes where the text is.
        let fields =
            self.file_find_focus.is_focused(window) || self.file_replace_focus.is_focused(window);
        if mode == MdMode::Preview && fields {
            window.focus(&self.focus_handle, cx);
        } else if mode != MdMode::Split && self.file_preview_focus.is_focused(window) {
            window.focus(&self.file_editor_focus, cx);
        }
        cx.notify();
    }

    /// Ctrl+Shift+V: Preview ↔ Source (Split goes to Preview).
    pub(crate) fn toggle_file_preview(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(mode) = self.active_file_tab(project_id).map(|tab| tab.mode) else {
            return;
        };
        let next = match mode {
            MdMode::Preview => MdMode::Source,
            MdMode::Split | MdMode::Source => MdMode::Preview,
        };
        self.set_file_mode(project_id, next, window, cx);
    }

    /// Ctrl+Alt+V: Split, or back to Source from it.
    pub(crate) fn toggle_file_split(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(mode) = self.active_file_tab(project_id).map(|tab| tab.mode) else {
            return;
        };
        let next = match mode {
            MdMode::Split => MdMode::Source,
            MdMode::Preview | MdMode::Source => MdMode::Split,
        };
        self.set_file_mode(project_id, next, window, cx);
    }

    /// `[ui] md_mode` is `mode` (switched to, or edited in the settings): every Files view
    /// opens its next Markdown tab in it.
    pub(crate) fn follow_md_mode(&mut self, mode: MdMode) {
        self.md_mode = mode;
        for view in self.files.values_mut() {
            view.md_mode = mode;
        }
    }

    /// The Split view: the editor and the page, `md_split` of the width for the editor, the
    /// divider between them. The page follows the editor's scroll.
    pub(crate) fn render_md_split(
        &self,
        project_id: Ulid,
        tab: &FileTab,
        shown: &files_view::Shown,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        self.follow_source_scroll(tab, cx);
        let bounds = Rc::clone(&self.md_split_bounds);
        let source = self.render_shown_file(project_id, shown, window, theme, cx);
        let page = self.render_md_preview(project_id, tab, true, window, theme, cx);
        let divider = theme.divider;
        let grip = div()
            .id("md-split-grip")
            .flex_none()
            .w(px(theme::GRIP_WIDTH))
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(move |style| style.bg(rgb(divider)))
            .child(div().w(px(theme::BORDER_WIDTH)).h_full().bg(rgb(divider)))
            .tooltip(tooltip(
                theme,
                "Drag to resize · double-click for half and half",
                "",
            ))
            .on_drag(MdSplitGrip, |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(
                cx.listener(|shell, event: &DragMoveEvent<MdSplitGrip>, window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                    let bounds = shell.md_split_bounds.get();
                    shell.md_split = files_view::md_split_share_at(
                        f32::from(bounds.origin.x),
                        f32::from(bounds.size.width),
                        f32::from(event.event.position.x),
                    );
                    shell.md_split_dragging = true;
                    cx.notify();
                }),
            )
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
    fn file_code_top_line(&self, cx: &App) -> usize {
        let theme = cx.theme();
        let row = theme.terminal_font_size * theme.code_line_height;
        let scrolled = -f32::from(self.file_code_scroll.0.borrow().base_handle.offset().y);
        if row > 0.0 {
            (scrolled / row).floor().max(0.0) as usize
        } else {
            0
        }
    }

    /// Scrolls the Split view's page to the block the editor shows at its top, once per move
    /// of the editor (the page can be scrolled on its own meanwhile).
    fn follow_source_scroll(&self, tab: &FileTab, cx: &App) {
        let Some(page) = tab.preview() else {
            return;
        };
        let at = Some((tab.opening, self.file_code_top_line(cx)));
        let sync = &self.md_split_sync;
        if sync.followed.get() == at {
            return;
        }
        sync.followed.set(at);
        if sync.hold.replace(false) {
            return;
        }
        let line = at.map_or(0, |(_, line)| line);
        // Shown first: a new opening resets the list, which would drop the scroll.
        self.file_page.show(tab.opening, page);
        let line = u32::try_from(line + 1).unwrap_or(u32::MAX);
        let (block, into) = markdown::block_at_line(&page.blocks, line).unwrap_or((0, 0.0));
        self.file_page.scroll_to_block(block, into);
    }

    /// A press on block `block` of the Split view's page: the editor's caret goes to its first
    /// line, scrolled into view when it is not shown (that move is not followed back).
    pub(crate) fn split_caret_to_block(&mut self, project_id: Ulid, block: usize, cx: &App) {
        let rows = self.file_page_lines(cx);
        let top = self.file_code_top_line(cx);
        let Some(tab) = self.active_file_tab(project_id) else {
            return;
        };
        let line = (tab.preview())
            .and_then(|page| page.blocks.get(block))
            .map(|block| (block.lines.start as usize).saturating_sub(1));
        let (Some(line), Some(editor)) = (line, tab.editor_mut()) else {
            return;
        };
        editor.set_cursor(Pos::new(line, 0), false);
        if !(top..top + rows).contains(&line) {
            self.file_code_scroll
                .scroll_to_item(line, ScrollStrategy::Center);
            self.md_split_sync.hold.set(true);
        }
    }

    /// A click on a link in a preview: a web link opens in the browser, `#heading` scrolls the
    /// page to it, a file of the folder opens in the Editor (at `#L12`'s line). A link outside
    /// a page (a thread message) resolves against the folder root and has no headings.
    pub(crate) fn open_preview_link(
        &mut self,
        action: &OpenLink,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let shown_target = self.files.get(&project_id).and_then(|v| v.worktree_task_id);
        // The file a page's link is in (the Editor's tab or the Docs file), the folder it is
        // in, its page and blocks.
        let (from, target, page) = match (action.on_page, self.view()) {
            (true, View::Editor) => {
                let tab = self.files.get(&project_id).and_then(FilesView::tab);
                let page = tab.and_then(|tab| Some((&self.file_page, tab.preview()?.clone())));
                (tab.map(|tab| tab.path.clone()), shown_target, page)
            }
            (true, View::Docs) => {
                let docs = self.docs.get(&project_id);
                let page = docs.and_then(|docs| match &docs.doc {
                    Doc::Shown(shown) => Some((&self.doc_page, Rc::clone(shown))),
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
            Some(LinkTarget::Web(url)) => cx.open_url(&url),
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
            Some(LinkTarget::File { path, line }) if shown_target == target => {
                self.reveal_file(project_id, target, path, line, Open::Kept, window, cx);
            }
            // A Docs link while the tree shows a task worktree: the project folder's file.
            Some(LinkTarget::File { path, line }) => {
                self.open_file_at(project_id, target, path, line, window, cx);
            }
            None => {
                let dest = &action.dest;
                self.toast_info(format!("Mkrew does not open this link: {dest}"), cx);
            }
        }
    }

    /// A click on a task box of the Files preview: ticks or unticks it in the source (unsaved,
    /// like any edit; Ctrl+Z takes it back).
    pub(crate) fn toggle_preview_task(
        &mut self,
        action: &ToggleTask,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace.filter(|_| self.view() == View::Editor) else {
            return;
        };
        let toggled = self
            .active_file_tab(project_id)
            .map(|tab| tab.toggle_task(action.0));
        match toggled {
            Some(Ok(true)) => {
                self.file_edited(project_id, window, cx);
                cx.notify();
            }
            Some(Err(StaleEdit)) => self.toast_error("The file changed; click again", cx),
            Some(Ok(false)) | None => {}
        }
    }

    /// "Copy" on a code block of a preview.
    pub(crate) fn copy_preview_code(
        &mut self,
        action: &CopyCode,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.0.clone()));
        self.toast_info("Copied", cx);
    }
}
