//! The in-app toast stack (SPEC §14): `toast_error` / `toast_info` push, the status tick
//! (`connection.rs`) expires, `render_toasts` draws it bottom-right over the content.

use crate::*;

impl Shell {
    /// A failure that has no place of its own to show in (its dialog closed, its view moved).
    pub(crate) fn toast_error(&mut self, text: impl Into<String>, cx: &mut Context<'_, Self>) {
        self.toasts.push(toast::Kind::Error, text, Instant::now());
        cx.notify();
    }

    /// Feedback for an action that otherwise shows nothing (a copy).
    pub(crate) fn toast_info(&mut self, text: impl Into<String>, cx: &mut Context<'_, Self>) {
        self.toasts.push(toast::Kind::Info, text, Instant::now());
        cx.notify();
    }

    /// In a machine window an entry that opens files on this computer (Zed, Explorer, Reveal,
    /// Follow, Open transcript) cannot work: the files are on the other computer. Shows the
    /// toast and returns `true`; the entry then returns without doing anything.
    pub(crate) fn local_only(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let Some(name) = self.machine_name().map(str::to_owned) else {
            return false;
        };
        self.toast_info(mkrew_ui::machines::opens_local_files_only(&name), cx);
        true
    }

    /// "Global: Copy last error": the newest error toast's full text, even after it hid.
    pub(crate) fn copy_last_error(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(text) = self.toasts.last_error().map(str::to_owned) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.toast_info("Copied", cx);
        }
    }

    /// A failed write of `settings.toml`: the palette lists the file's problem (§20.0) and the
    /// toast says the choice was not saved.
    pub(crate) fn settings_write_failed(
        &mut self,
        what: &str,
        error: impl std::fmt::Display,
        cx: &mut Context<'_, Self>,
    ) {
        let error = error.to_string();
        self.toast_error(format!("Cannot save {what}: {error}"), cx);
        self.settings_error = Some(error);
    }

    /// The stack, newest at the bottom, in the bottom-right corner of the content area. The
    /// wrapper has no hitbox: only a card (like a menu panel) absorbs clicks, and nothing here
    /// takes the keyboard.
    pub(crate) fn render_toasts(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        if self.toasts.items().is_empty() {
            return None;
        }
        let cards = self.toasts.items().iter().map(|item| {
            // Icon + text, never color alone.
            let (glyph, color) = match item.kind {
                toast::Kind::Info => (Icon::Info, theme.accent_text),
                toast::Kind::Error => (Icon::CircleAlert, theme.status_exited),
            };
            let id = item.id;
            let hover = theme.hover;
            let is_error = item.kind == toast::Kind::Error;
            let text = item.text.clone();
            let body = div()
                .id(("toast-text", id))
                .flex_1()
                .min_w_0()
                .line_clamp(theme::TOAST_MAX_LINES)
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .child(item.text.clone());
            // An error's full text is often cut by the line clamp: a click copies all of it.
            let body = if is_error {
                body.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .tooltip(tooltip(theme, "Copy the error", ""))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        shell.toast_info("Copied", cx);
                    }))
                    .into_any_element()
            } else {
                body.into_any_element()
            };
            menu_panel(("toast", id), theme)
                .w(theme::rems_px(theme::TOAST_WIDTH))
                .flex_row()
                .items_center()
                .gap(px(space::S))
                .px(px(space::M))
                .py(px(space::S))
                // Hovering pauses the countdown; leaving restarts it (toast.rs).
                .on_hover(cx.listener(move |shell, hovered: &bool, _, cx| {
                    shell.toasts.set_hovered(id, *hovered, Instant::now());
                    cx.notify();
                }))
                .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, color))
                .child(body)
                .child(
                    div()
                        .id(("toast-close", id))
                        .flex_none()
                        .p(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(theme, "Dismiss", ""))
                        .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.toasts.dismiss(id);
                            cx.notify();
                        })),
                )
        });
        Some(
            div()
                .absolute()
                .bottom(px(space::L))
                .right(px(space::L))
                .flex()
                .flex_col()
                .gap(px(space::S))
                .children(cards)
                .into_any_element(),
        )
    }

    /// A one-line notice in the tab bar (nothing went wrong, so no toast card); it goes away
    /// by itself.
    pub(crate) fn show_tab_notice(&mut self, text: &str, cx: &mut Context<'_, Self>) {
        let text = text.to_owned();
        self.tab_notice = Some(text.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(theme::TAB_NOTICE_TIMEOUT)
                .await;
            let _ = this.update(cx, |shell, cx| {
                if shell.tab_notice.as_deref() == Some(text.as_str()) {
                    shell.tab_notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
