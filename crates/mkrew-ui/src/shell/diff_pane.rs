//! The Diff leaf (spec 2026-11-23 §4.4, §5): opening it, the list state of its pane, the pane.

use crate::*;

/// The diff list of one workspace's Diff leaf: the `git_diff_*` fields `Shell` held while the
/// diff was a view of its own. One per workspace, so a diff pane in a detached window
/// and one in the main window (two workspaces) never share it.
pub(crate) struct DiffViewState {
    /// The diff rows (a variable-height list: wrapped lines are taller).
    pub(crate) list: ListState,
    /// What `list` was last laid out for (the diff, mode, row count; another resets it).
    pub(crate) layout: Cell<Option<(usize, DiffMode, usize)>>,
    /// What changes row heights in place (wrap, picked lines: remeasured).
    pub(crate) looks: Cell<Option<(bool, Option<Selection>)>>,
    /// A row to scroll to once the list is laid out for the shown diff.
    pub(crate) scroll_to: Cell<Option<usize>>,
    /// A diff row to scroll into view once the list is laid out (a keyboard pick moved).
    pub(crate) reveal: Cell<Option<usize>>,
}

impl DiffViewState {
    pub(crate) fn new() -> DiffViewState {
        DiffViewState {
            list: ListState::new(0, ListAlignment::Top, px(theme::GIT_DIFF_OVERDRAW)),
            layout: Cell::new(None),
            looks: Cell::new(None),
            scroll_to: Cell::new(None),
            reveal: Cell::new(None),
        }
    }
}

impl Shell {
    /// A Diff leaf of the active workspace is drawn: in the main window's tab in front, or in
    /// the tab a visible detached window shows.
    pub(crate) fn diff_on_screen(&self) -> bool {
        let Some(project_id) = self.workspace else {
            return false;
        };
        let Some(tabs) = self.tabs.get(&project_id) else {
            return false;
        };
        let holds = |tab: &Tab| tab.shown().contains(&PaneKey::Diff);
        let in_front = self.view() == View::Terminals && tabs.active().is_some_and(holds);
        in_front
            || (self.detached.visible_tabs().into_iter()).any(
                |DockTab::Terminal { project, tab_id }| {
                    project == project_id && tabs.tab(tab_id).is_some_and(holds)
                },
            )
    }

    /// Gives every workspace that holds a Diff leaf its list state (a leaf restored from the
    /// saved layout never went through `open_diff`) and drops the state of one that no longer
    /// does. Called at the start of every window's render, which has `&mut self`.
    pub(crate) fn ensure_diff_views(&mut self) {
        self.diff_views.retain(|id, _| {
            self.tabs
                .get(id)
                .is_some_and(|tabs| tabs.tab_with(&PaneKey::Diff).is_some())
        });
        for (id, tabs) in &self.tabs {
            if tabs.tab_with(&PaneKey::Diff).is_some() {
                self.diff_views
                    .entry(*id)
                    .or_insert_with(DiffViewState::new);
            }
        }
    }

    /// Opens the diff of the Git panel's selection as a leaf (`place`: a new tab after the
    /// active one, or a split of a pane). One Diff leaf per workspace: when it is open
    /// already, its tab comes forward and its pane gets the focus (`place_leaf`). `project_id`
    /// is the active workspace (every opener activates it first). Git loads when the leaf
    /// comes on screen (`sync_panes`).
    pub(crate) fn open_diff(
        &mut self,
        project_id: Ulid,
        place: documents::Place,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // A detached window has no Git panel: the main window opens it.
        if self.host_of(window) != HostId::Main {
            self.in_window(HostId::Main, cx, move |shell, window, cx| {
                window.activate_window();
                shell.open_diff(project_id, place, window, cx);
            });
            return;
        }
        if self.view() != View::Terminals {
            self.show_view(View::Terminals, window, cx);
        }
        self.place_leaf(project_id, Leaf::diff(), place, window, cx);
    }

    /// The Diff leaf's content: the selected file's diff card (toolbar, diff, comment margin).
    /// The pane carries the `"Git"` key context. While the panel's Git tab is not drawn it
    /// also holds the Git keyboard (`git_focus`); one element tracks `git_focus` per frame.
    pub(crate) fn render_diff_pane(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        // A Diff leaf of another workspace (torn off to a window while the main window shows
        // a different one): Git and its keys belong to the active workspace only, so this
        // pane never acts on the wrong one.
        if self.workspace != Some(project_id) {
            return message(
                theme,
                None,
                "This diff belongs to another workspace.".into(),
                Some("Switch to that workspace to review its changes.".into()),
            )
            .size_full()
            .child(
                button("diff-switch-workspace", "Switch to workspace", theme, true).on_click(
                    cx.listener(move |shell, _, window, cx| {
                        shell.in_main_window(window, cx, move |shell, window, cx| {
                            shell.activate_workspace(project_id, window, cx);
                        });
                    }),
                ),
            )
            .into_any_element();
        }
        let Some(view) = self.git.get(&project_id) else {
            return message(theme, None, "Loading changes…".into(), None)
                .size_full()
                .into_any_element();
        };
        let card = self
            .render_git_diff(project_id, view, theme, window, cx)
            .key_context(GIT_CONTEXT);
        let card = if self.git_panel_shown() {
            card
        } else {
            card.track_focus(&self.git_focus)
                .on_key_down(cx.listener(Self::git_key_down))
        };
        div()
            .size_full()
            .min_w_0()
            .flex()
            .font_family(theme.ui_font_family.clone())
            .child(card)
            .into_any_element()
    }
}
