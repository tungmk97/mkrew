//! Focus, modals and attaching terminal panes.

use crate::*;

impl Shell {
    /// Whether a modal dialog is open (it keeps the keyboard).
    pub(crate) fn modal_open(&self) -> bool {
        self.task_dialog.is_some()
            || self.undo_dialog.is_some()
            || self.worktree_dialog.is_some()
            || self.compare.is_some()
            || self.turns_dialog.is_some()
            || self.git_discard.is_some()
            || self.git_base.is_some()
            || self.add_workspace.is_some()
            || self.plan_dialog.is_some()
            || self.phone_open
            || self.broadcast.is_some()
            || self.ask.is_some()
            || self.history.is_some()
            || self.doc_dialog.is_some()
            || self.diagram_dialog.is_some()
            || self.quick_open.is_some()
            || self.palette.is_some()
            || self.prompt_picker.is_some()
            || self.run_setup.is_some()
            || self.settings_dialog.is_some()
    }

    /// A dialog control (a button, a chip) has the keyboard, not the dialog's own text: the
    /// dialog's text fields must neither show a caret nor take keys.
    pub(crate) fn dialog_control_focused(&self, window: &Window) -> bool {
        !self.dialog_focus.is_focused(window)
    }

    /// Drops an auto-repeating Enter / Space while a control has the keyboard: gpui clicks a
    /// focused control on the release of an Enter / Space it saw pressed, so the Enter that
    /// opened a dialog, held a moment too long, must not click what has focus
    /// ([`dialog_focus::blocks_repeat`]). In the dialog's own text a held Enter is typing.
    pub(crate) fn dialog_guard(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !self.dialog_focus.is_focused(window)
            && dialog_focus::blocks_repeat(&event.keystroke, event.is_held)
        {
            cx.stop_propagation();
        }
    }

    /// The dialog-level keys of `event` ([`dialog_focus::nav`]): Tab / Shift+Tab move to the
    /// next / previous control and wrap inside the dialog, and a focused control takes Enter,
    /// Space and typing. `edge`: the dialog's own fields have none left in that direction. When
    /// the move lands back on the dialog's own text, `enter(self, forward)` puts its caret on the
    /// first (Tab) or last (Shift+Tab) field. `true` = handled: the dialog's own key handler
    /// must return.
    pub(crate) fn dialog_nav(
        &mut self,
        event: &KeyDownEvent,
        edge: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
        enter: impl FnOnce(&mut Self, bool),
    ) -> bool {
        let on_control = !self.dialog_focus.is_focused(window);
        match dialog_focus::nav(&event.keystroke, on_control, edge) {
            Nav::Pass => false,
            Nav::Absorb => {
                cx.stop_propagation();
                true
            }
            Nav::Move(forward) => {
                cx.stop_propagation();
                if forward {
                    window.focus_next(cx);
                } else {
                    window.focus_prev(cx);
                }
                if self.dialog_focus.is_focused(window) {
                    enter(self, forward);
                }
                cx.notify();
                true
            }
        }
    }

    /// Closes every transient popover menu without moving the keyboard; see
    /// [`Self::dismiss_menus`] for the version that gives it back. Called by every bound command
    /// through `dismiss_menus`.
    pub(crate) fn close_menus(&mut self) {
        self.tab_menu = None;
        self.layout_menu = None;
        self.pane_menu = None;
        self.share_menu = None;
        self.workspace_menu = None;
        self.file_menu = None;
        self.file_tab_menu = None;
        self.run_menu = None;
        self.workspace_switcher = false;
        self.new_menu = false;
        self.run_with = None;
        self.fan_out = None;
        self.card_menu = None;
        self.git_menu = None;
        self.menu_return = None;
        self.menu_selected = None;
        self.menu_swatch = 0;
        self.menu_pending = None;
    }

