//! The Board's Jira side (SPEC §20I, ADR 0066): the `Tasks | Jira` switch, the workspace's
//! Jira keys field, the issue list (`List` tree or `Kanban` columns), and opening an issue as
//! a task draft.

use mkrew_core::settings::JiraLayout;
use mkrew_ui::jira::Issue;
use mkrew_ui::jira_board::{
    Section, TreeRow, columns, is_in_progress, linked_task, sections, select_step, split_area, tree,
};

use super::widgets::status_pill;

use crate::*;

impl Shell {
    /// The `[jira]` section, read per call so a hand edit applies at once (like voice).
    fn jira_settings(&self) -> Result<mkrew_core::settings::JiraSettings, String> {
        mkrew_core::settings::load(&self.settings_file)
            .map(|settings| settings.jira)
            .map_err(|error| error.to_string())
    }

    /// The settings and the API token, or why a call cannot be made.
    fn jira_setup(&self) -> Result<(mkrew_core::settings::JiraSettings, String), String> {
        let settings = self.jira_settings()?;
        mkrew_ui::jira::api_token(&settings, |name| std::env::var(name).ok())
            .map(|token| (settings, token))
            .map_err(|error| error.to_string())
    }

    /// The workspace's saved Jira keys (empty until it is linked).
    fn jira_keys_of(&self, project_id: Ulid) -> &[String] {
        self.sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or(&[][..], |p| p.jira_keys.as_slice())
    }

    /// The Jira list replaces the cards of the shown workspace's Board.
    pub(crate) fn jira_side_shown(&self) -> bool {
        self.jira_on
            && self
                .workspace
                .and_then(|project_id| self.jira.get(&project_id))
                .is_some_and(|view| view.source == Source::Jira)
    }

    pub(crate) fn set_board_source(
        &mut self,
        project_id: Ulid,
        source: Source,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let view = self.jira.entry(project_id).or_default();
        view.source = source;
        if source == Source::Tasks
            && (self.jira_filter_focus.is_focused(window)
                || self.jira_keys_focus.is_focused(window))
        {
            window.focus(&self.focus_handle, cx);
        }
        if source == Source::Jira && view.load == Load::NotLoaded {
            self.load_jira(project_id, window, cx);
        }
        cx.notify();
    }

    /// Searches the workspace's open issues; without keys the keys field shows instead.
    pub(crate) fn load_jira(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let keys = self.jira_keys_of(project_id).to_vec();
        if keys.is_empty() {
            let view = self.jira.entry(project_id).or_default();
            view.keys_field.get_or_insert_with(Input::default);
            window.focus(&self.jira_keys_focus, cx);
            cx.notify();
            return;
        }
        self.search_jira(project_id, keys, window, cx);
    }

