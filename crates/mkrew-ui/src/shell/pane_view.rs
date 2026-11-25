//! One pane of a tab as its own view, embedded `cached` (ADR 0114): output, frames and keys of
//! other panes do not lay it out again. Like `SidebarView` it renders again whenever the
//! `Shell` notifies, the window refreshes, its bounds change or something inside it scrolls.

use gpui::{Subscription, WeakEntity};

use crate::*;

/// Where a pane is drawn: its window, workspace, tab and leaf.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PaneSlot {
    pub(crate) host: HostId,
    pub(crate) project_id: Ulid,
    pub(crate) tab_id: Ulid,
    pub(crate) key: PaneKey,
}

// ponytail: every Shell notify renders every pane view; notify only the pane of the session
// that changed if a profile shows Shell notifies dominate.
pub(crate) struct PaneView {
    shell: WeakEntity<Shell>,
    slot: PaneSlot,
    _shell_changed: Subscription,
}

impl PaneView {
    pub(crate) fn new(shell: &Entity<Shell>, slot: PaneSlot, cx: &mut Context<'_, Self>) -> Self {
        Self {
            shell: shell.downgrade(),
            slot,
            _shell_changed: cx.observe(shell, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for PaneView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let view = cx.entity_id();
        let slot = self.slot.clone();
        self.shell
            .update(cx, |shell, cx| shell.render_pane(&slot, view, window, cx))
            .unwrap_or_else(|_| gpui::Empty.into_any_element())
    }
}
