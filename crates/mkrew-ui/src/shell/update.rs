//! In-app update (SPEC §20K, ADR 0071): the check, the title bar button and the apply step.

use mkrew_ui::update::{self, InstallKind, Release};

use crate::*;

const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// `window.prompt` answers; Cancel is also Esc. "What's new" opens the release page and asks
/// again.
const ANSWERS: [&str; 3] = ["Update", "What's new", "Cancel"];

impl Shell {
    /// Asks GitHub for the newest release. `manual` (palette) always answers with a toast.
    pub(crate) fn check_updates(
        &mut self,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { update::fetch_latest() })
                .await;
            let _ = this.update(cx, |shell, cx| shell.finish_check(manual, result, cx));
        })
        .detach();
    }

    fn finish_check(
        &mut self,
        manual: bool,
        result: anyhow::Result<Option<Release>>,
        cx: &mut Context<'_, Self>,
    ) {
        match result {
            Ok(Some(release)) if update::is_newer(&release.version, CURRENT) => {
                let fresh = self
                    .update
                    .as_ref()
                    .is_none_or(|known| known.version != release.version);
                if fresh || manual {
                    self.toast_info(format!("Mkrew {} is available", release.version), cx);
                }
                self.update = Some(release);
            }
            Ok(_) if manual => self.toast_info(format!("Mkrew {CURRENT} is up to date"), cx),
            Ok(_) => {}
            Err(error) => {
                tracing::warn!("update check: {error:#}");
                if manual {
                    self.toast_error(format!("Update check failed: {error:#}"), cx);
                }
            }
        }
        cx.notify();
    }

    /// The title bar button while a newer release is known; "Updating…" shows beside it
    /// while the download runs (the tooltip says which version).
    pub(crate) fn render_update_button(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let release = self.update.as_ref()?;
        let text = if self.updating {
            format!("Downloading Mkrew {}…", release.version)
        } else {
            format!("Update to Mkrew {} (you have {CURRENT})", release.version)
        };
        let button = icon_button_in("update", Icon::CircleArrowUp, theme.accent_text, theme)
            .tooltip(tooltip(theme, &text, ""))
            .when(self.updating, |el| el.opacity(opacity::DISABLED))
            .when(!self.updating, |el| {
                el.on_click(cx.listener(|shell, _, window, cx| {
                    shell.start_update(window, cx);
                }))
            });
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .child(button)
                .when(self.updating, |row| {
                    row.child(
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.accent_text))
                            .child("Updating…"),
                    )
                }),
        )
    }

    /// Click on the update button: confirm, download, put in place, quit (the new version opens).
    pub(crate) fn start_update(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(release) = self.update.clone() else {
            return;
        };
        let kind = update::current_install();
        let asset = update::pick_asset(
            std::env::consts::OS,
            std::env::consts::ARCH,
            &release.assets,
        )
        .cloned();
        let (InstallKind::Inno { .. } | InstallKind::MacApp { .. }, Some(asset)) = (&kind, asset)
        else {
            cx.open_url(&release.page_url);
            return;
        };
        let again: AfterSave = Box::new(|shell, window, cx| shell.start_update(window, cx));
        if self.ask_unsaved(again, window, cx) {
            return;
        }
        let working = self
            .sidebar
            .sessions()
            .iter()
            .filter(|s| s.kind == SessionKind::Agent && s.state == SessionState::Working)
            .count();
        let detail = update::prompt_detail(working);
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Update to Mkrew {}?", release.version),
            Some(detail.as_str()),
            &ANSWERS,
            cx,
        );
        let page_url = release.page_url.clone();
        cx.spawn_in(window, async move |this, cx| {
            let choice = answer.await.ok();
            if choice == Some(1) {
                let _ = this.update_in(cx, |shell, window, cx| {
                    cx.open_url(&page_url);
                    shell.start_update(window, cx);
                });
                return;
            }
            if choice != Some(0) {
                return;
            }
            let _ = this.update(cx, |shell, cx| {
                shell.updating = true;
                cx.notify();
            });
            let dir = std::env::temp_dir().join("mkrew-update");
            let file = cx
                .background_executor()
                .spawn(async move { update::download(&asset, &dir) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| match file {
                Ok(file) => shell.apply_update(kind, file, window, cx),
                Err(error) => shell.update_failed(&error, cx),
            });
        })
        .detach();
    }

    /// The download is there: ask about Files edits made meanwhile (the installer closes the app),
    /// then put it in place and quit. Cancel leaves the app running; a click on ⬆ starts over.
    fn apply_update(
        &mut self,
        kind: InstallKind,
        file: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (again_kind, again_file) = (kind.clone(), file.clone());
        let again: AfterSave = Box::new(move |shell, window, cx| {
            shell.apply_update(again_kind, again_file, window, cx);
        });
        if self.ask_unsaved(again, window, cx) {
            self.updating = false;
            cx.notify();
            return;
        }
        self.updating = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { update::apply(&kind, &file) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| match result {
                Ok(()) => shell.quit_ui(window, cx),
                Err(error) => shell.update_failed(&error, cx),
            });
        })
        .detach();
    }

    fn update_failed(&mut self, error: &anyhow::Error, cx: &mut Context<'_, Self>) {
        self.updating = false;
        self.toast_error(format!("Update failed: {error:#}"), cx);
    }
}
