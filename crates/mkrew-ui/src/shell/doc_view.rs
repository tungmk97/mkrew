//! The gpui state of one open file pane (focus handles, scroll, page): not pure, so it is
//! not in `Documents`. One per (workspace, path), made when the file is opened or first drawn.

use crate::*;

/// Which of a pane's focus handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocFocus {
    Editor,
    Find,
    Replace,
    Block,
    Preview,
}

pub(crate) struct DocView {
    /// The editor: typing goes there while it has focus (ADR 0028).
    pub(crate) editor_focus: FocusHandle,
    /// The find bar's two fields (outside `EDITOR_CONTEXT`'s arrow keys).
    pub(crate) find_focus: FocusHandle,
    pub(crate) replace_focus: FocusHandle,
    /// The block edit field of the Markdown preview (ADR 0072).
    pub(crate) block_focus: FocusHandle,
    /// The Split view's page (the editor beside it keeps `editor_focus`).
    pub(crate) preview_focus: FocusHandle,
    pub(crate) code_scroll: UniformListScrollHandle,
    /// Where the editor's scroll bar thumb is held while it is dragged.
    pub(crate) code_drag: scrollbar::Drag,
    /// Where the editor's text starts on screen (set while painting), for clicks.
    pub(crate) text_left: Rc<Cell<Pixels>>,
    /// The rendered Markdown page (a virtual list, kept across frames).
    pub(crate) page: preview::Page,
    /// Where the Split view's two halves were painted, for the divider drag.
    pub(crate) split_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The Split view's scroll sync (source -> preview, SPEC §12A).
    pub(crate) split_sync: files_split::Sync,
    /// Why the last "Open in Zed" of this file failed.
    pub(crate) zed_error: Option<String>,
}

impl DocView {
    pub(crate) fn new(cx: &App) -> DocView {
        DocView {
            editor_focus: cx.focus_handle(),
            find_focus: cx.focus_handle(),
            replace_focus: cx.focus_handle(),
            block_focus: cx.focus_handle(),
            preview_focus: cx.focus_handle(),
            code_scroll: UniformListScrollHandle::new(),
            code_drag: scrollbar::Drag::default(),
            text_left: Rc::default(),
            page: preview::Page::default(),
            split_bounds: Rc::default(),
            split_sync: files_split::Sync::default(),
            zed_error: None,
        }
    }

    pub(crate) fn handle(&self, which: DocFocus) -> &FocusHandle {
        match which {
            DocFocus::Editor => &self.editor_focus,
            DocFocus::Find => &self.find_focus,
            DocFocus::Replace => &self.replace_focus,
            DocFocus::Block => &self.block_focus,
            DocFocus::Preview => &self.preview_focus,
        }
    }
}
