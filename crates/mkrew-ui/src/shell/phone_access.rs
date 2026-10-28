//! The "Phone access" dialog and the title bar's phone icon (ADR 0053): start / stop `mk web`
//! (the daemon supervises it while `[web] enabled`), publish it with `tailscale serve`, and
//! hand the link to the phone as a QR code.

use crate::*;

impl Shell {
    /// "Phone access…" (palette, title bar icon): opens the dialog and reads the state at once.
    pub(crate) fn open_phone_access(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        // The status comes from the daemon: no connection, nothing to show.
        if self.connection.is_none() {
            return;
        }
        self.close_menus();
        self.phone_open = true;
        self.phone.opened();
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        self.poll_phone(window, cx);
    }

    pub(crate) fn close_phone_access(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.phone_open = false;
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn phone_access_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_phone_access(window, cx);
        }
    }

    /// Reads `daemon.status` (always) and Tailscale + `web_token` (dialog open). Both are off
    /// the UI thread: the call is a queued request, the `tailscale` runs and the file read go
    /// to the background executor. One poll at a time.
    pub(crate) fn poll_phone(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if !self.phone.begin_poll(Instant::now()) {
            return;
        }
        let open = self.phone_open;
        let config_dir = self.config_dir.clone();
        let settings_file = self.settings_file.clone();
        let status = connection.call::<mkrew_protocol::DaemonStatusResult>(Request::DaemonStatus);
        cx.spawn_in(window, async move |this, cx| {
            let web = status
                .await
                .map(|status| status.web)
                .map_err(|error| format!("{error:#}"));
            let local = if open {
                Some(
                    cx.background_executor()
                        .spawn(async move { Local::read(&config_dir, &settings_file) })
                        .await,
                )
            } else {
                None
            };
            let _ = this.update(cx, |shell, cx| {
                let (dark, light) = {
                    let theme = cx.theme();
                    (theme::hex(theme.qr_dark), theme::hex(theme.qr_light))
                };
                shell
                    .phone
                    .polled(web, local, Instant::now(), (&dark, &light));
                cx.notify();
            });
        })
        .detach();
    }

    /// Start / Stop: `[web] enabled` in `settings.toml`; the daemon's settings watcher does the
    /// rest. A failed write shows in the dialog, which stays open (a failure with a place of its
    /// own is not a toast, SPEC §14).
    pub(crate) fn set_phone_enabled(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match mkrew_core::settings::write_web_enabled(&self.settings_file, enabled) {
            Ok(()) => self.phone.written(enabled, Instant::now()),
            Err(error) => {
                tracing::warn!("cannot write [web] enabled: {error}");
                self.phone.write_failed(error.to_string());
            }
        }
        cx.notify();
        self.poll_phone(window, cx);
    }

    /// "Turn on" / "Turn off" of the Phone notifications row: `[web] push` in `settings.toml`.
    /// `mk web` reads it on every request, so there is nothing to wait for. A failed write shows
    /// in the dialog's error row (SPEC §14), not as a toast.
    pub(crate) fn set_phone_push(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match mkrew_core::settings::write_web_push(&self.settings_file, enabled) {
            Ok(()) => {
                self.phone.push = enabled;
                self.phone.notice = None;
            }
            Err(error) => {
                tracing::warn!("cannot write [web] push: {error}");
                self.phone.write_failed(error.to_string());
            }
        }
        cx.notify();
        self.poll_phone(window, cx);
    }

    /// "Set up": `tailscale serve --bg 8787` in the background (it can take a while: 30 s at
    /// most); its error shows under the Tailscale row.
    pub(crate) fn set_up_tailscale(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !self.phone.begin_setup() {
            return;
        }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async {
                    mkrew_core::tailscale::serve_setup(mkrew_core::tailscale::WEB_PORT)
                })
                .await;
            let _ = this.update(cx, |shell, cx| {
                shell.phone.setup_done(done);
                cx.notify();
            });
        })
        .detach();
    }

    /// "Copy link": the link carries the token; it goes to the clipboard and nowhere else
    /// (not to logs, not to a tooltip).
    pub(crate) fn copy_phone_link(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(text) = self.phone.copy_link() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            // The text says "Link copied", never the link: it carries the access token.
            self.toast_info("Link copied", cx);
        }
        cx.notify();
    }

    /// The title bar's phone icon (gray, like every title bar icon): a green dot on its corner
    /// while `mk web` runs. The tooltip says what it is. Hidden without a daemon connection
    /// (the dialog needs it).
    pub(crate) fn render_phone_button(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        self.connection.as_ref()?;
        let on = self.phone.running();
        let text = if on {
            "Phone access: on. Open Mkrew from your phone's browser to watch and answer sessions"
        } else {
            "Phone access: off. Use Mkrew from your phone's browser to watch and answer sessions"
        };
        Some(
            icon_button("phone-access", Icon::Smartphone, theme)
                .relative()
                .when(on, |el| {
                    el.child(
                        div()
                            .absolute()
                            .top(px(space::XS))
                            .right(px(space::XS))
                            .child(status_dot(theme.status_done, false)),
                    )
                })
                .tooltip(tooltip(theme, text, ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.open_phone_access(window, cx);
                })),
        )
    }

    /// The dialog: Status, Tailscale, then the link (QR, text, Copy link) while it is on.
    pub(crate) fn render_phone_access(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let phone = &self.phone;
        let glyph = |mark: Mark| {
            div()
                .flex_none()
                .text_color(rgb(match mark {
                    Mark::On | Mark::Done => theme.accent,
                    Mark::Down => theme.status_exited,
                    Mark::Off | Mark::Wait => theme.text_muted,
                }))
                .child(mark.glyph())
        };
        let line = |text: String| {
            div()
                .flex_1()
                .min_w_0()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .child(text)
        };
        let error_row = |text: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(text))
        };

        let status = phone_dialog::status_row(&phone.daemon, phone.pending());
        let status_row = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(glyph(status.mark))
            .child(line(status.text))
            .children(status.switch.map(|switch| {
                match switch {
                    Switch::Start => button("phone-start", "Start", theme, true)
                        .focus_stop(theme)
                        .tooltip(tooltip(
                            theme,
                            "Turn phone access on: the daemon runs mk web",
                            "",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.set_phone_enabled(true, window, cx);
                        })),
                    Switch::Stop => button("phone-stop", "Stop", theme, false)
                        .focus_stop(theme)
                        .tooltip(tooltip(
                            theme,
                            "Turn phone access off: the phone can no longer reach Mkrew",
                            "",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.set_phone_enabled(false, window, cx);
                        })),
                }
            }));

        let serve = phone_dialog::serve_row(&phone.serve, &phone.setup);
        let serve_row = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(glyph(serve.mark))
            .child(line(serve.text.to_owned()))
            .children(serve.button.map(|state| {
                match state {
                    SetUp::Ready => button("phone-setup", "Set up", theme, false)
                        .focus_stop(theme)
                        .tooltip(tooltip(
                            theme,
                            "Run tailscale serve so the phone reaches Mkrew over HTTPS",
                            "",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.set_up_tailscale(window, cx);
                        })),
                    SetUp::Busy => disabled_button("phone-setup", "Setting up…", theme, false),
                }
            }));
        let push_row = phone_dialog::push_row(&phone.daemon, phone.push).map(|row| {
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(glyph(row.mark))
                .child(line(row.text.to_owned()))
                .child(match row.switch {
                    PushSwitch::TurnOn => button("phone-push-on", "Turn on", theme, false)
                        .focus_stop(theme)
                        .tooltip(tooltip(
                            theme,
                            "Notify the phone when an agent needs you or finishes. The message is encrypted for the phone and goes through Apple's, Google's, Mozilla's or Microsoft's push service",
                            "",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.set_phone_push(true, window, cx);
                        })),
                    PushSwitch::TurnOff => button("phone-push-off", "Turn off", theme, false)
                        .focus_stop(theme)
                        .tooltip(tooltip(theme, "Stop notifying the phone", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.set_phone_push(false, window, cx);
                        })),
                })
        });
        let push_hint = (phone.push && push_row.is_some()).then(|| {
            meta_line(
                theme,
                "On the phone, open Mkrew from the Home Screen and tap the bell.".into(),
            )
        });

        let link: Option<gpui::AnyElement> = match phone.link_block() {
            LinkBlock::Hidden => None,
            LinkBlock::Starting => Some(meta_line(theme, "Starting…".into()).into_any_element()),
            LinkBlock::Ready => phone.link().map(|link| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::M))
                    // Dark modules on the light `qr_light` panel in both themes: phone cameras
                    // read that reliably (the SVG carries its own quiet zone and background).
                    .children(link.image.clone().map(|image| {
                        div().flex().justify_center().child(
                            gpui::img(image)
                                .size(theme::rems_px(theme::PHONE_QR_SIZE))
                                .rounded(px(radius::MEDIUM)),
                        )
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(font_size::META)
                                    .text_color(rgb(theme.text_muted))
                                    .child(link.text.clone()),
                            )
                            .child(
                                button(
                                    "phone-copy",
                                    if phone.copied { "Copied" } else { "Copy link" },
                                    theme,
                                    false,
                                )
                                .focus_stop(theme)
                                .tooltip(tooltip(
                                    theme,
                                    "Copy the link (it carries the access token)",
                                    "",
                                ))
                                .on_click(cx.listener(
                                    |shell, _, _, cx| {
                                        shell.copy_phone_link(cx);
                                    },
                                )),
                            ),
                    )
                    .child(meta_line(
                        theme,
                        "Scan with the iPhone camera, then Share → Add to Home Screen.".into(),
                    ))
                    .into_any_element()
            }),
        };

        let panel = div()
            .id("phone-access-dialog")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::phone_access_key_down),
            )
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Phone access"),
            )
            .child(status_row)
            .children(phone.notice.clone().map(error_row))
            .child(serve_row)
            .children(serve.error.map(error_row))
            .children(push_row)
            .children(push_hint)
            .children(link)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .pt(px(space::S))
                    .child(
                        button("phone-close", "Close", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Close", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_phone_access(window, cx);
                            })),
                    ),
            );
        Self::modal_shade("phone-access-shade", panel, cx)
    }
}
