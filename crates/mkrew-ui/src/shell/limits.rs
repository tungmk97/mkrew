//! The limits chip of the title bar (spec 2026-11-23 section 5.2, ADR 0106) and the
//! `limits.get` read that fills it for a window that connects late.

use crate::*;
use mkrew_ui::limits::Level;

impl Shell {
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
                    if shell.limits.load(listed) {
                        cx.notify();
                    }
                }
                Err(error) => tracing::warn!("limits.get failed: {error:#}"),
            });
        })
        .detach();
    }
}

/// Seconds east of UTC at Unix time `at`, from the OS time zone (0 when it cannot say).
pub(crate) fn local_offset(at: i64) -> i32 {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(at, 0)
        .single()
        .map_or(0, |time| time.offset().local_minus_utc())
}

fn level_color(level: Level, theme: &Theme) -> u32 {
    match level {
        Level::Warn => theme.status_needs_input,
        Level::Danger => theme.danger_text,
    }
}

impl Shell {
    /// The chip in the title bar's right group, just before the Run pill: the agent window
    /// closest to its limit, only from 80% on (nothing otherwise). Amber from 80%, red from
    /// 95%, always with the warning icon and the text. The hover lists every window with its
    /// reset time; a click opens Settings at the Integrations pages.
    pub(crate) fn render_limits_chip(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let chip =
            mkrew_ui::limits::chip(self.limits.all(), status::now_ms() / 1000, local_offset)?;
        let color = level_color(chip.level, theme);
        Some(
            div()
                .id("limits-chip")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::S))
                .h(theme::rems_px(theme::CONTROL_HEIGHT))
                .px(px(space::M))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.control_border))
                .whitespace_nowrap()
                .text_size(font_size::BODY)
                .text_color(rgb(color))
                .pressable(Look::Chip, true, theme)
                .tooltip(tooltip(theme, &chip.tooltip, ""))
                .on_press(
                    true,
                    cx.listener(|shell, _, window, cx| shell.open_limits_settings(window, cx)),
                )
                .child(icon_svg(Icon::TriangleAlert, theme::ICON_SIZE_SMALL, color))
                .child(chip.text)
                .into_any_element(),
        )
    }

    /// Settings at the Integrations pages (`settings_dialog::INTEGRATIONS_TAB`).
    fn open_limits_settings(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.open_settings_dialog(window, cx);
        if let Some(dialog) = self.settings_dialog.as_mut() {
            let _ = dialog.set_tab(settings_dialog::INTEGRATIONS_TAB);
        }
    }
}
