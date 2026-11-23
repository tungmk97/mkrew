//! What a detached window shows (ADR 0096, ADR 0106): the shared title bar with its tabs, the
//! active tab.

use crate::*;

impl Shell {
    pub(crate) fn render_host(
        &mut self,
        key: WindowKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let host = HostId::Detached(key);
        // A menu closed behind the keyboard's back, or a dialog control that vanished while
        // focused, must not leave the keyboard on nothing (as `Render for Shell` does).
        if self.menu_host == host && self.menu_open() && self.open_menu_rows(window).is_none() {
            self.dismiss_menus(window, cx);
        }
        if !self.menu_open() && self.menu_focus.contains_focused(window, cx) {
            self.focus_pane(window, cx);
        }
        if self.popup_host() == Some(host)
            && !window.has_active_prompt()
            && !self.dialog_focus.contains_focused(window, cx)
        {
            window.focus(&self.dialog_focus, cx);
        }
        // Rendering marks the focused pane of the active window viewed (its done leaves the
        // Inbox), as `Render for Shell` does for the main window.
        let mut viewed_changed = false;
        if window.is_window_active()
            && let Some(id) = self.focused_session(window)
            && let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == id)
        {
            viewed_changed = self.viewed.view(session);
        }
        if viewed_changed {
            self.sync_attention(cx);
        }
        let now = status::now_ms();
        window.set_rem_size(px(cx.global::<theme::Fonts>().ui_size));
        self.sync_traffic_lights(host, window, cx.global::<theme::Fonts>().ui_size);
        cx.global::<MenuAnchors>()
            .clear(window.window_handle().window_id());
        let scope = self.tab_scope(window);
        let title = scope
            .and_then(|(project, tab_id)| self.tab_title(project, tab_id))
            .unwrap_or_else(|| "Mkrew".to_owned());
        // Only on a change: setting it is a system call, and a frame comes with every output.
        if let Some(ui) = self.hosts.get_mut(&host)
            && ui.title != title
        {
            window.set_window_title(&title);
            ui.title = title.clone();
        }
        let content = (self.hosts.get(&host))
            .map(|ui| ui.content.clone())
            .unwrap_or_default();
        let body = match scope {
            Some((project, tab_id)) => {
                let tab = self.tabs.get(&project).and_then(|tabs| tabs.tab(tab_id));
                self.render_tab(
                    project,
                    tab,
                    now,
                    self.composer_focus.is_focused(window),
                    None,
                    &theme,
                    content,
                    window,
                    cx,
                )
                .into_any_element()
            }
            None => message(&theme, None, "Loading…".into(), None)
                .flex_1()
                .into_any_element(),
        };
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .font_family(theme.ui_font_family.clone())
            .child(tear_off_catcher(cx))
            .child(self.render_title_bar(host, now, &theme, window, cx))
            .children(self.render_reconnect_banner(&theme, cx))
            .child(body)
            .children(self.reconnecting().then(inert_layer))
            .children(self.detached_overlays(host, &theme, window, cx))
            .into_any_element()
    }

    /// The popovers drawn over `host`'s window: the menus, the palette and the toasts that
    /// belong to it (the main window draws its own in `Render for Shell`).
    fn detached_overlays(
        &self,
        host: HostId,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut overlays = Vec::new();
        if self.menu_host == host && !self.modal_open() {
            overlays.extend(
                (self.tab_menu.as_ref()).map(|menu| self.render_tab_menu(menu, window, theme, cx)),
            );
            overlays.extend(
                (self.layout_menu.as_ref()).map(|menu| self.render_layout_menu(menu, theme, cx)),
            );
            overlays.extend(
                (self.pane_menu.as_ref()).and_then(|menu| self.render_pane_menu(menu, theme, cx)),
            );
            overlays.extend(
                (self.share_menu.as_ref()).map(|menu| self.render_share_menu(menu, theme, cx)),
            );
        }
        if self.palette_host == host {
            overlays.extend(
                (self.palette.as_ref()).map(|palette| self.render_palette(palette, theme, cx)),
            );
        }
        if self.picker_host == host {
            overlays.extend(
                (self.prompt_picker.as_ref())
                    .map(|picker| self.render_prompt_picker(picker, theme, cx)),
            );
            overlays.extend(
                (self.slash_picker.as_ref())
                    .map(|picker| self.render_slash_picker(picker, theme, cx)),
            );
        }
        if self.toast_host() == host {
            overlays.extend(self.render_toasts(theme, cx));
        }
        overlays
    }

    /// The tab area of a detached window's title bar: a chip per tab, its workspace name in
    /// front. No `+`: new tabs are made in the main window. The strip and chip bounds are kept
    /// in `HostUi::strip` for drops from other windows (`record_strip`).
    pub(crate) fn render_detached_tab_area(
        &self,
        key: WindowKey,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let host = HostId::Detached(key);
        let tabs = self.detached.tabs_of(key);
        let active = self.detached.active(key);
        let strip = (self.hosts.get(&host))
            .map(|ui| ui.strip.clone())
            .unwrap_or_default();
        let sessions = self.sidebar.sessions();
        let chips: Vec<gpui::AnyElement> = tabs
            .iter()
            .enumerate()
            .filter_map(|(index, dock)| {
                let DockTab::Terminal { project, tab_id } = *dock;
                let tab = self.tabs.get(&project)?.tab(tab_id)?;
                // The tab's own rename field takes its place, also while its pane is renamed.
                if let Some(renaming) = self.tab_renaming(tab) {
                    return Some(self.render_rename(&renaming.draft, theme, cx));
                }
                let workspace = (self.sidebar.projects().iter())
                    .find(|p| p.id == project)
                    .map(|p| p.name.as_str());
                let file = file_label(tab, self.documents.get(&project));
                let item = DraggedItem {
                    list: ReorderList::Detached(key),
                    index,
                    title: (file.as_ref().map(|file| file.name.as_str()))
                        .or_else(|| session_label(tab, sessions))
                        .unwrap_or(&tab.title)
                        .to_owned()
                        .into(),
                };
                let chip = self
                    .tab_chip(
                        project,
                        tab,
                        active == Some(*dock),
                        workspace,
                        move |shell: &mut Shell| {
                            shell.detached.select(key, index);
                            shell.windows_dirty = true;
                        },
                        theme,
                        cx,
                    )
                    .child(record_strip(strip.clone(), Some(index)));
                Some(self.reorderable_shrinking(
                    chip.into_any_element(),
                    item,
                    true,
                    space::XS,
                    Some(theme::TAB_MIN_WIDTH),
                    theme,
                    cx,
                ))
            })
            .collect();
        // The key is the scroll target only, as in `render_tab_area`; the index counts the
        // chips built, which skip a tab that no longer resolves.
        let active_chip = active.and_then(|DockTab::Terminal { tab_id, .. }| {
            let index = (tabs.iter())
                .filter(|DockTab::Terminal { project, tab_id }| {
                    (self.tabs.get(project)).is_some_and(|set| set.tab(*tab_id).is_some())
                })
                .position(|dock| Some(*dock) == active)?;
            Some((SharedString::from(format!("tab/{tab_id:?}")), index))
        });
        self.tab_area_frame(host, chips, active_chip, None, theme, cx)
            .into_any_element()
    }
}