    /// Sends the search for `keys` (all or the user's, per `[jira] mine`) on the background
    /// executor.
    fn search_jira(
        &mut self,
        project_id: Ulid,
        keys: Vec<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let setup = self.jira_setup();
        let mine = self.jira_mine;
        let view = self.jira.entry(project_id).or_default();
        let serial = view.start_load();
        let (settings, token) = match setup {
            Ok(setup) => setup,
            Err(reason) => {
                view.finish_load(serial, Err(reason));
                cx.notify();
                return;
            }
        };
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { mkrew_ui::jira::search(&settings, &token, &keys, mine) })
                .await
                .map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Some(view) = shell.jira.get_mut(&project_id)
                    && view.finish_load(serial, result)
                {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Sets the `All | Mine` filter; every workspace's list is dropped (searched again on Refresh
    /// or the next switch to Jira). False when nothing changed.
    pub(crate) fn apply_jira_mine(&mut self, mine: bool) -> bool {
        if self.jira_mine == mine {
            return false;
        }
        self.jira_mine = mine;
        self.jira.values_mut().for_each(JiraView::forget);
        true
    }

    /// The `All | Mine` click: saves `[jira] mine` and searches the workspace again. A settings
    /// file that does not parse keeps the choice for this run (toast, §20.0).
    pub(crate) fn set_jira_mine(
        &mut self,
        project_id: Ulid,
        mine: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !self.apply_jira_mine(mine) {
            return;
        }
        if let Err(error) = mkrew_core::settings::write_jira_mine(&self.settings_file, mine) {
            self.settings_write_failed("the Jira filter", error, cx);
        }
        self.load_jira(project_id, window, cx);
    }

    /// The `List | Kanban` click: saves `[jira] layout` (a settings file that does not parse
    /// keeps the choice for this run, toast §20.0). The loaded issues stay.
    pub(crate) fn set_jira_layout(&mut self, layout: JiraLayout, cx: &mut Context<'_, Self>) {
        self.jira_layout = layout;
        if let Err(error) = mkrew_core::settings::write_jira_layout(&self.settings_file, layout) {
            self.settings_write_failed("the Jira layout", error, cx);
        }
        cx.notify();
    }

    /// Show Board again while the Board is shown (and Jira is set up): Tasks <-> Jira, like the
    /// `Tasks | Jira` switch.
    pub(crate) fn toggle_board_source(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let source = (self.jira.get(&project_id)).map_or(Source::Tasks, |view| view.source);
        self.set_board_source(project_id, source.other(), window, cx);
    }

    /// A board key (`Command::Card`) while the Jira list shows: up / down walk the column, in
    /// the Kanban left / right walk the columns, in the List left / right fold / unfold the row,
    /// Enter opens the issue like a click. Shift+left / right and `e` do nothing here.
    // ponytail: Kanban cards are not scrolled into view (one scroll area per column); add a
    // per-column ScrollHandle if long columns need it. The List scrolls through `jira_scroll`.
    pub(crate) fn jira_key(
        &mut self,
        key: board::CardKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self
            .workspace
            .filter(|_| self.view() == View::Board && self.jira_side_shown())
        else {
            return;
        };
        let layout = self.jira_layout;
        let Some(view) = self.jira.get(&project_id) else {
            return;
        };
        let Load::Loaded(issues) = &view.load else {
            return;
        };
        let selected = view.selected.clone();
        let filter = view.filter.as_str();
        let cols = columns(issues, layout, filter, &view.collapsed);
        if key == board::CardKey::Open {
            let listed = |key: &String| cols.iter().flatten().any(|listed| listed == key);
            if let Some(issue_key) = selected.filter(listed) {
                self.open_issue(project_id, issue_key, window, cx);
            }
            return;
        }
        let is_list = matches!(layout, JiraLayout::List);
        if is_list && matches!(key, board::CardKey::Left | board::CardKey::Right) {
            let fold = tree(issues, filter, &view.collapsed)
                .into_iter()
                .find(|row| Some(row.key) == selected.as_deref())
                // Left folds an open parent, Right unfolds a folded one.
                .filter(|row| row.has_children && row.collapsed == (key == board::CardKey::Right))
                .map(|row| row.key.to_owned());
            if let (Some(fold), Some(view)) = (fold, self.jira.get_mut(&project_id)) {
                view.toggle_collapsed(&fold);
                cx.notify();
            }
            return;
        }
        let Some(next) = select_step(&cols, selected.as_deref(), key).map(str::to_owned) else {
            return;
        };
        let at = is_list
            .then(|| {
                tree(issues, filter, &view.collapsed)
                    .iter()
                    .position(|row| row.key == next)
            })
            .flatten();
        if let Some(view) = self.jira.get_mut(&project_id) {
            view.selected = Some(next);
        }
        if let Some(at) = at {
            self.jira_scroll.scroll_to_item(at);
        }
        cx.notify();
    }

    /// A click on an issue row: the linked task's edit dialog, else a new task from the ticket.
    pub(crate) fn open_issue(
        &mut self,
        project_id: Ulid,
        key: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.jira.entry(project_id).or_default().selected = Some(key.clone());
        if !self.board.is_loaded() {
            self.jira.entry(project_id).or_default().row_error =
                Some((key, "Tasks are still loading, try again".into()));
            cx.notify();
            return;
        }
        if let Some(task) =
            mkrew_ui::jira_board::linked_task(self.board.tasks(), project_id, &key).cloned()
        {
            self.jira.entry(project_id).or_default().source = Source::Tasks;
            self.open_task_dialog(TaskDialog::edit(task), window, cx);
            return;
        }
        let setup = self.jira_setup();
        let view = self.jira.entry(project_id).or_default();
        view.row_error = None;
        let (settings, token) = match setup {
            Ok(setup) => setup,
            Err(reason) => {
                view.row_error = Some((key, reason));
                cx.notify();
                return;
            }
        };
        view.opening = Some(key.clone());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let fetched_key = key.clone();
            let draft = cx
                .background_executor()
                .spawn(async move { mkrew_ui::jira::issue_draft(&settings, &token, &fetched_key) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                // A slow reply never replaces a dialog opened meanwhile, nor opens in another
                // workspace: the draft is dropped (the human clicks the row again).
                let free = !shell.modal_open() && shell.workspace == Some(project_id);
                let Some(view) = shell.jira.get_mut(&project_id) else {
                    return;
                };
                if view.opening.as_deref() != Some(key.as_str()) {
                    return; // another row was clicked since
                }
                view.opening = None;
                match draft {
                    Ok(draft) if free => shell.open_task_dialog(
                        TaskDialog::from_draft(project_id, draft),
                        window,
                        cx,
                    ),
                    Ok(_) => {}
                    Err(error) => view.row_error = Some((key, format!("{error:#}"))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Saves the keys field (`project.update`); errors show under the field.
    pub(crate) fn save_jira_keys(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.jira.entry(project_id).or_default();
        // Without the field `typed_keys` is empty and would clear the saved keys.
        if view.keys_field.is_none() || view.saving_keys {
            return;
        }
        let params = workspace_menu::set_jira_keys(project_id, view.typed_keys());
        view.saving_keys = true;
        view.keys_error = None;
        let saved = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let saved = saved.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(view) = shell.jira.get_mut(&project_id) else {
                    return;
                };
                view.saving_keys = false;
                match saved {
                    Ok(project) => {
                        view.load = Load::NotLoaded;
                        // The sidebar may not have `project.updated` yet: search the saved keys.
                        if !project.jira_keys.is_empty() {
                            view.keys_field = None;
                            shell.focus_pane(window, cx);
                            shell.search_jira(project_id, project.jira_keys, window, cx);
                        }
                    }
                    Err(error) => view.keys_error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The keys field: typing edits, Enter saves, Esc clears (then leaves the field when the
    /// workspace already has keys).
    pub(crate) fn jira_keys_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(project_id), Some(key)) = (self.workspace, quick_open::key(&event.keystroke))
        else {
            return;
        };
        let has_keys = !self.jira_keys_of(project_id).is_empty();
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let view = self.jira.entry(project_id).or_default();
        // The field only has the keyboard while shown: saved keys absent or Edit.
        let field = view.keys_field.get_or_insert_with(Input::default);
        match key {
            quick_open::Key::Close if !field.is_empty() => field.clear(),
            quick_open::Key::Close => {
                if has_keys {
                    view.keys_field = None;
                    self.focus_pane(window, cx);
                }
            }
            quick_open::Key::Open => return self.save_jira_keys(project_id, window, cx),
            quick_open::Key::Move(_) => {}
            quick_open::Key::Edit(key) => {
                copy_selection(&key, field, cx);
                field.edit(&key);
            }
            quick_open::Key::Paste => {
                field.insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                field.insert(&text);
            }
        }
        cx.notify();
    }

    /// The "Filter issues" field: typing edits, Esc clears (then leaves the field).
    pub(crate) fn jira_filter_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(project_id), Some(key)) = (self.workspace, quick_open::key(&event.keystroke))
        else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let field = &mut self.jira.entry(project_id).or_default().filter;
        match key {
            quick_open::Key::Close if !field.is_empty() => field.clear(),
            quick_open::Key::Close => self.focus_pane(window, cx),
            quick_open::Key::Open | quick_open::Key::Move(_) => {}
            quick_open::Key::Edit(key) => {
                copy_selection(&key, field, cx);
                field.edit(&key);
            }
            quick_open::Key::Paste => {
                field.insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                field.insert(&text);
            }
        }
        cx.notify();
    }

    /// Shows the keys field (Edit and the project button), filled with the saved keys.
    fn edit_jira_keys(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let keys = self.jira_keys_of(project_id).to_vec();
        self.jira.entry(project_id).or_default().edit_keys(&keys);
        window.focus(&self.jira_keys_focus, cx);
        cx.notify();
    }

    /// A two-option segmented control; the inactive segment runs `switch` with its option.
    /// `id` prefixes the segments' element ids (`(id, index)`).
    fn board_toggle<M: Copy + PartialEq + 'static>(
        id: &'static str,
        mode: M,
        options: [(M, &'static str); 2],
        tip: &'static str,
        switch: impl Fn(&mut Self, M, &mut Window, &mut Context<'_, Self>) + Clone + 'static,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        segmented(theme).children(options.into_iter().enumerate().map(
            |(index, (target, label))| {
                let switch = switch.clone();
                segment((id, index), target == mode, theme)
                    .tooltip(tooltip(theme, tip, SharedString::default()))
                    .when(target != mode, |el| {
                        el.on_click(cx.listener(move |shell, _, window, cx| {
                            switch(shell, target, window, cx);
                        }))
                    })
                    .child(label)
            },
        ))
    }

    /// The toolbar: the `Tasks | Jira` switch (only when `[jira] site` is set); in Jira mode
    /// with keys also the project button, Edit, the `All | Mine` filter, the `List | Kanban`
    /// layout, the "Filter issues" field and Refresh.
    pub(crate) fn render_board_header(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        if !self.jira_on {
            return None;
        }
        let view = self.jira.get(&project_id);
        let source = view.map_or(Source::Tasks, |v| v.source);
        let keys = self.jira_keys_of(project_id);
        let toggle = Self::board_toggle(
            "board-source",
            source,
            [(Source::Tasks, "Tasks"), (Source::Jira, "Jira")],
            "Tasks / Jira",
            move |shell, source, window, cx| shell.set_board_source(project_id, source, window, cx),
            theme,
            cx,
        );
        let header = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::S))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .child(toggle);
        if source != Source::Jira || keys.is_empty() {
            return Some(header);
        }
        let focused = self.jira_filter_focus.is_focused(window);
        let empty = Input::default();
        let filter = view.map_or(&empty, |v| &v.filter);
        let field = caret_field(
            "jira-filter",
            filter,
            "Filter issues",
            focused,
            theme,
            cx,
            move |shell, caret| {
                if let Some(view) = shell.jira.get_mut(&project_id) {
                    view.filter.click(caret.at, caret.extend, caret.clicks);
                }
            },
        )
        .track_focus(&self.jira_filter_focus)
        .on_key_down(cx.listener(Self::jira_filter_key_down))
        .relative()
        .child(self.typing(&self.jira_filter_focus, Self::jira_filter_key_down, cx))
        .on_click(cx.listener(|shell, _, window, cx| {
            window.focus(&shell.jira_filter_focus, cx);
            cx.notify();
        }));
        let project = div()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .child(format!("Project {}", keys.join(", ")))
            .child(icon_svg(
                Icon::ChevronDown,
                theme::ICON_SIZE_SMALL,
                theme.text_subtle,
            ));
        Some(
            header
                .child(
                    secondary_button("jira-project", project, theme).on_click(cx.listener(
                        move |shell, _, window, cx| shell.edit_jira_keys(project_id, window, cx),
                    )),
                )
                .child(
                    button("jira-edit", "Edit", theme, false).on_click(cx.listener(
                        move |shell, _, window, cx| shell.edit_jira_keys(project_id, window, cx),
                    )),
                )
                .child(Self::board_toggle(
                    "jira-assignee",
                    self.jira_mine,
                    [(false, "All"), (true, "Mine")],
                    "All issues / assigned to me",
                    move |shell, mine, window, cx| {
                        shell.set_jira_mine(project_id, mine, window, cx);
                    },
                    theme,
                    cx,
                ))
                .child(Self::board_toggle(
                    "jira-layout",
                    self.jira_layout,
                    [(JiraLayout::List, "List"), (JiraLayout::Kanban, "Kanban")],
                    "Under their parent / one column per status",
                    |shell, layout, _, cx| shell.set_jira_layout(layout, cx),
                    theme,
                    cx,
                ))
                .child(div().flex_1())
                .child(
                    search_box(field, None, focused, theme)
                        .flex_none()
                        .w(theme::rems_px(theme::BOARD_FILTER_WIDTH)),
                )
                .child(
                    button(
                        "jira-refresh",
                        icon_text(Icon::RotateCw, "Refresh", button_fg(theme, false)),
                        theme,
                        false,
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.load_jira(project_id, window, cx);
                    })),
                ),
        )
    }

    /// The Jira side of the Board: the keys field, the loading / failed / empty states, or the
    /// issue rows. Render never starts a load.
    pub(crate) fn render_jira(
        &self,
        project_id: Ulid,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let keys = self.jira_keys_of(project_id);
        let view = self.jira.get(&project_id);
        let field = view.and_then(|v| v.keys_field.as_ref());
        if field.is_some() || keys.is_empty() {
            let saving = view.is_some_and(|v| v.saving_keys);
            let error = view.and_then(|v| v.keys_error.clone());
            return div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .px(px(space::L))
                .pt(px(space::M))
                .child(
                    div()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text_muted))
                        .child("Link this workspace to Jira projects"),
                )
                .child(
                    caret_field(
                        "jira-keys",
                        field.unwrap_or(&Input::default()),
                        "PROJ, BILL",
                        self.jira_keys_focus.is_focused(window),
                        theme,
                        cx,
                        move |shell, caret| {
                            if let Some(field) = shell
                                .jira
                                .get_mut(&project_id)
                                .and_then(|view| view.keys_field.as_mut())
                            {
                                field.click(caret.at, caret.extend, caret.clicks);
                            }
                        },
                    )
                    .track_focus(&self.jira_keys_focus)
                    .on_key_down(cx.listener(Self::jira_keys_key_down))
                    .relative()
                    .child(self.typing(&self.jira_keys_focus, Self::jira_keys_key_down, cx))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        window.focus(&shell.jira_keys_focus, cx);
                        cx.notify();
                    })),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(
                            button("jira-save", "Save", theme, true).on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    shell.save_jira_keys(project_id, window, cx);
                                },
                            )),
                        )
                        .when(saving, |el| {
                            el.child(loading_dots(now, rgb(theme.text_muted)))
                        }),
                )
                .children(error.map(|error| notice(theme.text, format!("✕ {error}"))))
                .into_any_element();
        }
        match view.map_or(&Load::NotLoaded, |v| &v.load) {
            Load::Loading => message(theme, None, "Loading Jira issues…".into(), None)
                .flex_1()
                .child(loading_dots(now, rgb(theme.text_muted)))
                .into_any_element(),
            Load::Failed(reason) => message(
                theme,
                None,
                "Cannot load Jira issues.".into(),
                Some(reason.clone().into()),
            )
            .flex_1()
            .child(button("jira-retry", "Retry", theme, false).on_click(
                cx.listener(move |shell, _, window, cx| shell.load_jira(project_id, window, cx)),
            ))
            .into_any_element(),
            Load::Loaded(issues) if issues.is_empty() => message(
                theme,
                None,
                if self.jira_mine {
                    format!("No open issues assigned to you in {}", keys.join(", "))
                } else {
                    format!("No open issues in {}", keys.join(", "))
                }
                .into(),
                None,
            )
            .flex_1()
            .into_any_element(),
            Load::Loaded(issues) => {
                let opening = view.and_then(|v| v.opening.as_deref());
                let row_error = view.and_then(|v| v.row_error.as_ref());
                let filter = view.map_or("", |v| v.filter.as_str());
                let dots = |key: &str| {
                    (opening == Some(key)).then(|| loading_dots(now, rgb(theme.text_muted)))
                };
                let error_under = |key: &str| {
                    row_error
                        .filter(|(failed, _)| failed.as_str() == key)
                        .map(|(_, reason)| notice(theme.text, format!("✕ {reason}")))
                };
                let no_match = || {
                    message(
                        theme,
                        None,
                        format!("No issues match \"{}\"", filter.trim()).into(),
                        None,
                    )
                    .flex_1()
                    .into_any_element()
                };
                match self.jira_layout {
                    JiraLayout::List => {
                        let no_folds = HashSet::new();
                        let collapsed = view.map_or(&no_folds, |v| &v.collapsed);
                        let rows = tree(issues, filter, collapsed);
                        if rows.is_empty() {
                            return no_match();
                        }
                        let rows: Vec<gpui::AnyElement> = rows
                            .iter()
                            .enumerate()
                            .map(|(index, row)| {
                                div()
                                    .child(
                                        self.issue_row(project_id, index, row, theme, cx)
                                            .children(dots(row.key)),
                                    )
                                    .children(error_under(row.key).map(|e| e.px(px(space::S))))
                                    .into_any_element()
                            })
                            .collect();
                        div()
                            .id("jira-issues")
                            .track_scroll(&self.jira_scroll)
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(space::L))
                            .py(px(space::S))
                            .children(rows)
                            .into_any_element()
                    }
                    JiraLayout::Kanban => {
                        let groups = sections(issues, filter);
                        if groups.is_empty() {
                            return no_match();
                        }
                        let mut index = 0;
                        let mut columns: Vec<gpui::AnyElement> = Vec::new();
                        for (column, group) in groups.iter().enumerate() {
                            let mut cards: Vec<gpui::AnyElement> = Vec::new();
                            for issue in &group.issues {
                                cards.push(
                                    self.issue_card(project_id, index, issue, theme, cx)
                                        .children(dots(&issue.key))
                                        .children(error_under(&issue.key))
                                        .into_any_element(),
                                );
                                index += 1;
                            }
                            columns.push(
                                div()
                                    .flex_1()
                                    .min_w(px(theme::BOARD_COLUMN_MIN_WIDTH))
                                    .flex()
                                    .flex_col()
                                    .child(section_header(group, theme))
                                    .child(
                                        div()
                                            .id(("jira-column", column))
                                            .flex_1()
                                            .min_h_0()
                                            .flex()
                                            .flex_col()
                                            .gap(px(space::S))
                                            .p(px(space::S))
                                            .rounded(px(radius::MEDIUM))
                                            .bg(rgb(theme.surface))
                                            .overflow_y_scroll()
                                            .children(cards),
                                    )
                                    .into_any_element(),
                            );
                        }
                        div()
                            .id("jira-kanban")
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .gap(px(space::M))
                            .px(px(space::L))
                            .pb(px(space::L))
                            .overflow_x_scroll()
                            .children(columns)
                            .into_any_element()
                    }
                }
            }
            Load::NotLoaded => message(
                theme,
                None,
                "Press Refresh to load Jira issues.".into(),
                None,
            )
            .flex_1()
            .into_any_element(),
        }
    }

    /// One List row: fold chevron (or its space), type icon, key, area chip, title, type
    /// label, linked-task chip and the status pill, indented per parent above. A parent outside
    /// the results has no pill and a muted title. A click opens the issue (`open_issue`).
    fn issue_row(
        &self,
        project_id: Ulid,
        index: usize,
        row: &TreeRow<'_>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let key = row.key.to_owned();
        let (area, title) = split_area(row.summary);
        let fold = if row.has_children {
            let folded = key.clone();
            let hover = theme.hover;
            div()
                .id(("jira-fold", index))
                .flex_none()
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
                .tooltip(tooltip(
                    theme,
                    if row.collapsed { "Expand" } else { "Collapse" },
                    SharedString::default(),
                ))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    // Not a click on the row (which would open the issue).
                    cx.stop_propagation();
                    if let Some(view) = shell.jira.get_mut(&project_id) {
                        view.toggle_collapsed(&folded);
                    }
                    cx.notify();
                }))
                .child(icon_svg(
                    if row.collapsed {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronDown
                    },
                    theme::ICON_SIZE_SMALL,
                    theme.text_subtle,
                ))
                .into_any_element()
        } else {
            div()
                .flex_none()
                .w(theme::rems_px(theme::ICON_SIZE_SMALL))
                .into_any_element()
        };
        let row_hover = theme.row_hover;
        let selected = self
            .jira
            .get(&project_id)
            .and_then(|view| view.selected.as_deref())
            == Some(row.key);
        div()
            .id(("jira-issue", index))
            .flex()
            .items_center()
            .gap(px(space::S))
            .h(theme::rems_px(theme::BOARD_ROW_HEIGHT))
            .pl(px(space::S + row.depth as f32 * space::XL))
            .pr(px(space::S))
            .rounded(px(radius::MEDIUM))
            // Always bordered, so selecting does not shift the content.
            .border(px(theme::BORDER_WIDTH))
            .border_color(if selected {
                rgb(theme.pane_focus_border).into()
            } else {
                gpui::transparent_black()
            })
            .cursor_pointer()
            .text_color(rgb(theme.text))
            .hover(move |style| style.bg(rgb(row_hover)))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_issue(project_id, key.clone(), window, cx);
            }))
            .child(fold)
            .child(kind_icon(row.kind, theme))
            .child(key_label(row.key, theme).w(theme::rems_px(theme::BOARD_KEY_WIDTH)))
            .children(area.map(|area| area_chip(area, theme)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::BODY)
                    .text_color(rgb(if row.issue.is_some() {
                        theme.text
                    } else {
                        theme.text_muted
                    }))
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_subtle))
                    .child(row.kind.to_owned()),
            )
            .children(self.linked_chip(project_id, row.key, theme))
            .children(row.issue.map(|issue| issue_pill(&issue.status, theme)))
    }

    /// One Kanban card: title (two lines at most), then type icon, key, area chip and the
    /// linked-task chip, then the parent if any. A click opens the issue (`open_issue`).
    fn issue_card(
        &self,
        project_id: Ulid,
        index: usize,
        issue: &Issue,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let key = issue.key.clone();
        let (area, title) = split_area(&issue.summary);
        let hover = theme.border;
        let selected = self
            .jira
            .get(&project_id)
            .and_then(|view| view.selected.as_deref())
            == Some(issue.key.as_str());
        div()
            .id(("jira-card", index))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::M))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            // Always bordered, so hovering does not shift the content.
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(if selected {
                theme.pane_focus_border
            } else {
                theme.surface_raised
            }))
            .when(!selected, |el| {
                el.hover(move |style| style.border_color(rgb(hover)))
            })
            .cursor_pointer()
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_issue(project_id, key.clone(), window, cx);
            }))
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .line_clamp(2)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(kind_icon(&issue.kind, theme))
                    .child(key_label(&issue.key, theme))
                    .children(area.map(|area| area_chip(area, theme)))
                    .child(div().flex_1())
                    .children(self.linked_chip(project_id, &issue.key, theme)),
            )
            .children(issue.parent.as_ref().map(|parent| {
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_subtle))
                    .child(format!("{} {}", parent.key, parent.summary))
            }))
    }

    /// The linked task's column (`Todo`, `Running`…) as a chip, when a task is linked.
    fn linked_chip(&self, project_id: Ulid, key: &str, theme: &Theme) -> Option<gpui::Div> {
        linked_task(self.board.tasks(), project_id, key).map(|task| {
            div()
                .flex_none()
                .px(px(space::S))
                .rounded(px(radius::SMALL))
                .bg(rgb(theme.surface_raised))
                .text_size(font_size::META)
                .child(board::column_title(task.status))
        })
    }
}

