//! The `+` row of the tab strip (SPEC §10, ADR 0108): `+` grows on hover or keyboard focus into
//! the default agent, the default terminal, a new note and `▾` (today's menu). Each item starts
//! something by click, or by drag onto a pane edge or the tab strip.

use crate::*;
use gpui::AnimationExt as _;

/// Payload of a drag from an item of the `+` row: what to start where it is dropped.
#[derive(Debug, Clone)]
pub(crate) struct DraggedNew {
    pub(crate) kind: NewKind,
    pub(crate) title: SharedString,
}

/// Where a pane being started lands once its session exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SpawnAt {
    /// Beside the focused pane of the tab in front, split along the axis (Split right / down).
    Beside(Axis),
    /// On an edge of a given pane, in the tab that holds it (a drop from the `+` row).
    At(PaneKey, Edge),
}

impl Shell {
    /// The `+` after the last tab. Hover or keyboard focus opens the row of items on the same
    /// line; the pointer may leave for `PLUS_ROW_COLLAPSE_DELAY` without closing it. While a
    /// pane starts it shows the loading dots instead (one start at a time). `host`: the main
    /// window (items open tabs, `▾` opens the menu) or a detached one (items split its tab).
    pub(crate) fn render_new_tab_button(
        &self,
        host: HostId,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        if let Some(kind) = self.starting_pane {
            let what = match kind {
                SessionKind::Agent => "Starting agent",
                SessionKind::Shell => "Starting shell",
            };
            return div()
                .id("new-tab")
                .flex_none()
                .min_w(theme::rems_px(theme::CONTROL_HEIGHT))
                .h(theme::rems_px(theme::CONTROL_HEIGHT))
                .px(px(space::S))
                .flex()
                .items_center()
                .justify_center()
                .tooltip(tooltip(theme, what, ""))
                .child(loading_dots(now, rgb(theme.text_muted)))
                .into_any_element();
        }
        let main = host == HostId::Main;
        let project = if main {
            self.workspace
        } else {
            self.tab_scope(window).map(|(project, _)| project)
        };
        let agents = self.pane_agents_of(project);
        let shell_name = project.and_then(|id| self.default_shell_name(id));
        let items = plus_row_items(default_agent(&agents), shell_name.as_deref(), main);
        // `+` is tab stop 0, the items follow: ← / → stop at the ends of the row.
        let stops = items.len() + 1;
        // The menu opens below the row, outside it: the pointer is then off the row, which
        // stays open while the menu does.
        let open =
            self.plus.shown() || self.new_menu || self.plus_focus.contains_focused(window, cx);
        let plus = div()
            .id("new-tab")
            .flex_none()
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .flex()
            .items_center()
            .rounded(px(radius::MEDIUM))
            .text_color(rgb(theme.text_muted))
            .pressable(Look::Icon, true, theme)
            .focus_stop(theme)
            .chip_arrows(0, stops)
            // The row names its items once open; the tooltip would sit on top of them.
            .when(!open, |el| {
                el.tooltip(tooltip(
                    theme,
                    "New tab",
                    format!(
                        "{} / {}",
                        self.keymap.label(Command::NewAgentTab),
                        self.keymap.label(Command::NewShellTab)
                    ),
                ))
            })
            // Enter on a focused `+` clicks it too (gpui). An open menu already closed on
            // this mouse down (outside it): the click reopens it. A detached window has none.
            .on_press(
                main,
                cx.listener(|shell, _, window, cx| shell.open_new_menu(window, cx)),
            )
            .child(icon_svg(Icon::Plus, theme::ICON_SIZE, theme.text_muted));
        let row = open.then(|| {
            div()
                .flex()
                .items_center()
                .children(items.iter().enumerate().map(|(index, item)| {
                    self.new_row_item(item, index + 1, stops, main, theme, cx)
                }))
                .with_animation(
                    "new-row-items",
                    gpui::Animation::new(theme::motion::FAST).with_easing(gpui::ease_out_quint()),
                    |el, delta| {
                        el.opacity(delta)
                            .relative()
                            .left(px(-theme::PANEL_RISE * (1.0 - delta)))
                    },
                )
        });
        let menu = (main && self.new_menu && !self.modal_open()).then(|| {
            render_menu(
                self.menu_state(),
                "new-tab-panel",
                self.new_menu_rows(),
                None,
                Place::Dropdown,
                theme,
                cx,
            )
        });
        // No gap between `+` and its items and no padding: the pointer never leaves the row
        // between them, and a click between two items cannot focus the wrapper.
        div()
            .id("new-row")
            .flex_none()
            .flex()
            .items_center()
            .track_focus(&self.plus_focus)
            .on_hover(cx.listener(|shell, hovered: &bool, window, cx| {
                shell.hover_new_row(*hovered, window, cx);
            }))
            .on_key_down(cx.listener(|shell, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                    cx.stop_propagation();
                    shell.plus.dismiss();
                    shell.focus_pane(window, cx);
                    cx.notify();
                }
            }))
            .child(plus)
            .children(row)
            .children(menu)
            .into_any_element()
    }

    /// One item after `+`: a `pressable(Look::Icon)` with its tooltip and shortcut, a tab stop
    /// with the focus ring (`index` in the row, `+` is 0), a click that starts it and, but for
    /// `▾`, a drag that starts it where it is dropped.
    fn new_row_item(
        &self,
        item: &RowItem,
        index: usize,
        stops: usize,
        main: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let label = item.label(main);
        let keys = item
            .command(main)
            .map(|command| self.keymap.label(command))
            .unwrap_or_default();
        let glyph = match item {
            RowItem::Agent { id, icon, .. } => agent_chip(
                sidebar::agent_badge(icon.as_deref(), id),
                theme.text_muted,
                theme,
            )
            .into_any_element(),
            RowItem::Shell { .. } => {
                icon_svg(Icon::Terminal, theme::ICON_SIZE, theme.text_muted).into_any_element()
            }
            RowItem::Note => {
                icon_svg(Icon::NotebookPen, theme::ICON_SIZE, theme.text_muted).into_any_element()
            }
            RowItem::More => icon_svg(Icon::ChevronDown, theme::ICON_SIZE_SMALL, theme.text_muted)
                .into_any_element(),
        };
        let base = div()
            .id(SharedString::from(format!("new-row-{index}")))
            .flex_none()
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .flex()
            .items_center()
            .rounded(px(radius::MEDIUM))
            .pressable(Look::Icon, true, theme)
            .focus_stop(theme)
            .chip_arrows(index, stops)
            .tooltip(tooltip(theme, &label, keys))
            .child(glyph);
        match item.kind() {
            Some(kind) => {
                let style = PopupStyle::of(theme);
                let accent = theme.accent;
                let drag = DraggedNew {
                    kind: kind.clone(),
                    title: label.into(),
                };
                base.on_press(
                    true,
                    cx.listener(move |shell, _, window, cx| {
                        shell.click_new(kind.clone(), window, cx);
                    }),
                )
                .on_drag(drag, move |item, offset, _, cx| {
                    let (style, title) = (style.clone(), item.title.clone());
                    cx.new(|_| DragChip {
                        style,
                        accent,
                        title,
                        offset,
                    })
                })
                .into_any_element()
            }
            None => base
                .on_press(
                    true,
                    cx.listener(|shell, _, window, cx| shell.open_new_menu(window, cx)),
                )
                .into_any_element(),
        }
    }

    /// The display name of the workspace's default shell profile, if it has one.
    fn default_shell_name(&self, project_id: Ulid) -> Option<String> {
        let project = (self.sidebar.projects().iter()).find(|p| p.id == project_id)?;
        let id = project.default_shell.as_deref()?;
        (self.agent_profiles.iter())
            .find(|profile| profile.id == id)
            .map(|profile| profile.name.clone())
    }

    /// The pointer entered or left the row: it opens at once; leaving closes it after
    /// `PLUS_ROW_COLLAPSE_DELAY` unless the pointer came back (serial and timer, as `peek`).
    pub(crate) fn hover_new_row(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if hovered {
            self.plus.enter();
            cx.notify();
            return;
        }
        let serial = self.plus.leave();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::PLUS_ROW_COLLAPSE_DELAY)
                .await;
            let _ = this.update_in(cx, |shell, _, cx| {
                if shell.plus.collapse(serial) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A click (or Enter) on an item. The main window opens a tab. A detached window has no
    /// new tab of its own (new tabs belong to the main window), so it splits its tab beside
    /// the focused pane, like its Split ▾.
    pub(crate) fn click_new(
        &mut self,
        kind: NewKind,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let at = self
            .detached_key(window)
            .and_then(|_| self.focused_key(window))
            .map(|key| (key, Edge::Right));
        self.start_new(kind, at, window, cx);
    }

    /// Starts `kind`: `at` `None` = in a new tab right after the active one; `Some` = in a leaf
    /// on that edge of that pane. Without a default of the kind, the usual `ask_for_default`
    /// toast and workspace menu.
    pub(crate) fn start_new(
        &mut self,
        kind: NewKind,
        at: Option<(PaneKey, Edge)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match (kind, at) {
            (NewKind::Agent(profile), None) => {
                self.open_new_tab(Some((SessionKind::Agent, profile)), window, cx);
            }
            (NewKind::Shell, None) => {
                self.open_new_tab(Some((SessionKind::Shell, None)), window, cx);
            }
            (NewKind::Agent(profile), Some((key, edge))) => {
                self.spawn_pane(
                    SessionKind::Agent,
                    profile,
                    SpawnAt::At(key, edge),
                    window,
                    cx,
                );
            }
            (NewKind::Shell, Some((key, edge))) => {
                self.spawn_pane(SessionKind::Shell, None, SpawnAt::At(key, edge), window, cx);
            }
            (NewKind::Note, at) => self.new_note(at, window, cx),
        }
    }

    /// An item dropped on the tab strip (the main window's; `strip_drops`): a new tab right
    /// after the active one.
    pub(crate) fn drop_new_on_strip(
        &mut self,
        item: &DraggedNew,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() {
            return;
        }
        self.start_new(item.kind.clone(), None, window, cx);
    }

    /// New note (SPEC §20.0 `[notes] folder`): `fs.new_note` makes the file, then it opens
    /// kept, in source mode, with the cursor in the editor; in a new tab after the active one
    /// (`at` `None`) or in a leaf on an edge of a pane. A failure shows `toast_error` with the
    /// reason and nothing opens.
    pub(crate) fn new_note(
        &mut self,
        at: Option<(PaneKey, Edge)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        // The workspace of the pane dropped on, else of this window's tab, else the shown one.
        let holder = (at.as_ref())
            .and_then(|(key, _)| self.project_holding(key))
            .map(|(project, _)| project);
        let Some(project_id) = holder
            .or_else(|| self.tab_scope(window).map(|(project, _)| project))
            .or(self.workspace)
        else {
            return;
        };
        // The Files view's worktree target, as `create_entry`: the note opens from there.
        let target = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        let made = connection.call::<FsCreated>(Request::FsNewNote(FsNewNoteParams {
            project_id,
            worktree_task_id: target,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let made = made.await;
            let _ = this.update_in(cx, |shell, window, cx| match made {
                Ok(FsCreated { path }) => {
                    // The pane dropped on may have closed while the daemon wrote the file.
                    let place = match at {
                        Some((key, edge)) if shell.project_holding(&key).is_some() => {
                            documents::Place::Split(key, edge)
                        }
                        _ => documents::Place::NewTab,
                    };
                    shell.open_document(project_id, path.clone(), None, place, window, cx);
                    shell.set_file_mode(project_id, &path, MdMode::Source, window, cx);
                    shell.refresh_files(project_id, window, cx);
                }
                Err(error) => {
                    tracing::warn!(%project_id, "new note failed: {error:#}");
                    shell.toast_error(format!("Cannot create the note: {error:#}"), cx);
                }
            });
        })
        .detach();
    }
}

impl PaneDragged for DraggedNew {
    /// Any pane that belongs to a workspace takes it: the session or note starts in the
    /// workspace that holds the pane (`project_holding`).
    fn takes(&self, shell: &Shell, _project_id: Ulid, target: &PaneKey) -> bool {
        shell.project_holding(target).is_some()
    }

    fn land(
        &self,
        shell: &mut Shell,
        _project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        shell.start_new(self.kind.clone(), Some((target, edge)), window, cx);
    }
}