    /// The pane keyboard input goes to: the focused pane of the active tab (none while the
    /// board is shown).
    pub(crate) fn focused_session(&self) -> Option<Ulid> {
        if self.view() != View::Terminals {
            return None;
        }
        let tabs = self.tabs.get(&self.workspace?)?;
        tabs.active()?.focused()
    }

    /// The views the right panel is drawn next to.
    pub(crate) fn panel_view(&self) -> bool {
        matches!(self.view(), View::Terminals | View::Editor | View::Diff)
    }

    /// The right panel is drawn.
    pub(crate) fn panel_shown(&self) -> bool {
        self.right_panel.open && self.panel_view()
    }

    /// The right panel's tab on screen: a kept Session tab shows Files while
    /// `[features] session` is off.
    pub(crate) fn panel_tab(&self) -> PanelTab {
        match self.right_panel.tab {
            PanelTab::Session if !self.features.session => PanelTab::Files,
            tab => tab,
        }
    }

    /// The right panel shows the file tree.
    pub(crate) fn file_tree_shown(&self) -> bool {
        self.panel_shown() && self.panel_tab() == PanelTab::Files
    }

    /// Some of the active workspace's Editor view is drawn (the editor or the tree), so it
    /// follows `git.changed` and refreshes when it comes back.
    pub(crate) fn files_on_screen(&self) -> bool {
        self.view() == View::Editor || self.file_tree_shown()
    }

    /// The right panel shows the Git list (header, changes or History, commit box).
    pub(crate) fn git_panel_shown(&self) -> bool {
        self.panel_shown() && self.right_panel.tab == PanelTab::Git
    }

    /// Some of the active workspace's Git is drawn (the panel's list or the diff), so it
    /// follows `git.changed` and refreshes when it comes back (SPEC §12B).
    pub(crate) fn git_shown(&self) -> bool {
        self.view() == View::Diff || self.git_panel_shown()
    }

    /// The pane the Session tab describes: the active tab's focused pane, also while the
    /// Editor is shown (unlike `focused_session`, which is where keys go).
    pub(crate) fn panel_session(&self) -> Option<Ulid> {
        let tabs = self.tabs.get(&self.workspace?)?;
        tabs.active()?.focused()
    }

    /// Attaches the sessions on screen (live ones, or already attached; none while the window
    /// is minimized or the board is shown) and detaches all others (SPEC §10), then focuses
    /// the focused pane.
    pub(crate) fn sync_panes(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let visible = on_screen(
            window.is_visible(),
            self.workspace
                .filter(|_| self.view() == View::Terminals)
                .and_then(|id| self.tabs.get(&id)),
        );
        for session in self.router.wanted() {
            if visible.contains(&session) {
                continue;
            }
            if self.router.forget(session)
                && let Some(connection) = &self.connection
            {
                detach(connection, session, cx);
            }
            self.panes.remove(&session);
        }
        self.attach_errors.retain(|id, _| visible.contains(id));
        for &session in &visible {
            let skip = self.router.is_wanted(session)
                || self.attach_errors.contains_key(&session)
                || self.is_exited(session);
            if !skip {
                self.attach(session, window, cx);
            }
        }
        self.focus_pane(window, cx);
    }

    /// Platform text input (IME, Unikey) for the field focused by `focus`, fed to `on_key`.
    /// Put it inside the field; only the focused field's handler is registered.
    pub(crate) fn typing(
        &self,
        focus: &FocusHandle,
        on_key: ime::OnKey<Self>,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        typing_input(focus, on_key, &self.ime_marked, &self.typing_target, cx)
    }

