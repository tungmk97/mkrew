//! The theme catalog's reload (ADR 0101), the gallery's preview and its drawing on
//! Settings → Appearance. The model is `mkrew_ui::settings_dialog`; the catalog is
//! `mkrew_ui::theme_catalog`.

use crate::*;
use mkrew_ui::settings_dialog::Badge;
use mkrew_ui::theme::Palette;
use mkrew_ui::theme_catalog::write_theme_copy;
use mkrew_ui::theme_catalog::{CatalogTheme, FileError};

impl Shell {
    /// "New theme from current": writes `<themes>/<name> copy.toml` (`copy 2`, `copy 3` when
    /// taken) with the palette of the theme shown now and opens it in Zed (spec 3.3). A
    /// failure toasts it and opens nothing; the poll lists the new file within a second.
    pub(crate) fn new_theme_from_current(&mut self, cx: &mut Context<'_, Self>) {
        let shown = cx.theme().clone();
        let catalog = cx.global::<ThemeCatalog>();
        let palette = (catalog.find(&shown.name))
            .map_or_else(|| Palette::from_theme(&shown), |listed| listed.palette);
        match write_theme_copy(&self.themes_dir, catalog, &shown.name, &palette) {
            Err(error) => {
                tracing::warn!("new theme failed: {error}");
                self.toast_error(error, cx);
            }
            Ok(path) => {
                if let Err(error) = zed::open(&path, None) {
                    tracing::warn!(path = %path.display(), "open theme file failed: {error}");
                    self.toast_error(error.to_string(), cx);
                }
            }
        }
    }

    /// "Open themes folder": makes the folder when it is missing and opens it in the OS file
    /// manager, like "Open in File Explorer".
    pub(crate) fn open_themes_folder(&mut self, cx: &mut Context<'_, Self>) {
        match std::fs::create_dir_all(&self.themes_dir) {
            Ok(()) => cx.open_with_system(&self.themes_dir),
            Err(error) => {
                self.toast_error(format!("Cannot create the themes folder: {error}"), cx);
            }
        }
    }
}

impl Shell {
    /// `<config>/themes/` changed: the catalog is read again and the theme shown is rebuilt
    /// from it, so a kept theme whose file was edited shows its new colors at once and one
    /// whose file went falls back like an unknown name. The theme picker lists the old
    /// catalog, so it closes (restoring its preview) rather than pick a shifted index.
    pub(crate) fn apply_themes_dir(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let catalog = ThemeCatalog::load(&self.themes_dir);
        if let Some(dialog) = &mut self.settings_dialog {
            dialog.set_catalog(catalog.clone());
        }
        cx.set_global(catalog);
        if self.theme_pick.is_some() {
            self.close_palette(window, cx);
        }
        let mode = cx.theme().mode;
        self.gallery_preview = None;
        theme::refresh(mode, window, cx);
        cx.refresh_windows();
        self.sync_theme_preview(window, cx);
        cx.notify();
    }

    /// Makes the theme shown follow what the dialog wants previewed (`previewing`): the
    /// card's theme in the mode the app has, or the kept theme again. Nothing is written.
    pub(crate) fn sync_theme_preview(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        let wanted = (self.settings_dialog.as_ref()).and_then(|d| d.previewing().cloned());
        if wanted.as_ref() == self.gallery_preview.as_ref().map(|(_, name)| name) {
            return;
        }
        let Some(name) = wanted else {
            if let Some((mode, _)) = self.gallery_preview.take() {
                theme::refresh(mode, window, cx);
                cx.refresh_windows();
            }
            return;
        };
        let mode = (self.gallery_preview.as_ref()).map_or_else(|| cx.theme().mode, |(m, _)| *m);
        let fonts = cx.global::<theme::Fonts>().clone();
        let Some(listed) = cx.global::<ThemeCatalog>().find(&name) else {
            return;
        };
        let shown = Theme {
            mode,
            ..listed.theme.clone()
        };
        cx.set_global(shown.with_fonts(&fonts));
        self.gallery_preview = Some((mode, name));
        cx.refresh_windows();
    }

