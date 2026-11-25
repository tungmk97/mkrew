//! The root view of a Space window (ADR 0115): `Shell` draws it (`render_space`) and handles
//! its input, as for a detached window (`DetachedWindow`).

use crate::*;

pub(crate) struct SpaceWindow {
    shell: gpui::WeakEntity<Shell>,
    key: SpaceKey,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SpaceWindow {
    pub(crate) fn new(
        shell: Entity<Shell>,
        key: SpaceKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let weak = shell.downgrade();
        let host = HostId::Space(key);
        let mut subscriptions = vec![
            // Every `Shell` change may change what this window shows.
            cx.observe(&shell, |_, _, cx| cx.notify()),
            // System mode follows the OS live (ADR 0027).
            cx.observe_window_appearance(window, |this, window, cx| {
                // A preview was built for the old appearance; the kept theme is rebuilt now.
                let _ = this
                    .shell
                    .update(cx, |shell, _| shell.drop_gallery_preview());
                theme::refresh(cx.theme().mode, window, cx);
                cx.notify();
            }),
            // Minimized → detach every pane; restored → re-attach from daemon snapshots (§10).
            cx.observe_window_visibility(window, move |this, visibility, window, cx| {
                let _ = this.shell.update(cx, |shell, cx| {
                    shell.hosts.entry(host).or_default().visible =
                        visibility == gpui::WindowVisibility::Visible;
                    shell.sync_panes(window, cx);
                    cx.notify();
                });
            }),
            // Coming back to the window views the focused pane (a done session leaves the
            // Inbox), even one marked unread while it kept the keyboard, and ends the flash.
            cx.observe_window_activation(window, move |this, window, cx| {
                if window.is_window_active() {
                    cx.set_reduce_motion(mkrew_ui::desktop::reduce_motion());
                    let _ = this.shell.update(cx, |shell, cx| {
                        shell.activated(host);
                        shell.viewed.activated();
                        shell.desktop.flash(false);
                        cx.notify();
                    });
                }
            }),
            cx.observe_window_bounds(window, move |this, window, cx| {
                let _ = this
                    .shell
                    .update(cx, |shell, _| shell.record_bounds(host, window));
            }),
        ];
        // The focus subscriptions are per window: each Space window has its own.
        subscriptions.extend(shell.update(cx, |shell, cx| {
            [
                cx.on_blur(&shell.rename_focus, window, |shell, window, cx| {
                    shell.finish_rename(true, window, cx);
                }),
                cx.on_blur(&shell.follow_up_focus, window, |shell, _, cx| {
                    // A typed draft (or one being sent) stays on its card without its caret;
                    // an empty one goes.
                    shell
                        .follow_up
                        .take_if(|draft| draft.text.text().is_empty() && !draft.busy);
                    cx.notify();
                }),
                // The focused control of a dialog left the frame (a button swapped for the next
                // step): the dialog takes the keyboard back so Tab and Esc keep working. A
                // confirm prompt keeps it; menus are never open over a modal.
                cx.on_focus_lost(window, |shell, window, cx| {
                    if shell.modal_open() && !window.has_active_prompt() {
                        window.focus(&shell.dialog_focus, cx);
                    }
                }),
            ]
        }));
        let close = weak.clone();
        window.on_window_should_close(cx, move |window, cx| {
            close
                .update(cx, |shell, cx| shell.space_should_close(key, window, cx))
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

impl Render for SpaceWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let key = self.key;
        let content = self
            .shell
            .update(cx, |shell, cx| shell.render_space(key, window, cx))
            .unwrap_or_else(|_| div().into_any_element());
        let shell = self.shell.clone();
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(move |_, action: &Run, window, cx| {
                let _ = shell.update(cx, |shell, cx| shell.run(action, window, cx));
            }))
            .on_action(cx.listener({
                let shell = self.shell.clone();
                move |_, action: &preview::OpenPath, window, cx| {
                    let _ = shell.update(cx, |shell, cx| shell.open_chat_path(action, window, cx));
                }
            }))
            .on_action(cx.listener({
                let shell = self.shell.clone();
                move |_, action: &preview::CopyCode, window, cx| {
                    let _ =
                        shell.update(cx, |shell, cx| shell.copy_preview_code(action, window, cx));
                }
            }))
            .on_action(cx.listener({
                let shell = self.shell.clone();
                move |_, action: &preview::OpenLink, window, cx| {
                    let _ =
                        shell.update(cx, |shell, cx| shell.open_preview_link(action, window, cx));
                }
            }))
            .on_action(cx.listener({
                let shell = self.shell.clone();
                move |_, action: &preview::ExpandDiagram, window, cx| {
                    let _ = shell.update(cx, |shell, cx| shell.expand_diagram(action, window, cx));
                }
            }))
            .child(content)
    }
}
