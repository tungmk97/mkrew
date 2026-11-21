//! The widget bar (ADR 0102): its switch, its hide toggles and the `limits.get` read. The
//! drawing and the right-click menu follow in the same file.

use crate::*;

impl Shell {
    /// `[ui] widget_bar` on or off, saved. A settings file that does not parse keeps the
    /// choice for this run; the palette says why (§20.0) and a toast says it was not saved.
    pub(crate) fn set_widget_bar(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        if self.widget_bar == on {
            return;
        }
        self.widget_bar = on;
        if let Err(error) = mkrew_core::settings::write_ui_value(
            &self.settings_file,
            "widget_bar",
            &on.to_string(),
        ) {
            self.settings_write_failed("the widget bar", error, cx);
        }
        cx.notify();
    }

    /// Ctrl+Alt+Shift+J, the palette's "View: Show/hide widget bar".
    pub(crate) fn toggle_widget_bar(&mut self, cx: &mut Context<'_, Self>) {
        self.set_widget_bar(!self.widget_bar, cx);
    }

    /// The limits the daemon holds, for a window that connects after they were reported;
    /// events keep them live afterwards.
    pub(crate) fn load_limits(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let listed = connection.call::<Vec<mkrew_protocol::AgentLimits>>(Request::LimitsGet);
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update(cx, |shell, cx| match listed {
                Ok(listed) => {
                    if shell.widget_data.load_limits(listed) && shell.widget_bar {
                        cx.notify();
                    }
                }
                Err(error) => tracing::warn!("limits.get failed: {error:#}"),
            });
        })
        .detach();
    }
}