/// The issue type's icon: layers in the accent for an epic or workstream, else a checkbox.
fn kind_icon(kind: &str, theme: &Theme) -> gpui::Svg {
    let (glyph, color) =
        if kind.eq_ignore_ascii_case("epic") || kind.eq_ignore_ascii_case("workstream") {
            (Icon::Layers, theme.accent)
        } else {
            (Icon::SquareCheck, theme.text_subtle)
        };
    icon_svg(glyph, theme::BOARD_TYPE_ICON, color).flex_none()
}

/// The issue key in the terminal font.
fn key_label(key: &str, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .font_family(theme.terminal_font_family.clone())
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .child(key.to_owned())
}

/// The `[BE]` / `[FE]` / other area tag of a title as a tinted chip.
fn area_chip(area: &str, theme: &Theme) -> gpui::Div {
    let color = if area.eq_ignore_ascii_case("be") {
        theme.info_soft
    } else if area.eq_ignore_ascii_case("fe") {
        theme.teal
    } else {
        theme.accent_text
    };
    tinted_chip(area.to_owned(), color, theme)
}

/// The status pill: filled `info` for In Progress, outlined muted for the rest.
fn issue_pill(status: &str, theme: &Theme) -> gpui::Div {
    if is_in_progress(status) {
        status_pill(
            status.to_owned(),
            theme.info,
            false,
            theme.info_text,
            theme.tint(theme.info),
        )
    } else {
        status_pill(
            status.to_owned(),
            theme.text_muted,
            true,
            theme.text_muted,
            rgb(theme.hover),
        )
    }
}

/// A status group's header: its status icon (half circle in `info` for In Progress, else an
/// empty circle), the name and the count.
fn section_header(section: &Section<'_>, theme: &Theme) -> gpui::Div {
    let (glyph, color) = if is_in_progress(section.status) {
        (Icon::CircleHalf, theme.info)
    } else {
        (Icon::Circle, theme.text_muted)
    };
    div()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .pt(px(space::M))
        .pb(px(space::XS))
        .text_size(font_size::BODY)
        .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, color))
        .child(
            div()
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .child(section.status.to_owned()),
        )
        .child(
            div()
                .text_color(rgb(theme.text_subtle))
                .child(section.issues.len().to_string()),
        )
}
