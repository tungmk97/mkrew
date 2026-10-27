//! Editor view tabs (SPEC §12A, ADR 0080): switching, closing (unsaved text is asked about
//! first), pinning. The strip and its menu are drawn in `file_tabs_render`.

use crate::*;

impl Shell {
    /// The shown tab of `project_id`'s Files view.
    pub(crate) fn active_file_tab(&mut self, project_id: Ulid) -> Option<&mut FileTab> {
        self.files.get_mut(&project_id)?.tab_mut()
    }

    /// After the active tab changed: a link's line asked before it showed lands, the editor
    /// shows its caret, the strip shows the tab, a stale or failed tab is read and a `.md`
    /// page is built. A find or block-edit field the new tab hides gives the keyboard back.
    pub(crate) fn file_tab_shown(
        &mut self,
        project_id: Ulid,
        read: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(view) = self.files.get_mut(&project_id) {
            let tab = view.tab_mut();
            let (previewing, block_edit) = tab.map_or((false, false), |tab| {
                tab.apply_pending_line();
                (tab.previewing(), tab.block_edit.is_some())
            });
            let find = self.file_find_focus.is_focused(window)
                || self.file_replace_focus.is_focused(window);
            let block = self.file_block_focus.is_focused(window);
            if (find && previewing) || (block && !block_edit) {
                window.focus(&self.file_editor_focus, cx);
            }
            if let Some(shown) = view.shown() {
                self.file_code_scroll
                    .scroll_to_item(shown.editor.cursor().line, ScrollStrategy::Center);
            }
            if let Some(at) = view.active_index() {
                self.file_tabs_scroll.scroll_to_item(at);
            }
        }
        let requests = Requests { list: vec![], read };
        self.files_requests(project_id, requests, window, cx);
        self.build_preview(project_id, window, cx);
        self.follow_file_tabs(project_id, window, cx);
    }

    /// The Editor view comes with the first open file and leaves with the last one (closed,
    /// or another worktree started the files over). Only for the shown workspace.
    pub(crate) fn follow_file_tabs(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace != Some(project_id) {
            return;
        }
        let stored = self.views.get(&project_id).copied().unwrap_or_default();
        match (self.has_file_tabs(project_id), stored == View::Editor) {
            (true, false) => self.show_view(View::Editor, window, cx),
            (false, true) => self.show_view(View::Terminals, window, cx),
            _ => {}
        }
    }

    /// A click on a tab (`Kept` for a double click), or any open of a file: its tab shows.
    pub(crate) fn open_file_tab(
        &mut self,
        project_id: Ulid,
        path: &str,
        how: Open,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = self.files_view(project_id).open(path, how);
        self.file_tab_shown(project_id, read, window, cx);
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab in the Files view.
    pub(crate) fn cycle_file_tab(
        &mut self,
        project_id: Ulid,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = self.files_view(project_id).cycle(forward);
        self.file_tab_shown(project_id, read, window, cx);
    }

    /// Ctrl+Alt+1–9 in the Files view.
    pub(crate) fn nth_file_tab(
        &mut self,
        project_id: Ulid,
        n: u8,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = self.files_view(project_id).select_nth(n);
        self.file_tab_shown(project_id, read, window, cx);
    }

    /// ✕, middle click, Ctrl+W and the tab menu's closes: asks about each unsaved tab first
    /// (Cancel closes nothing).
    pub(crate) fn close_file_tabs(
        &mut self,
        project_id: Ulid,
        path: &str,
        which: Close,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_tab_menu = None;
        let paths = self.files_view(project_id).close_targets(path, which);
        if paths.is_empty() {
            return;
        }
        let closing = paths.clone();
        let then: AfterSave = Box::new(move |shell, window, cx| {
            let read = shell.files_view(project_id).close(&closing);
            shell.file_tab_shown(project_id, read, window, cx);
        });
        self.leave_files(project_id, paths, then, window, cx);
    }

    /// Ctrl+W / Ctrl+Shift+W in the Files view.
    pub(crate) fn close_active_file_tab(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let active = (self.files.get(&project_id))
            .and_then(FilesView::file)
            .map(str::to_owned);
        if let Some(path) = active {
            self.close_file_tabs(project_id, &path, Close::One, window, cx);
        }
    }

    pub(crate) fn pin_file_tab(
        &mut self,
        project_id: Ulid,
        path: &str,
        pinned: bool,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_tab_menu = None;
        self.files_view(project_id).pin(path, pinned);
        cx.notify();
    }
}