    /// The pointer entered card `name`: after the preview delay it shows that theme.
    pub(crate) fn gallery_hover(
        &mut self,
        name: SharedString,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        let serial = dialog.gallery_hover(&name);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::THEME_PREVIEW_DELAY)
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(dialog) = shell.settings_dialog.as_mut() {
                    dialog.gallery_hover_due(serial);
                }
                shell.sync_theme_preview(window, cx);
            });
        })
        .detach();
    }

    /// The pointer left the gallery: the kept theme comes back.
    pub(crate) fn gallery_leave(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        if let Some(dialog) = self.settings_dialog.as_mut() {
            dialog.gallery_leave();
        }
        self.sync_theme_preview(window, cx);
    }

    /// The gallery: the "Themes" overline, the Dark and Light groups of cards, one card per
    /// file that is not loaded, and the folder line (spec 3.1).
    pub(crate) fn render_theme_gallery(
        &self,
        dialog: &SettingsDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let mut cards = div()
            .id("theme-gallery")
            .flex()
            .flex_col()
            .gap(px(space::M))
            // Leaving the whole gallery (not one card) puts the kept theme back. A key press
            // is not leaving: the pointer may rest here while the keyboard previews.
            .hover_listener_mode(gpui::HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(|shell, hovered: &bool, window, cx| {
                if !*hovered {
                    shell.gallery_leave(window, cx);
                }
            }))
            .child(overline("Themes", theme).px(px(space::S)));
        for (title, dark) in [("Dark", true), ("Light", false)] {
            let listed: Vec<&CatalogTheme> = dialog.catalog.group(dark).collect();
            if listed.is_empty() {
                continue;
            }
            let row: Vec<gpui::AnyElement> = listed
                .into_iter()
                .map(|card| self.theme_card(dialog, card, theme, cx))
                .collect();
            cards = cards.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(overline(title, theme).px(px(space::S)))
                    .child(div().flex().flex_wrap().gap(px(space::M)).children(row)),
            );
        }
        let errors = dialog.catalog.errors();
        if !errors.is_empty() {
            cards = cards.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::M))
                    .children(errors.iter().map(|error| theme_error_card(error, theme))),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(cards)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .child(div().flex_1().min_w_0().child(meta_line(
                        theme,
                        self.themes_dir.display().to_string().into(),
                    )))
                    .child(
                        quiet_button(
                            "theme-new-from-current",
                            "New theme from current",
                            theme,
                            theme.hover,
                        )
                        .on_press(
                            true,
                            cx.listener(|shell, _, _, cx| shell.new_theme_from_current(cx)),
                        ),
                    )
                    .child(
                        quiet_button(
                            "theme-open-folder",
                            "Open themes folder",
                            theme,
                            theme.hover,
                        )
                        .on_press(
                            true,
                            cx.listener(|shell, _, _, cx| shell.open_themes_folder(cx)),
                        ),
                    ),
            )
            .into_any_element()
    }

    /// One card: the picture, the name and the badges; one `card_press` element.
    fn theme_card(
        &self,
        dialog: &SettingsDialog,
        card: &CatalogTheme,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let name = card.name.clone();
        let badges = SettingsDialog::card_badges(card, dialog.kept_names());
        let kept = badges
            .clone()
            .any(|badge| matches!(badge, Badge::DarkMode | Badge::LightMode));
        let focused = dialog.focused_card().is_some_and(|c| c.name == card.name);
        let ring = if focused {
            theme.focus_ring
        } else if kept {
            theme.selected_ring
        } else {
            theme.divider
        };
        let hover_name = name.clone();
        let press_name = name.clone();
        let badge_row = badges.into_iter().map(|badge| {
            let color = match badge {
                Badge::DarkMode | Badge::LightMode => theme.accent_text,
                Badge::File => theme.text_muted,
                Badge::LowContrast => theme.diff_mod_text,
            };
            let text = div()
                .text_size(font_size::META)
                .text_color(rgb(color))
                .child(badge.label());
            match (badge, card.problems.first()) {
                (Badge::LowContrast, Some(problem)) => text
                    .id(SharedString::from(format!("theme-low-{name}")))
                    .tooltip(tooltip(theme, &problem.to_string(), ""))
                    .into_any_element(),
                _ => text.into_any_element(),
            }
        });
        div()
            .id(SharedString::from(format!("theme-card-{name}")))
            .flex_none()
            .w(px(theme::THEME_CARD_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface))
            // Always bordered, so a ring shifts nothing; at rest the divider, so the hover
            // border of `card_press` shows.
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(ring))
            .map(|el| card_press(el, kept || focused, theme))
            // After a key moved to this card, it scrolls itself into view when it is painted,
            // by the least that shows it whole (its bounds are this frame's).
            .when(focused, |el| {
                let reveal = self.gallery_reveal.clone();
                let scroll = self.settings_scroll.clone();
                el.child(
                    canvas(
                        |_, _, _| (),
                        move |card, (), window, _| {
                            if !reveal.replace(false) {
                                return;
                            }
                            let view = scroll.bounds();
                            let mut offset = scroll.offset();
                            if card.top() < view.top() {
                                offset.y += view.top() - card.top();
                            } else if card.bottom() > view.bottom() {
                                offset.y -= card.bottom() - view.bottom();
                            } else {
                                return;
                            }
                            offset.y = offset.y.clamp(-scroll.max_offset().y, px(0.));
                            scroll.set_offset(offset);
                            window.refresh();
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .on_hover(cx.listener(move |shell, hovered: &bool, window, cx| {
                if *hovered {
                    shell.gallery_hover(hover_name.clone(), window, cx);
                }
            }))
            .on_press(
                true,
                cx.listener(move |shell, _, window, cx| {
                    let name = press_name.clone();
                    shell.settings_do(move |d, keymap| d.gallery_click(&name, keymap), window, cx);
                }),
            )
            .child(theme_picture(&card.theme))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(card.name.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .min_h(theme::rems_px(theme::CHIP_HEIGHT))
                    .children(badge_row),
            )
            .into_any_element()
    }
}

/// A small window drawn in `card`'s own colors, whatever theme the app has: the chrome strip,
/// the sidebar strip, two text bars, an accent pill and the six hues of its ANSI palette.
fn theme_picture(card: &Theme) -> gpui::Div {
    let bar = |color: u32, width: f32| {
        div()
            .h(px(theme::THEME_BAR_HEIGHT))
            .w(px(width))
            .rounded(px(radius::SMALL))
            .bg(rgb(color))
    };
    let dots = card.ansi[1..=6].iter().map(|color| {
        div()
            .size(px(theme::THEME_DOT))
            .rounded_full()
            .bg(rgb(*color))
    });
    div()
        .relative()
        .w_full()
        .h(px(theme::THEME_PICTURE_HEIGHT))
        .rounded(px(radius::MEDIUM))
        .overflow_hidden()
        .bg(rgb(card.bg))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(card.border))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(theme::THEME_PICTURE_CHROME))
                .bg(rgb(card.chrome)),
        )
        .child(
            div()
                .absolute()
                .top(px(theme::THEME_PICTURE_CHROME))
                .bottom_0()
                .left_0()
                .w(px(theme::THEME_PICTURE_SIDEBAR))
                .bg(rgb(card.sidebar)),
        )
        .child(
            div()
                .absolute()
                .top(px(theme::THEME_PICTURE_CHROME + space::S))
                .left(px(theme::THEME_PICTURE_SIDEBAR + space::S))
                .flex()
                .flex_col()
                .gap(px(space::S))
                .child(bar(card.text, theme::THEME_BAR_LONG))
                .child(bar(card.text_muted, theme::THEME_BAR_SHORT))
                .child(
                    div()
                        .h(px(theme::THEME_PILL_HEIGHT))
                        .w(px(theme::THEME_PILL_WIDTH))
                        .rounded_full()
                        .bg(rgb(card.accent)),
                )
                .child(div().flex().gap(px(space::XS)).children(dots)),
        )
}

/// A file that is not loaded: its name and why, in the danger color; not clickable, so no
/// press look.
fn theme_error_card(error: &FileError, theme: &Theme) -> gpui::AnyElement {
    div()
        .flex_none()
        .w(px(theme::THEME_CARD_WIDTH))
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .p(px(space::S))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .child(error.file.clone()),
        )
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.danger_text))
                .child(error.message.clone()),
        )
        .into_any_element()
}
