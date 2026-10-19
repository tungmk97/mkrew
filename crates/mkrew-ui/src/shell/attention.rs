//! The OS shell's view of the Inbox (SPEC §20J, ADR 0064): taskbar badge, flash, tray icon and
//! menu, hiding the window to the tray, quitting the UI.

use crate::*;

impl Shell {
    /// Recounts "Needs you" and updates only what changed: the badge when its number moved, the
    /// tray (icon, tooltip, menu) when anything it shows moved. O(sessions), no allocation when
    /// nothing changed; called on session events, after a list load and when a view changed
    /// what needs the user, never per frame.
    pub(crate) fn sync_attention(&mut self, cx: &mut Context<'_, Self>) {
        let (projects, sessions) = (self.sidebar.projects(), self.sidebar.sessions());
        let counts = attention::counts(projects, sessions, &self.viewed);
        let change = self.attention.update(counts);
        if !change.tray {
            return;
        }
        let theme = cx.theme();
        if change.badge {
            // Dark ink on amber in both themes: the dark theme's background token.
            self.desktop
                .set_badge(counts.needs_you, theme.status_needs_input, Theme::dark().bg);
        }
        let color = match counts.icon() {
            attention::Icon::NeedsYou => theme.status_needs_input,
            _ => theme.status_working,
        };
        let menu = attention::menu(projects, sessions, &self.viewed, counts);
        self.desktop.set_tray(counts.icon(), color, menu);
        if counts.needs_you == 0 {
            self.desktop.flash(false);
        }
    }

    /// Shows the window again (tray, toast click, second launch) and raises it. gpui's
    /// `activate_window` only restores a *minimized* window, not a hidden one. Explorer makes
    /// the taskbar button again a moment later, without its badge, so that is set once more
    /// after a tick.
    pub(crate) fn restore_window(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.desktop.show_window();
        window.activate_window();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(theme::STATUS_TICK).await;
            let _ = this.update(cx, |shell, _| shell.desktop.reapply_badge());
        })
        .detach();
    }

    /// A click in the tray.
    pub(crate) fn tray_event(
        &mut self,
        event: TrayEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match event {
            TrayEvent::Open => self.restore_window(window, cx),
            TrayEvent::Session(session) => {
                self.restore_window(window, cx);
                self.open_session(session, window, cx);
            }
            TrayEvent::Quit => self.quit_ui(window, cx),
            TrayEvent::TaskbarRestarted => self.desktop.reapply_badge(),
        }
        cx.notify();
    }

    /// "Quit UI": unsaved Files changes are asked first (with the window shown, so the question
    /// is seen), then the UI ends; the daemon and the agents keep running (SPEC §2.2).
    pub(crate) fn quit_ui(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let again: AfterSave = Box::new(|shell, window, cx| shell.quit_ui(window, cx));
        if self.ask_unsaved(again, window, cx) {
            self.restore_window(window, cx);
            return;
        }
        self.desktop.remove_tray();
        cx.quit();
    }
}
