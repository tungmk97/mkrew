//! The sidebar as a child view of the [`Shell`], embedded `cached`: GPUI re-renders a view
//! whose child notified (a terminal pane printing), so without this every output frame laid
//! out the whole sidebar again. It renders again only when the Shell notifies (its state
//! changed) or the window refreshes (focus, theme, resize).

use gpui::WeakEntity;

use crate::*;

pub(crate) struct SidebarView {
    shell: WeakEntity<Shell>,
}

impl SidebarView {
    pub(crate) fn new(shell: WeakEntity<Shell>, cx: &mut Context<'_, Self>) -> Self {
        if let Some(entity) = shell.upgrade() {
            cx.observe(&entity, |_, _, cx| cx.notify()).detach();
        }
        Self { shell }
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let now = status::now_ms();
        // It records its own menu anchors (the Shell frame start keeps them: this view is cached).
        cx.global::<MenuAnchors>().clear_sidebar();
        self.shell
            .update(cx, |shell, cx| {
                if shell.sidebar_layout.compact() {
                    shell
                        .render_compact_sidebar(now, &theme, cx)
                        .into_any_element()
                } else {
                    shell
                        .render_sidebar(now, &theme, window, cx)
                        .into_any_element()
                }
            })
            .unwrap_or_else(|_| gpui::Empty.into_any_element())
    }
}
