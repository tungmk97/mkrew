//! Browser panes (ADR 0111): the Shell's page state per web leaf, and the pane's body.

use crate::*;
use mkrew_ui::browser::{NativeView, Page, PageEvent, Parent};

/// One browser pane's live state, keyed by its leaf id in `Shell.browsers`.
pub(crate) struct Browser {
    /// The workspace whose tabs hold the pane.
    pub(crate) project: Ulid,
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
    /// The address field's keyboard.
    pub(crate) address_focus: FocusHandle,
    /// The page has the keyboard (its last IPC was a focus).
    pub(crate) page_keys: bool,
    /// The key list the page's script holds (`forwarded` at its build or last patch).
    pub(crate) keys: Vec<String>,
    /// The address field is being edited.
    pub(crate) editing: bool,
    /// The page is being built (`prepare_browsers`); `native` comes with `page_built`.
    pub(crate) building: bool,
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
        let mut leaves: HashMap<Ulid, (Ulid, String)> = HashMap::new();
        for (project, tabs) in &self.tabs {
            // Tabs still loading list no leaves yet: their panes keep their state.
            if self.loading_tabs.contains(project) {
                continue;
            }
            for tab in tabs.tabs() {
                let Some(layout) = tab.layout() else { continue };
                for key in tab.keys() {
                    if let (PaneKey::Web(id), Some(Leaf::Web { web, .. })) =
                        (&key, layout.leaf_of(&key))
                    {
                        leaves.insert(*id, (*project, web.clone()));
                    }
                }
            }
        }
        let loading = &self.loading_tabs;
        self.browsers
            .retain(|id, browser| leaves.contains_key(id) || loading.contains(&browser.project));
        for (id, (project, url)) in leaves {
            self.browsers.entry(id).or_insert_with(|| {
                let mut address = Input::default();
                address.insert(&url);
                Browser {
                    project,
                    page: Page::new(url),
                    native: None,
                    host: None,
                    error: None,
                    focus: cx.focus_handle(),
                    address,
                    address_focus: cx.focus_handle(),
                    page_keys: false,
                    keys: Vec::new(),
                    editing: false,
                    building: false,
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
            PageEvent::Open(url) if !browser.native.as_ref().is_some_and(NativeView::is_shown) => {
                tracing::debug!(%id, "browser: dropped a new-window request of a hidden page: {url}");
            }
            PageEvent::Open(url) => match mkrew_ui::browser::new_window_target(&url) {
                mkrew_ui::browser::NewWindow::Load => {
                    if let Some(native) = &browser.native {
                        native.load(&url);
                    }
                }
                mkrew_ui::browser::NewWindow::System => cx.open_url(&url),
                mkrew_ui::browser::NewWindow::Drop => {
                    tracing::debug!(%id, "browser: dropped a new-window request: {url}");
                }
            },
            PageEvent::Finished(url) => {
                browser.page.loading = false;
                if !browser.editing {
                    browser.address.clear();
                    browser.address.insert(&url);
                }
                browser.page.url = url.clone();
                // The page's init script carries the key list of its build: bring it up to
                // date after a keymap change.
                let keys = mkrew_ui::browser::keys::forwarded(&self.keymap);
                if browser.keys != keys {
                    if let Some(native) = &browser.native {
                        native.script(&mkrew_ui::browser::keys::set_keys_script(&keys));
                    }
                    browser.keys = keys;
                }
                self.store_url(id, url, window, cx);
            }
            // A page that is not on screen, or in a window that is not the active one, has no
            // say over the window's focus.
            PageEvent::Focus
                if !browser.native.as_ref().is_some_and(NativeView::is_shown)
                    || !browser.host.is_some_and(|h| {
                        host_active(h, self.main_window, &self.window_handles, cx)
                    }) => {}
            PageEvent::Focus => {
                browser.page_keys = true;
                if browser.editing {
                    // The page took the keyboard from the address field.
                    browser.editing = false;
                    let url = browser.page.url.clone();
                    browser.address.clear();
                    browser.address.insert(&url);
                }
                if let Some(host) = browser.host {
                    // GPUI focus moves in the page's own window.
                    self.in_window(host, cx, move |shell, window, cx| {
                        if let Some(browser) = shell.browsers.get(&id) {
                            window.focus(&browser.focus, cx);
                        }
                    });
                }
                if let Some(project) = self.web_project(id) {
                    self.focus_pane_key(project, &PaneKey::Web(id));
                }
            }
            PageEvent::Key(name) => {
                // Only a page that has the keyboard in its focused pane of the active window,
                // and only a forwarded global or browser binding (ADR 0111): the page is not
                // trusted.
                if let (true, Some(host)) = (browser.page_keys, browser.host)
                    && host_active(host, self.main_window, &self.window_handles, cx)
                {
                    self.in_window(host, cx, move |shell, window, cx| {
                        if shell.focused_key(window) != Some(PaneKey::Web(id))
                            || !mkrew_ui::browser::keys::forwarded(&shell.keymap).contains(&name)
                        {
                            return;
                        }
                        let command = gpui::Keystroke::parse(&name)
                            .ok()
                            .and_then(|keystroke| shell.keymap.forwarded_command(&keystroke));
                        if let Some(command) = command {
                            shell.run(&Run(command), window, cx);
                        }
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
    /// builds the page on the next draw. A pane whose build failed tries again with it.
    pub(crate) fn navigate(
        &mut self,
        id: Ulid,
        url: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(browser) = self.browsers.get_mut(&id) else {
            // A link during startup: the leaf's URL builds the page once the tabs are in.
            return self.store_url(id, url, window, cx);
        };
        browser.editing = false;
        browser.error = None;
        browser.address.clear();
        browser.address.insert(&url);
        browser.page.url = url.clone();
        match &browser.native {
            Some(native) => native.load(&url),
            None => self.store_url(id, url, window, cx),
        }
        cx.notify();
    }

    /// A new empty browser pane with its address field focused: on an edge of a pane
    /// (`at`), or in a new tab (`None`). New tabs belong to the main window.
    pub(crate) fn new_browser(
        &mut self,
        at: Option<(Ulid, PaneKey, Edge)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if mkrew_ui::browser::available().is_err() {
            return;
        }
        let Some(project_id) = (at.as_ref().map(|(project, ..)| *project))
            .or_else(|| self.tab_scope(window).map(|(project, _)| project))
            .or(self.workspace)
        else {
            return;
        };
        let leaf = Leaf::web("");
        let Leaf::Web { id, .. } = leaf else { return };
        let place = match at {
            Some((_, key, edge)) => documents::Place::Split(key, edge),
            None => {
                if self.host_of(window) != HostId::Main {
                    return self.in_window(HostId::Main, cx, move |shell, window, cx| {
                        window.activate_window();
                        shell.new_browser(None, window, cx);
                    });
                }
                self.views.insert(project_id, View::Terminals);
                if self.workspace != Some(project_id) {
                    self.activate_workspace(project_id, window, cx);
                }
                documents::Place::NewTab
            }
        };
        self.place_leaf(project_id, leaf, place, window, cx);
        // A new tab's leaf exists once the daemon answered `tab.create`: the next draw that
        // shows the pane focuses its field (`prepare_browsers`).
        self.browser_to_focus = Some(id);
        cx.notify();
    }

    /// `SplitBrowser`: a browser pane right of the focused pane.
    pub(crate) fn split_browser(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let at = (self.tab_scope(window).zip(self.focused_key(window)))
            .map(|((project, _), key)| (project, key, Edge::Right));
        if at.is_some() {
            self.new_browser(at, window, cx);
        }
    }

    /// A web link clicked in `window` (spec 4.1): `http(s)` loads in the shown tab's browser
    /// pane, or a new one right of `from` (else the focused pane), or a new tab; anything else,
    /// and every link where pages cannot show, goes to the system.
    pub(crate) fn open_web(
        &mut self,
        url: String,
        from: Option<PaneKey>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !is_web(&url) {
            return cx.open_url(&url);
        }
        if let Err(reason) = mkrew_ui::browser::available() {
            if !self.browser_fallback_told {
                self.browser_fallback_told = true;
                self.toast_info(format!("Web pages open in your browser: {reason}"), cx);
            }
            return cx.open_url(&url);
        }
        let scope = self.tab_scope(window);
        let Some(project_id) = scope.map(|(project, _)| project).or(self.workspace) else {
            return cx.open_url(&url);
        };
        let tab = scope.and_then(|(project, tab_id)| self.tabs.get(&project)?.tab(tab_id));
        match web_place(tab, from.as_ref()) {
            WebPlace::Load(id) => {
                self.focus_pane_key(project_id, &PaneKey::Web(id));
                self.navigate(id, url, window, cx);
                self.sync_panes(window, cx);
            }
            WebPlace::Split(target) => {
                let place = documents::Place::Split(target, Edge::Right);
                self.place_leaf(project_id, Leaf::web(url), place, window, cx);
            }
            WebPlace::NewTab => {
                // Terminals show the panes: Board and Docs give way (as a file from Docs).
                self.views.insert(project_id, View::Terminals);
                if self.workspace != Some(project_id) {
                    self.activate_workspace(project_id, window, cx);
                }
                self.place_leaf(
                    project_id,
                    Leaf::web(url),
                    documents::Place::NewTab,
                    window,
                    cx,
                );
            }
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
        if let Some(id) = self.browser_to_focus
            && shown.contains(&id)
            && self.browsers.contains_key(&id)
        {
            self.browser_to_focus = None;
            self.focus_address(id, window, cx);
        }
        let focused = self.focused_key(window);
        let dark = theme::slot_is_dark(cx.theme().mode, theme::os_dark(window));
        let usable = mkrew_ui::browser::available().is_ok();
        let overlay = self.browser_overlay(cx);
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
                && !browser.building
                && !browser.page.url.is_empty()
            {
                match Parent::of(window) {
                    Ok(parent) => {
                        browser.building = true;
                        let keys = mkrew_ui::browser::keys::forwarded(&self.keymap);
                        let data_dir = self.paths.data_dir.join("WebView2");
                        let (id, url, events) =
                            (*id, browser.page.url.clone(), self.page_events.clone());
                        let window_id = window.window_handle().window_id();
                        // Built from a task, with nothing borrowed: the build runs the message
                        // loop, and a window update in progress would make every task that
                        // updates this window fail meanwhile, the daemon connection's included
                        // (see `Parent`).
                        cx.spawn(async move |this, cx| {
                            let open = cx.update(|cx| {
                                cx.windows().iter().any(|w| w.window_id() == window_id)
                            });
                            // A window closed meanwhile builds nothing and records no error:
                            // its moved page is built again in the window that shows it.
                            let built = open.then(|| {
                                NativeView::build(
                                    &parent,
                                    id,
                                    &url,
                                    &keys,
                                    dark,
                                    Some(&data_dir),
                                    events,
                                )
                            });
                            let _ = this.update(cx, |shell, cx| {
                                shell.page_built(id, host, &url, built, keys, cx);
                            });
                        })
                        .detach();
                    }
                    Err(error) => {
                        tracing::warn!(%id, "browser: no window handle for the page: {error}");
                        browser.error = Some(error);
                    }
                }
            }
            // Focus moved to another pane: the address field is left, its text restored.
            if browser.host == Some(host) && browser.editing && focused != Some(PaneKey::Web(*id)) {
                browser.editing = false;
                let url = browser.page.url.clone();
                browser.address.clear();
                browser.address.insert(&url);
            }
            let Some(native) = &browser.native else {
                continue;
            };
            if browser.host == Some(host) && !here {
                native.set_shown(false);
            }
            if here {
                native.set_dark(dark);
                // The keyboard left the page's pane, or an overlay covers the page: give it
                // back to the window (hiding a page never does, so a background window stays
                // inactive).
                if browser.page_keys && (focused != Some(PaneKey::Web(*id)) || overlay) {
                    native.focus_parent();
                    browser.page_keys = false;
                }
            }
        }
    }

    /// The page of browser pane `id`, built for `host` on `url` (`prepare_browsers`), arrives
    /// (`None`: its window closed first). A pane closed or moved to another window meanwhile
    /// drops it (the next render builds the moved one in its new window).
    fn page_built(
        &mut self,
        id: Ulid,
        host: HostId,
        url: &str,
        built: Option<Result<NativeView, String>>,
        keys: Vec<String>,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(browser) = self.browsers.get_mut(&id) else {
            return;
        };
        browser.building = false;
        if browser.host == Some(host)
            && let Some(built) = built
        {
            match built {
                Ok(native) => {
                    // The address changed while the page was built.
                    if browser.page.url != url {
                        native.load(&browser.page.url);
                    }
                    browser.native = Some(native);
                    browser.page_keys = false;
                    browser.keys = keys;
                }
                Err(error) => {
                    tracing::warn!(%id, "browser: cannot build the page: {error}");
                    browser.error = Some(error);
                }
            }
        }
        cx.notify();
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
        window: &Window,
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
            .child(self.web_toolbar(id, browser, window, theme, cx))
            .child(div().flex_1().min_h_0().relative().child(body))
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

    /// The focused pane of `window` if it is a browser pane.
    pub(crate) fn focused_web(&self, window: &Window) -> Option<Ulid> {
        match self.focused_key(window)? {
            PaneKey::Web(id) => Some(id),
            _ => None,
        }
    }

    /// Gives pane `id`'s address field the keyboard, its text selected.
    pub(crate) fn focus_address(
        &mut self,
        id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(project) = self.web_project(id) {
            self.focus_pane_key(project, &PaneKey::Web(id));
        }
        let Some(browser) = self.browsers.get_mut(&id) else {
            return;
        };
        if let Some(native) = &browser.native {
            native.focus_parent();
        }
        browser.page_keys = false;
        browser.editing = true;
        browser.address.select_all();
        window.focus(&browser.address_focus, cx);
        cx.notify();
    }

    /// The browser pane commands, on the focused browser pane of `window`.
    pub(crate) fn browser_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(id) = self.focused_web(window) else {
            return;
        };
        let script = match command {
            Command::BrowserAddress => return self.focus_address(id, window, cx),
            Command::BrowserOpenExternal => return self.open_external(id, cx),
            Command::BrowserBack => "history.back()",
            Command::BrowserForward => "history.forward()",
            Command::BrowserReload => "location.reload()",
            _ => return,
        };
        if let Some(native) = self.browsers.get(&id).and_then(|b| b.native.as_ref()) {
            native.script(script);
        }
    }

    /// Back, forward, reload / stop, the address field, open in the system browser, close
    /// (spec 5.2), and the loading line under it.
    fn web_toolbar(
        &self,
        id: Ulid,
        browser: &Browser,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let has_page = browser.native.is_some();
        let nav = |name: &'static str, glyph: Icon, tip: &'static str, command: Command| {
            let button = if has_page {
                icon_button(name, glyph, theme).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        shell.browser_command(command, window, cx);
                    },
                ))
            } else {
                disabled_icon_button(name, glyph, theme.text_muted, theme)
            };
            button.tooltip(tooltip(theme, tip, self.keymap.label(command)))
        };
        let reload = if browser.page.loading {
            // Spec 5.2: while loading the button stops the load.
            let stop = if has_page {
                icon_button("web-stop", Icon::X, theme).on_click(cx.listener(
                    move |shell, _, _, _| {
                        if let Some(native) =
                            shell.browsers.get(&id).and_then(|b| b.native.as_ref())
                        {
                            native.script("window.stop()");
                        }
                    },
                ))
            } else {
                disabled_icon_button("web-stop", Icon::X, theme.text_muted, theme)
            };
            stop.tooltip(tooltip(theme, "Stop", ""))
        } else {
            nav(
                "web-reload",
                Icon::RotateCw,
                "Reload",
                Command::BrowserReload,
            )
        };
        let focused = browser.address_focus.is_focused(window);
        let field = caret_field(
            "web-address",
            &browser.address,
            "Type an address or paste a link",
            focused,
            theme,
            cx,
            move |shell, caret| {
                if let Some(browser) = shell.browsers.get_mut(&id) {
                    browser.address.click(caret.at, caret.extend, caret.clicks);
                }
            },
        )
        .flex_1()
        .min_w_0()
        .h(theme::rems_px(theme::INPUT_HEIGHT))
        // The fixed height leaves no room for the frame's padding: center the line instead.
        .flex()
        .items_center()
        .py_0()
        .pb(px(theme::TEXT_LIFT * 2.0))
        .overflow_hidden()
        .whitespace_nowrap()
        .track_focus(&browser.address_focus)
        .on_key_down(cx.listener(Self::web_address_key_down))
        .relative()
        .child(self.typing(&browser.address_focus, Self::web_address_key_down, cx))
        .on_click(cx.listener(move |shell, _, window, cx| {
            // Entry is decided by `editing`: the field's mouse-down already focused it.
            if !shell.browsers.get(&id).is_some_and(|b| b.editing) {
                shell.focus_address(id, window, cx);
            }
        }));
        // The pane frame's mouse down focuses the pane, which would take the keyboard back.
        // Stopped here, on a parent: the field's own mouse-down (caret, selection) runs first.
        let field = div()
            .flex_1()
            .min_w_0()
            .flex()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(field);
        let row = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::S))
            .child(nav(
                "web-back",
                Icon::ArrowLeft,
                "Back",
                Command::BrowserBack,
            ))
            .child(nav(
                "web-forward",
                Icon::ArrowRight,
                "Forward",
                Command::BrowserForward,
            ))
            .child(reload)
            .child(field)
            .child(
                icon_button("web-external", Icon::ExternalLink, theme)
                    .tooltip(tooltip(
                        theme,
                        "Open in system browser",
                        self.keymap.label(Command::BrowserOpenExternal),
                    ))
                    .on_click(cx.listener(move |shell, _, _, cx| shell.open_external(id, cx))),
            )
            .child(
                icon_button("web-close", Icon::X, theme)
                    .tooltip(tooltip(
                        theme,
                        "Close",
                        self.keymap.label(Command::ClosePane),
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.close_web_pane(id, window, cx);
                    })),
            );
        let line = div()
            .flex_none()
            .h(px(theme::PANE_ACCENT_WIDTH))
            .when(browser.page.loading, |el| el.bg(rgb(theme.accent)));
        div().flex_none().flex().flex_col().child(row).child(line)
    }

    /// Leaves the address field for the page: GPUI focus goes to the pane first, then the
    /// page takes the keyboard (a pane without a page keeps it on the pane).
    fn address_to_page(&mut self, id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(browser) = self.browsers.get_mut(&id) {
            window.focus(&browser.focus, cx);
            if let Some(native) = &browser.native {
                native.focus();
                browser.page_keys = true;
            }
        }
        cx.notify();
    }

    /// Typing in an address field (`typing` takes a plain fn): the pane whose field has the
    /// keyboard.
    fn web_address_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let id = (self.browsers.iter())
            .find(|(_, browser)| browser.address_focus.is_focused(window))
            .map(|(id, _)| *id);
        if let Some(id) = id {
            self.address_key_down(id, event, window, cx);
        }
    }

    /// Typing in the address field: edits, Enter loads, Esc restores and gives the page the keys.
    fn address_key_down(
        &mut self,
        id: Ulid,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let keystroke = &event.keystroke;
        if let Some(edit) = field_edit(keystroke, cx) {
            if let Some(browser) = self.browsers.get_mut(&id) {
                browser.editing = true;
                edit.apply(&mut browser.address, cx);
            }
            cx.notify();
            return;
        }
        if keystroke.modifiers.modified() {
            return;
        }
        match keystroke.key.as_str() {
            "enter" => {
                let typed = (self.browsers.get(&id))
                    .map(|b| b.address.as_str().to_owned())
                    .unwrap_or_default();
                if let Some(url) = mkrew_ui::browser::address::normalize(&typed) {
                    // `navigate` ends the editing and shows the new URL.
                    self.navigate(id, url, window, cx);
                    self.address_to_page(id, window, cx);
                }
            }
            "escape" => {
                if let Some(browser) = self.browsers.get_mut(&id) {
                    browser.editing = false;
                    let url = browser.page.url.clone();
                    browser.address.clear();
                    browser.address.insert(&url);
                }
                self.address_to_page(id, window, cx);
            }
            _ => {}
        }
    }
}

/// The scheme of `url` (empty if it has none).
fn scheme_of(url: &str) -> &str {
    url.split_once(':').map_or("", |(scheme, _)| scheme)
}

/// `http` and `https` links load in a browser pane.
fn is_web(url: &str) -> bool {
    let scheme = scheme_of(url);
    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
}

/// Whether `host`'s window is the OS's active one: real clicks and keys reach a page only then.
fn host_active(
    host: HostId,
    main: gpui::AnyWindowHandle,
    handles: &HashMap<WindowKey, gpui::AnyWindowHandle>,
    cx: &App,
) -> bool {
    let handle = match host {
        HostId::Main => Some(main),
        HostId::Detached(key) => handles.get(&key).copied(),
    };
    handle.is_some() && cx.active_window() == handle
}
