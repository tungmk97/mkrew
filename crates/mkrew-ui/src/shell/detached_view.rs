//! The root view of a detached window (ADR 0096): `Shell` draws it and handles its input.

use crate::*;

pub(crate) struct DetachedWindow {
    shell: gpui::WeakEntity<Shell>,
    key: WindowKey,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl DetachedWindow {
    pub(crate) fn new(
        shell: Entity<Shell>,
        key: WindowKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let weak = shell.downgrade();
        let subscriptions = vec![
            // Every `Shell` change may change what this window shows.
            cx.observe(&shell, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, window, cx| {
                theme::refresh(cx.theme().mode, window, cx);
                cx.notify();
            }),
            cx.observe_window_activation(window, move |this, window, cx| {
                if window.is_window_active() {
                    let _ = this.shell.update(cx, |shell, _| {
                        shell.activated(HostId::Detached(key));
                    });
                }
            }),
            cx.observe_window_bounds(window, move |this, window, cx| {
                let _ = this.shell.update(cx, |shell, _| {
                    shell.record_bounds(HostId::Detached(key), window);
                });
            }),
            cx.observe_window_visibility(window, move |this, visibility, window, cx| {
                let shown = visibility == gpui::WindowVisibility::Visible;
                let _ = this.shell.update(cx, |shell, cx| {
                    shell.detached.set_visible(key, shown);
                    shell.sync_panes(window, cx);
                });
            }),
        ];
        let close = weak.clone();
        window.on_window_should_close(cx, move |window, cx| {
            close
                .update(cx, |shell, cx| shell.detached_should_close(key, window, cx))
                .unwrap_or(true)
        });
        Self {
            shell: weak,
            key,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }
}

impl Render for DetachedWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let key = self.key;
        let content = self
            .shell
            .update(cx, |shell, cx| shell.render_host(key, window, cx))
            .unwrap_or_else(|_| div().into_any_element());
        let shell = self.shell.clone();
        // The window's key context is a focused pane's while no pane has focus; a pane's own
        // `key_context` (set by its element) is inside it.
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context(keymap::TERMINAL_CONTEXT)
            .on_action(cx.listener(move |_, action: &Run, window, cx| {
                let _ = shell.update(cx, |shell, cx| shell.run(action, window, cx));
            }))
            .child(content)
    }
}
