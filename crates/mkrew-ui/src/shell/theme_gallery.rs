//! The theme catalog's reload (ADR 0101), the gallery's preview and its drawing on
//! Settings → Appearance. The model is `mkrew_ui::settings_dialog`; the catalog is
//! `mkrew_ui::theme_catalog`.

use crate::*;

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
        theme::refresh(mode, window, cx);
        cx.refresh_windows();
        cx.notify();
    }
}
