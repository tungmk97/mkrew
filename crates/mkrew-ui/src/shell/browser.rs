//! Browser panes (ADR 0110): the Shell's page state per web leaf, and the pane's body.

use crate::*;
use mkrew_ui::browser::{NativeView, Page, PageEvent};

/// One browser pane's live state, keyed by its leaf id in `Shell.browsers`.
pub(crate) struct Browser {
    pub(crate) page: Page,
    /// The page's view (Windows and macOS), built the first time the pane is drawn with a URL.
    pub(crate) native: Option<NativeView>,
    /// The window `native` is a child of.
    pub(crate) host: Option<HostId>,
    /// Why `native` could not be built (spec 5.3); the pane offers the system browser.
    pub(crate) error: Option<String>,
    /// The pane's keyboard (`BROWSER_CONTEXT`).
    pub(crate) focus: FocusHandle,
    pub(crate) address: Input,
    // Read by the address bar (Task 6).
    #[allow(dead_code)]
    pub(crate) address_focus: FocusHandle,
    /// The page has the keyboard (its last IPC was a focus).
    pub(crate) page_keys: bool,
    /// The address field is being edited.
    pub(crate) editing: bool,
}

impl Shell {
    /// The workspace whose tabs hold browser pane `id`.
    pub(crate) fn web_project(&self, id: Ulid) -> Option<Ulid> {
        let key = PaneKey::Web(id);
        (self.tabs.iter())
            .find(|(_, tabs)| tabs.tab_with(&key).is_some())
            .map(|(project, _)| *project)
    }

    /// Every web leaf of every layout has its state, and only those: a closed pane's page goes.
    pub(crate) fn sync_browsers(&mut self, cx: &mut Context<'_, Self>) {
        let mut leaves: HashMap<Ulid, String> = HashMap::new();
        for (project, tabs) in &self.tabs {
            // Tabs still loading list no leaves yet: nothing may be dropped.
            if self.loading_tabs.contains(project) {
                return;
            }
            for tab in tabs.tabs() {
                let Some(layout) = tab.layout() else { continue };
                for key in tab.keys() {
                    if let (PaneKey::Web(id), Some(Leaf::Web { web, .. })) =
                        (&key, layout.leaf_of(&key))
                    {
                        leaves.insert(*id, web.clone());
                    }
                }
            }
        }
        self.browsers.retain(|id, _| leaves.contains_key(id));
        for (id, url) in leaves {
            self.browsers.entry(id).or_insert_with(|| {
                let mut address = Input::default();
                address.insert(&url);
                Browser {
                    page: Page::new(url),
                    native: None,
                    host: None,
                    error: None,
                    focus: cx.focus_handle(),
                    address,
                    address_focus: cx.focus_handle(),
                    page_keys: false,
                    editing: false,
                }
            });
        }
    }

    /// What a page reported (spec 6.1, 7.4).
    pub(crate) fn page_event(
        &mut self,
        id: Ulid,
        event: PageEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(browser) = self.browsers.get_mut(&id) else {
            return;
        };
        match event {
            PageEvent::Started => browser.page.loading = true,
            PageEvent::Title(title) => browser.page.title = title,
            PageEvent::Open(url) => {
                if let Some(native) = &browser.native {
                    native.load(&url);
                }
            }
            PageEvent::Finished(url) => {
                browser.page.loading = false;
                if !browser.editing {
                    browser.address.clear();
                    browser.address.insert(&url);
                }
                browser.page.url = url.clone();
                self.store_url(id, url, window, cx);
            }
            PageEvent::Focus => {
                browser.page_keys = true;
                if let Some(project) = self.web_project(id) {
                    self.focus_pane_key(project, &PaneKey::Web(id));
                }
            }
            PageEvent::Key(name) => {
                let command = gpui::Keystroke::parse(&name)
                    .ok()
                    .and_then(|keystroke| self.keymap.command_for(&keystroke));
                if let (Some(command), Some(host)) = (command, browser.host) {
                    self.in_window(host, cx, move |shell, window, cx| {
                        shell.run(&Run(command), window, cx);
                    });
                }
            }
        }
        cx.notify();
    }

    /// Saves the page's URL in its leaf (one `tab.set_layout` per finished load).
    fn store_url(
        &mut self,
        id: Ulid,
        url: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.web_project(id) else {
            return;
        };
        let write = (self.tabs.get_mut(&project_id)).and_then(|tabs| {
            tabs.update_leaf(&PaneKey::Web(id), |leaf| {
                if let Leaf::Web { web, .. } = leaf {
                    *web = url;
                }
            })
        });
        if let Some(write) = write {
            self.save_layout(project_id, write, window, cx);
        }
    }

    /// Loads `url` in browser pane `id`: in its page, or (no page yet) as the leaf's URL, which
    /// builds the page on the next draw.
    // Called by the address bar (Task 6).
    #[allow(dead_code)]
    pub(crate) fn navigate(
        &mut self,
        id: Ulid,
        url: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(browser) = self.browsers.get_mut(&id) else {
            return;
        };
        browser.editing = false;
        browser.address.clear();
        browser.address.insert(&url);
        browser.page.url = url.clone();
        match &browser.native {
            Some(native) => native.load(&url),
            None => self.store_url(id, url, window, cx),
        }
        cx.notify();
    }