    pub(crate) fn focus_pane(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        // An open confirmation keeps the keyboard; it hands focus back when answered.
        if window.has_active_prompt() {
            return;
        }
        // A modal dialog keeps the keyboard until it closes; the history's replay pane takes it
        // (copy, find, scrollback), its unbound keys bubble to the dialog.
        if self.history.is_some()
            && let Some(pane) = &self.history_pane
        {
            pane.update(cx, |pane, cx| pane.focus(window, cx));
            return;
        }
        if self.modal_open() {
            window.focus(&self.dialog_focus, cx);
            return;
        }
        // The panes' connection is dead: the keyboard stays with the window until it is back.
        if self.reconnecting() {
            window.focus(&self.focus_handle, cx);
            return;
        }
        match self
            .focused_session()
            .and_then(|session| self.panes.get(&session))
        {
            Some(pane) => pane.update(cx, |pane, cx| pane.focus(window, cx)),
            None => window.focus(&self.focus_handle, cx),
        }
    }

    /// `session.attach`; frames arriving before the reply are buffered by the router.
    pub(crate) fn attach(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(generation) = self.router.begin_attach(session) else {
            return;
        };
        let attach = connection.call::<SessionAttachResult>(Request::SessionAttach(SessionRef {
            session_id: session,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let attached = attach.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let attached = match attached {
                    Ok(attached) => attached,
                    Err(error) => {
                        if shell.router.fail_attach(session, generation) {
                            tracing::warn!(%session, "session.attach failed: {error:#}");
                            shell.attach_errors.insert(session, format!("{error:#}"));
                            cx.notify();
                        }
                        return;
                    }
                };
                let Some(buffered) = shell.router.finish_attach(session, generation) else {
                    // Hidden meanwhile (and not wanted again, or the router would have kept
                    // this attach): drop the daemon's attachment.
                    if !shell.router.is_wanted(session) {
                        detach(&connection, session, cx);
                    }
                    return;
                };
                let snapshot = buffered.snapshot.unwrap_or(attached.snapshot);
                let painted = shell.painted.clone();
                // A machine window opens URLs only: the paths a session prints are on the other
                // computer, not in this computer's folders.
                let machine = shell.target.is_machine();
                let base = shell
                    .link_scope(session)
                    .map(|(_, _, dir)| dir)
                    .filter(|_| !machine);
                let pane = cx.new(|cx| {
                    let mut pane =
                        TerminalPane::new(session, &snapshot, connection, painted, window, cx);
                    pane.set_links(base);
                    if machine {
                        pane.refuse_pasted_files();
                    }
                    pane.feed(&buffered.output);
                    if let Some(exit_code) = buffered.exited {
                        pane.set_exited(exit_code);
                    }
                    pane
                });
                cx.subscribe_in(
                    &pane,
                    window,
                    move |shell, _, link: &OpenLink, window, cx| {
                        shell.open_terminal_link(session, link, window, cx);
                    },
                )
                .detach();
                cx.subscribe(&pane, |shell, _, failed: &AttachFailed, cx| {
                    shell.toast_error(failed.0.clone(), cx);
                })
                .detach();
                cx.subscribe(&pane, |shell, _, _: &LocalFiles, cx| {
                    if let Some(name) = shell.machine_name().map(str::to_owned) {
                        shell.toast_info(mkrew_ui::machines::local_files_refused(&name), cx);
                    }
                })
                .detach();
                cx.subscribe(&pane, |_, _, _: &CopyModeChanged, cx| cx.notify())
                    .detach();
                cx.subscribe(&pane, |shell, _, _: &NoLinks, cx| {
                    shell.show_tab_notice("No links on screen.", cx);
                })
                .detach();
                shell.panes.insert(session, pane);
                if shell.focused_session() == Some(session) {
                    shell.focus_pane(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The shade behind a modal dialog: it blocks the window behind it (a stray click there
    /// never closes the dialog; Esc or Cancel does) and keeps the keyboard in the dialog.
    pub(crate) fn modal_shade(
        id: &'static str,
        panel: impl IntoElement,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        div()
            .id(id)
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(cx.theme().scrim))
            .occlude()
            // The keyboard stays in the dialog.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.focus_pane(window, cx);
                }),
            )
            .child(panel)
            .into_any_element()
    }
}
