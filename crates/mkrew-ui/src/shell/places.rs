//! Opening a file or any leaf in the layouts: one way in for every entry point (SPEC §6.1).

use crate::*;

impl Shell {
    /// The only way a file opens (SPEC §6.1): where `place` says, or, when the file is open
    /// already in any tab or window, only its pane is focused. `file` says which folder it is
    /// of (the project's or a task worktree's). `line` is a link's (line, column), 1-based,
    /// shown in either case. Runs in the workspace's Space window (`in_workspace` hops there).
    pub(crate) fn open_document(
        &mut self,
        project_id: Ulid,
        file: FileKey,
        line: Option<(u32, u32)>,
        place: documents::Place,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !self.acts_here(project_id, window) {
            return self.in_workspace(project_id, window, cx, move |shell, window, cx| {
                shell.open_document(project_id, file, line, place, window, cx);
            });
        }
        let preview = place == documents::Place::Preview;
        self.ensure_doc(project_id, &file, None, cx);
        if let Some(doc) = self.doc_mut(project_id, &file) {
            doc.set_line(line);
        }
        let mut leaf = Leaf::of_file(file.clone());
        if let Leaf::File { preview: p, .. } = &mut leaf {
            *p = preview;
        }
        // Terminals show the panes: Board, Docs and the Diff give way.
        self.views.insert(project_id, View::Terminals);
        if self.workspace_in(window) != Some(project_id) {
            self.activate_workspace(self.space_host(window), project_id, window, cx);
        }
        self.place_leaf(project_id, leaf, place, window, cx);
        // Shown already: the line applies now; otherwise after the read.
        self.show_pending_line(project_id, &file);
        self.focus_file_pane(project_id, &file, window, cx);
    }

    /// Puts `leaf` in the layouts as `place` resolves (`documents::resolve_place`): it is
    /// focused where it is open already, replaces the preview tab, opens a new tab right after
    /// the active one, or splits a pane at an edge. Saves the layout.
    pub(crate) fn place_leaf(
        &mut self,
        project_id: Ulid,
        leaf: Leaf,
        place: documents::Place,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let key = leaf.key();
        let Some(tabs) = self.tabs.get(&project_id) else {
            self.toast_error("The tabs of this workspace are still loading", cx);
            return;
        };
        let resolution =
            documents::resolve_place(tabs, self.documents.get(&project_id), &key, &place);
        match resolution {
            Resolution::Focus(key) => self.show_pane(project_id, &key, window, cx),
            Resolution::Reuse { old } => {
                let Some(tabs) = self.tabs.get_mut(&project_id) else {
                    return;
                };
                let Some(tab_id) = tabs.tab_with(&old).map(|tab| tab.id) else {
                    return;
                };
                tabs.activate(tab_id);
                // The new leaf takes the old one's place: insert beside it, then close it.
                let writes = [
                    tabs.insert_leaf(tab_id, Some(&old), Edge::Right, leaf),
                    tabs.close_leaf(&old),
                ];
                for write in writes.into_iter().flatten() {
                    self.save_layout(project_id, write, window, cx);
                }
                self.sync_panes(window, cx);
            }
            Resolution::NewTab { position } => {
                self.create_tab_then(
                    project_id,
                    position,
                    move |shell, tab_id, window, cx| {
                        let write = (shell.tabs.get_mut(&project_id))
                            .and_then(|tabs| tabs.insert_leaf(tab_id, None, Edge::Right, leaf));
                        if let Some(write) = write {
                            shell.save_layout(project_id, write, window, cx);
                        }
                        shell.sync_panes(window, cx);
                    },
                    window,
                    cx,
                );
            }
            Resolution::Split { target, edge } => {
                let Some(tabs) = self.tabs.get_mut(&project_id) else {
                    return;
                };
                let Some(tab_id) = tabs.tab_with(&target).map(|tab| tab.id) else {
                    return;
                };
                if let Some(write) = tabs.insert_leaf(tab_id, Some(&target), edge, leaf) {
                    self.save_layout(project_id, write, window, cx);
                }
                self.sync_panes(window, cx);
            }
        }
        cx.notify();
    }

    /// Brings the pane of `key` forward: its tab is selected (in a detached window, that
    /// window is raised, as `open_session` does), the pane focused.
    fn show_pane(
        &mut self,
        project_id: Ulid,
        key: &PaneKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tab_id) = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.tab_with(key))
            .map(|tab| tab.id)
        else {
            return;
        };
        self.focus_pane_key(project_id, key);
        match self.terminal_host(project_id, tab_id) {
            Some(host @ HostId::Detached(window_key)) => {
                let dock = DockTab::Terminal {
                    project: project_id,
                    tab_id,
                };
                if let Some(at) =
                    (self.detached.tabs_of(window_key).iter()).position(|t| *t == dock)
                {
                    self.detached.select(window_key, at);
                }
                self.windows_dirty = true;
                self.in_window(host, cx, move |shell, window, cx| {
                    window.activate_window();
                    shell.sync_panes(window, cx);
                    cx.notify();
                });
            }
            Some(HostId::Space(_)) | None => {
                if let Some(tabs) = self.tabs.get_mut(&project_id) {
                    tabs.activate(tab_id);
                }
                self.sync_panes(window, cx);
            }
        }
    }

    /// The pane of `file` has the keyboard in its editor (a new file, a link, a note).
    pub(crate) fn focus_file_pane(
        &mut self,
        project_id: Ulid,
        file: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.focus_pane_key(project_id, &PaneKey::File(file.clone()));
        self.focus_doc(project_id, file, DocFocus::Editor, window, cx);
    }

    /// A kept leaf: an edit, a double click or a pin ends the preview tab.
    pub(crate) fn keep_leaf(
        &mut self,
        project_id: Ulid,
        file: &FileKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let key = PaneKey::File(file.clone());
        let write = (self.tabs.get_mut(&project_id))
            .and_then(|tabs| tabs.update_leaf(&key, documents::keep_leaf));
        if let Some(write) = write {
            self.save_layout(project_id, write, window, cx);
            cx.notify();
        }
    }
}