    /// An overlay above the panes is open: every page hides so it shows (spec 7.2).
    /// ponytail: one rule for every window; a palette in one window hides pages in all.
    pub(crate) fn browser_overlay(&self, cx: &App) -> bool {
        self.modal_open()
            || self.menu_open()
            || self.peek.is_some()
            || self.drop_hover.is_some()
            || cx.has_active_drag()
    }

    /// Runs at the start of `host`'s render: builds the pages of this window's shown web leaves,
    /// moves the ones that changed window, and hides every page of this window not on screen.
    pub(crate) fn prepare_browsers(
        &mut self,
        host: HostId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.browsers.is_empty() {
            return;
        }
        let shown: Vec<Ulid> = self.shown_browsers(host, window);
        let focused = self.focused_key(window);
        let dark = theme::slot_is_dark(cx.theme().mode, theme::os_dark(window));
        let keys = mkrew_ui::browser::keys::forwarded(&self.keymap);
        let data_dir = self.paths.data_dir.join("WebView2");
        let usable = mkrew_ui::browser::available().is_ok();
        for (id, browser) in &mut self.browsers {
            let here = shown.contains(id);
            if here && browser.host != Some(host) {
                // A page built in another window is rebuilt here on its URL (history is lost):
                // wry 0.57 keeps the first parent of a child view, so `reparent` would leave
                // its focus target in the old window.
                browser.native = None;
                browser.host = Some(host);
            }
            if here
                && usable
                && browser.native.is_none()
                && browser.error.is_none()
                && !browser.page.url.is_empty()
            {
                match NativeView::build(
                    window,
                    *id,
                    &browser.page.url,
                    &keys,
                    dark,
                    Some(&data_dir),
                    self.page_events.clone(),
                ) {
                    Ok(native) => browser.native = Some(native),
                    Err(error) => {
                        tracing::warn!(%id, "browser: cannot build the page: {error}");
                        browser.error = Some(error);
                    }
                }
            }
            let Some(native) = &browser.native else {
                continue;
            };
            if browser.host == Some(host) && !here {
                native.set_shown(false);
            }
            if here {
                native.set_dark(dark);
                // The keyboard left the page's pane: give it back to the window.
                if browser.page_keys && focused != Some(PaneKey::Web(*id)) {
                    native.focus_parent();
                    browser.page_keys = false;
                }
            }
        }
    }

    /// The web leaves `host` shows now: its tab's shown panes (a zoom shows one), none while
    /// the main window shows another view.
    fn shown_browsers(&self, host: HostId, window: &Window) -> Vec<Ulid> {
        let Some((project, tab_id)) = self.tab_scope(window) else {
            return Vec::new();
        };
        debug_assert_eq!(self.host_of(window), host);
        (self.tabs.get(&project).and_then(|tabs| tabs.tab(tab_id)))
            .map(|tab| {
                tab.shown()
                    .into_iter()
                    .filter_map(|key| match key {
                        PaneKey::Web(id) => Some(id),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Closes browser pane `id` (its tab goes with its last pane).
    pub(crate) fn close_web_pane(
        &mut self,
        id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(project_id) = self.web_project(id) {
            self.remove_leaf(project_id, PaneKey::Web(id), window, cx);
        }
    }

    /// The body of browser pane `id`: the page, or why there is none.
    pub(crate) fn render_web_pane(
        &self,
        _project_id: Ulid,
        id: Ulid,
        _is_focused: bool,
        theme: &Theme,
        _window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(browser) = self.browsers.get(&id) else {
            return message(theme, None, "Loading…".into(), None)
                .size_full()
                .into_any_element();
        };
        let body = match (&browser.native, &browser.error) {
            (Some(native), _) if !self.browser_overlay(cx) => {
                native.surface(true).into_any_element()
            }
            (Some(native), _) => div()
                .size_full()
                .child(native.surface(false))
                .child(self.page_card(browser, theme))
                .into_any_element(),
            (None, Some(error)) => self.unavailable_body(id, error, theme, cx),
            (None, None) => match mkrew_ui::browser::available() {
                Err(reason) => self.unavailable_body(id, &reason.to_string(), theme, cx),
                Ok(()) => empty_state(
                    theme,
                    None,
                    None,
                    "Type an address or paste a link".into(),
                    None,
                )
                .size_full()
                .into_any_element(),
            },
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.surface))
            .child(body)
            .into_any_element()
    }

    /// Where the page is while an overlay hides it: its title and URL.
    fn page_card(&self, browser: &Browser, theme: &Theme) -> gpui::Div {
        empty_state(
            theme,
            None,
            None,
            browser.page.label().to_owned().into(),
            Some(browser.page.url.clone().into()),
        )
        .absolute()
        .inset_0()
    }

    /// The page cannot be shown here (spec 5.3): the reason and the system browser.
    fn unavailable_body(
        &self,
        id: Ulid,
        reason: &str,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::M))
            .child(message(theme, None, reason.to_owned().into(), None))
            .child(
                button("web-open-external", "Open in system browser", theme, false)
                    .on_click(cx.listener(move |shell, _, _, cx| shell.open_external(id, cx))),
            )
            .into_any_element()
    }

    /// The page in the system browser.
    pub(crate) fn open_external(&mut self, id: Ulid, cx: &mut Context<'_, Self>) {
        if let Some(url) = (self.browsers.get(&id))
            .map(|b| b.page.url.clone())
            .filter(|url| !url.is_empty())
        {
            cx.open_url(&url);
        }
    }
}
