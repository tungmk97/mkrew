//! A Space window's content (ADR 0113): the title bar, the sidebar, the main area and what
//! opens over them, for the workspace that window shows. `SpaceWindow` draws it.

use crate::*;

impl Shell {
    pub(crate) fn render_space(
        &mut self,
        key: SpaceKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        self.ensure_diff_views();
        let host = HostId::Space(key);
        self.prepare_browsers(host, window, cx);
        let theme = cx.theme().clone();
        // A menu closed behind the keyboard's back (workspace switched, dialog opened, daemon
        // event) must not leave it on the hidden menu: give it back to the pane. A menu whose
        // owner went away (no rows) draws nothing: close it the same way.
        if self.menu_open() && self.open_menu_rows(window).is_none() {
            self.dismiss_menus(window, cx);
        }
        if !self.menu_open() && self.menu_focus.contains_focused(window, cx) {
            self.focus_pane(window, cx);
        }
        // Voice input types into the field whose typing canvas registers again during this
        // frame's paint (§20G); a field that is gone from the screen is no target. That field
        // paints the listening ring.
        self.typing_target.field.replace(None);
        // Menus opened from the keyboard hang from what this frame lays out.
        cx.global::<MenuAnchors>()
            .clear(window.window_handle().window_id());
        self.typing_target.ring.set(self.voice_ring(&theme, cx));
        window.set_rem_size(px(cx.global::<theme::Fonts>().ui_size));
        self.sync_traffic_lights(host, window, cx.global::<theme::Fonts>().ui_size);
        // A dialog control that vanished while focused (a button swapped for its disabled twin
        // while saving, the next step of a dialog) would leave the keyboard on nothing and Esc
        // would miss the dialog: the dialog itself takes the keyboard back.
        if self.modal_open()
            && self.popup_host().is_none_or(|popup| popup == host)
            && !window.has_active_prompt()
            && !self.dialog_focus.contains_focused(window, cx)
        {
            window.focus(&self.dialog_focus, cx);
        }
        if let Some(launched) = self.launched {
            if !self.first_frame_logged {
                self.first_frame_logged = true;
                tracing::info!(ms = launched.elapsed().as_millis(), "first frame");
            }
            if matches!(self.status, Status::Connected) {
                self.launched = None;
                tracing::info!(ms = launched.elapsed().as_millis(), "first connected frame");
            }
        }
        // One clock reading per frame, so every spinner and time agrees.
        let now = status::now_ms();
        // Rendering is what marks the focused pane of the active window viewed (its done leaves
        // the Inbox): every state change and window activation re-renders. A change in what
        // needs the user also reaches the badge and the tray.
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
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .track_focus(&self.focus_handle)
            .when(self.view(self.space_host(window)) == View::Board, |el| {
                el.key_context(BOARD_CONTEXT)
            })
            .when(self.view(self.space_host(window)) == View::Docs, |el| {
                el.key_context(DOCS_CONTEXT)
            })
            .on_action(cx.listener(Self::run))
            .on_action(cx.listener(Self::expand_diagram))
            .on_action(cx.listener(Self::open_preview_link))
            .on_action(cx.listener(Self::toggle_preview_task))
            .on_action(cx.listener(Self::copy_preview_code))
            .on_action(cx.listener(Self::open_chat_path))
            // `/` focuses the sidebar filter while nothing else has the keyboard (a terminal or
            // a field keeps its `/`).
            .on_key_down(cx.listener(|shell, event: &KeyDownEvent, window, cx| {
                if shell.reconnecting() && !shell.modal_open() {
                    return;
                }
                shell.docs_key_down(event, window, cx);
                let filter_shown = !shell.sidebar_layout.hidden()
                    && !shell.sidebar_layout.compact()
                    && !shell.sidebar.is_empty();
                if event.keystroke.key == "/"
                    && !event.keystroke.modifiers.modified()
                    && filter_shown
                    && !shell.modal_open()
                    && shell.focus_handle.is_focused(window)
                {
                    cx.stop_propagation();
                    shell.sidebar.end_browse();
                    window.focus(&shell.filter_focus, cx);
                    cx.notify();
                }
            }))
            .child(tear_off_catcher(cx))
            .child(self.render_title_bar(host, now, &theme, window, cx))
            .children(self.render_reconnect_banner(&theme, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    // Hidden: not built at all (nothing to lay out or paint). Its own cached
                    // view: a pane printing does not lay it out again (`SidebarView`).
                    .children((!self.sidebar_layout.hidden()).then(|| {
                        let style = gpui::StyleRefinement::default().flex_none().h_full();
                        let style = if self.sidebar_layout.compact() {
                            style.w(theme::rems_px(theme::COMPACT_SIDEBAR_WIDTH))
                        } else {
                            style.w(px(self.sidebar_width))
                        };
                        let shell = cx.entity().downgrade();
                        (self.hosts.entry(host).or_default().sidebar_view)
                            .get_or_insert_with(|| {
                                cx.new(|cx| shell::sidebar_view::SidebarView::new(shell, cx))
                            })
                            .clone()
                            .cached(style)
                    }))
                    .child(self.render_main(now, &theme, window, cx))
                    // Over the sidebar and the main area, under the dialogs: no mouse input
                    // reaches them while the connection is down.
                    .children(self.reconnecting().then(inert_layer))
                    .children(
                        self.task_dialog
                            .as_ref()
                            .map(|dialog| self.render_task_dialog(dialog, window, &theme, cx)),
                    )
                    .children(
                        self.undo_dialog
                            .as_ref()
                            .map(|dialog| self.render_undo_dialog(dialog, &theme, cx)),
                    )
                    .children(
                        self.worktree_dialog
                            .as_ref()
                            .map(|dialog| self.render_worktree_dialog(dialog, &theme, cx)),
                    )
                    .children(
                        self.compare
                            .as_ref()
                            .map(|compare| self.render_compare(compare, now, &theme, cx)),
                    )
                    .children(
                        self.turns_dialog
                            .as_ref()
                            .map(|dialog| self.render_turns_dialog(dialog, now, &theme, cx)),
                    )
                    .children(
                        self.git_discard
                            .as_ref()
                            .map(|dialog| self.render_git_discard(dialog, &theme, cx)),
                    )
                    .children(
                        self.git_base
                            .as_ref()
                            .map(|dialog| self.render_git_base(dialog, window, &theme, cx)),
                    )
                    .children(
                        self.add_workspace
                            .as_ref()
                            .map(|dialog| self.render_add_workspace(dialog, window, &theme, cx)),
                    )
                    .children(
                        self.plan_dialog
                            .as_ref()
                            .map(|dialog| self.render_plan_path(dialog, window, &theme, cx)),
                    )
                    .children(
                        self.phone_open
                            .then(|| self.render_phone_access(&theme, cx)),
                    )
                    .children(
                        self.ports_dialog
                            .is_some()
                            .then(|| self.render_ports(&theme, cx)),
                    )
                    .children(
                        self.settings_dialog
                            .as_ref()
                            .map(|dialog| self.render_settings_dialog(dialog, window, &theme, cx)),
                    )
                    .children(
                        self.run_setup
                            .as_ref()
                            .map(|setup| self.render_run_setup(setup, window, &theme, cx)),
                    )
                    .children(
                        self.broadcast
                            .as_ref()
                            .map(|dialog| self.render_broadcast(dialog, window, &theme, cx)),
                    )
                    .children(
                        self.ask
                            .as_ref()
                            .map(|dialog| self.render_ask(dialog, window, &theme, cx)),
                    )
                    .children(
                        self.history
                            .as_ref()
                            .map(|dialog| self.render_history(dialog, now, &theme, cx)),
                    )
                    .children(self.doc_dialog.as_ref().map(|dialog| match dialog {
                        DocDialog::Send(dialog) => self.render_send_dialog(dialog, now, &theme, cx),
                        DocDialog::Changes(changes) => self.render_changes(changes, &theme, cx),
                    }))
                    .children(
                        self.diagram_dialog
                            .as_ref()
                            .map(|diagram| self.render_diagram_dialog(diagram, &theme, cx)),
                    )
                    .children(
                        self.palette
                            .as_ref()
                            .filter(|_| self.palette_host == host)
                            .map(|palette| self.render_palette(palette, &theme, cx)),
                    )
                    .children(
                        self.prompt_picker
                            .as_ref()
                            .filter(|_| self.picker_host == host)
                            .map(|picker| self.render_prompt_picker(picker, &theme, cx)),
                    )
                    .children(
                        self.slash_picker
                            .as_ref()
                            .filter(|_| self.picker_host == host)
                            .map(|picker| self.render_slash_picker(picker, &theme, cx)),
                    )
                    .children(
                        self.workspace_menu
                            .as_ref()
                            .filter(|_| !self.modal_open())
                            .map(|menu| self.render_workspace_menu(menu, &theme, cx)),
                    )
                    .children(
                        self.tab_menu
                            .as_ref()
                            .filter(|_| {
                                self.view(self.space_host(window)) == View::Terminals
                                    && !self.modal_open()
                                    && self.menu_host == host
                            })
                            .map(|menu| self.render_tab_menu(menu, window, &theme, cx)),
                    )
                    .children(
                        self.layout_menu
                            .as_ref()
                            .filter(|_| {
                                self.view(self.space_host(window)) == View::Terminals
                                    && !self.modal_open()
                                    && self.menu_host == host
                            })
                            .map(|menu| self.render_layout_menu(menu, &theme, cx)),
                    )
                    .children(
                        self.pane_menu
                            .as_ref()
                            .filter(|_| {
                                self.view(self.space_host(window)) == View::Terminals
                                    && !self.modal_open()
                                    && self.menu_host == host
                            })
                            .and_then(|menu| self.render_pane_menu(menu, &theme, cx)),
                    )
                    .children(
                        self.share_menu
                            .as_ref()
                            .filter(|_| {
                                self.view(self.space_host(window)) == View::Terminals
                                    && !self.modal_open()
                                    && self.menu_host == host
                            })
                            .map(|menu| self.render_share_menu(menu, &theme, cx)),
                    )
                    .children(
                        (self.file_menu.as_ref())
                            .filter(|menu| {
                                self.workspace_in(window) == Some(menu.project_id)
                                    && self.file_tree_shown(self.space_host(window))
                                    && !self.modal_open()
                            })
                            .map(|menu| self.render_file_menu(menu, &theme, cx)),
                    )
                    .children(
                        self.card_menu
                            .as_ref()
                            .filter(|_| {
                                self.view(self.space_host(window)) == View::Board
                                    && !self.modal_open()
                            })
                            .map(|menu| self.render_card_menu(menu, &theme, cx)),
                    )
                    .children(
                        self.render_toasts(&theme, cx)
                            .filter(|_| self.toast_host() == host),
                    )
                    .children(self.render_voice_pill(&theme, cx)),
            )
            .into_any_element()
    }
}
