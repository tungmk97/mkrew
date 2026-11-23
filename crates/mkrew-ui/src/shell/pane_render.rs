//! Tab bar, tabs, rename field, pane frame and header.

use crate::*;

impl Shell {
    /// The tab area of the main window's title bar: the Editor / Diff chips (while open), the
    /// pinned tabs, the other tabs, a rule between groups; then `+` outside the scrolled part.
    /// With a view other than Terminals shown, its chip is the active one and every terminal
    /// tab looks inactive. A click on a tab shows Terminals (`tab_chip`).
    pub(crate) fn render_tab_area(
        &self,
        project_id: Ulid,
        tabs: &Tabs,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let view = self.view();
        let active = tabs.active().map(|tab| tab.id);
        // `place` counts the tabs of other windows too: it is the reorder index.
        let drawn: Vec<(usize, &Tab)> = tabs
            .tabs()
            .iter()
            .enumerate()
            .filter(|(_, tab)| self.terminal_host(project_id, tab.id) == HostId::Main)
            .collect();
        let pinned_count = tabs.tabs().iter().filter(|tab| tab.pinned).count();
        let pinned: Vec<bool> = drawn.iter().map(|(_, tab)| tab.pinned).collect();
        let items = area_items(
            self.has_file_tabs(project_id),
            self.diff_open(project_id),
            &pinned,
        );
        let strip = (self.hosts.get(&HostId::Main))
            .map(|ui| ui.strip.clone())
            .unwrap_or_default();
        let chips: Vec<gpui::AnyElement> = items
            .iter()
            .map(|item| match *item {
                AreaItem::View(chip) => self
                    .view_chip(chip, project_id, view, theme, cx)
                    .into_any_element(),
                AreaItem::Rule => title_rule(theme).into_any_element(),
                AreaItem::Tab(slot) => {
                    let (place, tab) = drawn[slot];
                    let tab_id = tab.id;
                    // The tab's own rename field takes its place; a pane is renamed in its
                    // header.
                    if let Some(renaming) =
                        (self.renaming.as_ref()).filter(|r| r.target == RenameTarget::Tab(tab_id))
                    {
                        self.render_rename(&renaming.draft, theme, cx)
                    } else {
                        // The focused pane's title stands in for a default tab name.
                        let label = session_label(tab, self.sidebar.sessions());
                        let item = DraggedItem {
                            list: ReorderList::Tabs(project_id, tab.pinned),
                            index: if tab.pinned {
                                place
                            } else {
                                place - pinned_count
                            },
                            title: label.unwrap_or(&tab.title).to_owned().into(),
                        };
                        let select = move |shell: &mut Shell| {
                            if let Some(tabs) = shell.active_tabs_mut() {
                                tabs.activate(tab_id);
                            }
                        };
                        let chip = self
                            .tab_chip(
                                project_id,
                                tab,
                                active == Some(tab_id) && view == View::Terminals,
                                None,
                                select,
                                theme,
                                cx,
                            )
                            .child(record_strip(strip.clone(), Some(slot)));
                        self.reorderable_shrinking(
                            chip.into_any_element(),
                            item,
                            true,
                            space::XS,
                            Some(theme::TAB_MIN_WIDTH),
                            theme,
                            cx,
                        )
                    }
                }
            })
            .collect();
        let active_slot = active.and_then(|id| drawn.iter().position(|(_, tab)| tab.id == id));
        // The key is the scroll target only: the shown chip, or the active tab.
        let key = SharedString::from(match view {
            View::Terminals => format!("tab/{active:?}"),
            other => format!("view/{other:?}"),
        });
        let scroll_to = scroll_index(&items, view, active_slot).map(|index| (key, index));
        self.tab_area_frame(
            HostId::Main,
            chips,
            scroll_to,
            Some(self.render_new_tab_button(now, theme, cx)),
            theme,
            cx,
        )
    }

    /// The frame both windows' tab areas share: `chips` in a sideways-scrolling row (the chip
    /// at `active.1` scrolls into view when `active.0` changes), then `trailing` (the `+`)
    /// outside the scrolled part. It records the strip and its chip slots for drops from other
    /// windows (`HostUi::strip`), takes tab drops and draws the drop line.
    pub(crate) fn tab_area_frame(
        &self,
        host: HostId,
        chips: Vec<gpui::AnyElement>,
        active: Option<(SharedString, usize)>,
        trailing: Option<gpui::AnyElement>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let ui = self.hosts.get(&host);
        let strip = ui.map(|ui| ui.strip.clone()).unwrap_or_default();
        let scroll = ui.map(|ui| ui.scroll.clone()).unwrap_or_default();
        if let Some(ui) = ui {
            let mut seen = ui.scrolled.borrow_mut();
            match active {
                Some((key, index)) if seen.as_ref() != Some(&key) => {
                    scroll.scroll_to_item(index);
                    *seen = Some(key);
                }
                Some(_) => {}
                // No target (Board, Docs): the next target scrolls again.
                None => *seen = None,
            }
        }
        div()
            .relative()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .font_family(theme.ui_font_family.clone())
            .on_drop(cx.listener(|shell, item: &DraggedItem, window, cx| {
                shell.drop_reordered(item, window, cx);
            }))
            // First: the strip records before its chips (prepaint goes parent first).
            .child(record_strip(strip, None))
            .child(
                div()
                    .id("tab-area-scroll")
                    .min_w_0()
                    .flex_shrink_1()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .overflow_x_scroll()
                    .track_scroll(&scroll)
                    .children(chips),
            )
            .children(trailing.map(|el| div().flex_none().child(el)))
            .children(self.drop_hint_line(host, theme))
    }

    /// The Editor / Diff chip: icon, name and ✕ (all file tabs / the diff). The chip of the
    /// shown view takes the active tab look. Not draggable.
    fn view_chip(
        &self,
        chip: View,
        project_id: Ulid,
        shown: View,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let active = chip == shown;
        let name = chip.title();
        let (glyph, close_tip) = match chip {
            View::Diff => (Icon::FileDiff, "Close diff"),
            _ => (Icon::Code, "Close all files"),
        };
        let group = SharedString::from(format!("view-{name}"));
        tab_frame(group.clone(), active, theme)
            .flex_none()
            .tooltip(tooltip(theme, name, self.keymap.label(chip.command())))
            .on_press(
                true,
                cx.listener(move |shell, _, window, cx| shell.show_view(chip, window, cx)),
            )
            .child(icon_svg(
                glyph,
                theme::ICON_SIZE_SMALL,
                if active { theme.text } else { theme.text_muted },
            ))
            .child(name)
            .child(
                row_action(
                    SharedString::from(format!("view-close-{name}")),
                    icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),
                    theme,
                )
                // Always on the shown chip, on hover for the other.
                .when(!active, |el| {
                    el.opacity(opacity::HIDDEN)
                        .group_hover(group, |style| style.opacity(opacity::FULL))
                })
                .tooltip(tooltip(theme, close_tip, ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    match chip {
                        View::Diff => shell.close_git_diff(project_id, window, cx),
                        _ => {
                            let path = (shell.files.get(&project_id))
                                .and_then(FilesView::file)
                                .map(str::to_owned);
                            if let Some(path) = path {
                                // Pinned file tabs stay (`Close::All`), as in the tab menu.
                                shell.close_file_tabs(project_id, &path, Close::All, window, cx);
                            }
                        }
                    }
                })),
            )
    }

    /// The Board / Docs icon on the right of the title bar: accent while that view shows (a
    /// second click goes back to Terminals).
    pub(crate) fn view_icon(
        &self,
        target: View,
        glyph: Icon,
        shown: View,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let on = target == shown;
        icon_button_in(
            SharedString::from(format!("view-{}", target.title())),
            glyph,
            if on { theme.accent } else { theme.text_muted },
            theme,
        )
        .tooltip(tooltip(
            theme,
            target.title(),
            self.keymap.label(target.command()),
        ))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.show_view(if on { View::Terminals } else { target }, window, cx);
        }))
    }

    /// One tab of the title bar's tab area (main or detached): kind icon, title, status dot, pin,
    /// ×. `select` makes it the shown tab of its window (a click); `prefix` (the workspace name,
    /// in a detached strip) leads the title. The caller wraps it for drag and drop.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tab_chip(
        &self,
        project_id: Ulid,
        tab: &Tab,
        is_active: bool,
        prefix: Option<&str>,
        select: impl Fn(&mut Shell) + 'static,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tab_id = tab.id;
        // The focused pane's title stands in for a default tab name.
        let label = session_label(tab, self.sidebar.sessions());
        // A double-click renames that pane while the tab keeps its default name, else the tab.
        let rename_pane = double_click_renames(tab, self.sidebar.sessions());
        // The most urgent state among the tab's panes (SPEC §11), as one dot.
        let urgent = status::most_urgent(self.sidebar.sessions(), &tab.sessions());
        let dot = self.tab_dot(tab, urgent, theme);
        let group = SharedString::from(format!("tab-{tab_id}"));
        let icon = match tab_kind(tab, self.sidebar.sessions()) {
            TabKind::Agent(profile) => {
                Some(agent_chip(self.agent_badge(&profile), theme.text, theme))
            }
            // The `>_` icon in a chip-sized box, so shell and agent tabs line up.
            TabKind::Shell => Some(
                div()
                    .flex_none()
                    .size(theme::rems_px(theme::AGENT_CHIP_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon_svg(
                        Icon::Terminal,
                        theme::ICON_SIZE_SMALL,
                        theme.text_muted,
                    )),
            ),
            TabKind::Empty => None,
        };
        tab_frame(group.clone(), is_active, theme)
            // Shrinks to `TAB_MIN_WIDTH` before the tab area scrolls.
            .flex_shrink_1()
            .min_w(px(theme::TAB_MIN_WIDTH))
            .on_press(
                true,
                cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                    select(shell);
                    // A tab of the main window shows its terminals, also from Board, Docs,
                    // the Editor or the Diff.
                    if shell.host_of(window) == HostId::Main {
                        shell.show_view(View::Terminals, window, cx);
                    }
                    shell.sync_panes(window, cx);
                    if event.click_count() == 2 {
                        match rename_pane {
                            Some(session) => shell.start_pane_rename(session, window, cx),
                            None => shell.start_rename(window, cx),
                        }
                    }
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                    shell.open_tab_menu(tab_id, event.position, window, cx);
                }),
            )
            .children(icon)
            .when_some(urgent, |el, session| {
                el.tooltip(status_tooltip(
                    theme,
                    session.state,
                    session.state_source,
                    session.state_changed_at,
                ))
            })
            .when_some(label.filter(|_| urgent.is_none()), |el, label| {
                el.tooltip(tooltip(theme, label, ""))
            })
            .when(tab.pinned, |el| el.child(pin_mark()))
            .children(prefix.map(|name| {
                div()
                    .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                    .truncate()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(name.to_owned())
            }))
            .child(
                div()
                    .min_w_0()
                    .max_w(px(theme::TAB_TITLE_MAX_WIDTH))
                    .truncate()
                    .child(label.unwrap_or(&tab.title).to_owned()),
            )
            .children(dot.map(|(color, hollow)| status_dot(color, hollow)))
            .child(
                row_action(
                    SharedString::from(format!("tab-close-{tab_id}")),
                    icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),
                    theme,
                )
                // Always on the active tab, on hover for the others.
                .when(!is_active, |el| {
                    el.opacity(opacity::HIDDEN)
                        .group_hover(group, |style| style.opacity(opacity::FULL))
                })
                .tooltip(tooltip(
                    theme,
                    "Close tab",
                    if is_active {
                        self.keymap.label(Command::CloseTab)
                    } else {
                        String::new()
                    },
                ))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    // Never activate the tab being closed.
                    cx.stop_propagation();
                    shell.close_tab(project_id, tab_id, window, cx);
                })),
            )
    }

    /// The title being renamed inline: the draft with its caret and selection, in the active
    /// tab's place; a click puts the caret under the pointer, a drag selects.
    pub(crate) fn render_rename(
        &self,
        draft: &Input,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (text, layout) = caret_text(
            draft.as_str(),
            draft.cursor(),
            draft.selection(),
            "",
            true,
            theme,
        );
        let field = div()
            .id("tab-rename")
            .flex()
            .items_center()
            .min_w(px(theme::TAB_RENAME_MIN_WIDTH))
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.pane_focus_border))
            .bg(rgb(theme.surface_raised))
            .whitespace_nowrap()
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.rename_focus)
            .on_key_down(cx.listener(Self::rename_key_down))
            .relative()
            .child(self.typing(&self.rename_focus, Self::rename_key_down, cx))
            .child(text);
        caret_mouse(field, &cx.entity(), layout, |shell, caret| {
            if let Some(renaming) = shell.renaming.as_mut() {
                renaming.draft.click(caret.at, caret.extend, caret.clicks);
            }
        })
        .into_any_element()
    }

    /// The tab's split tree: each pane placed at its fraction of the area (`None`: the
    /// empty state). `composer_focused`: the focused pane's composer has the keyboard.
    /// `voice_ring`: the focused pane's terminal gets the voice text; its ring color (§20G).
    /// `content` gets the size of the area.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_tab(
        &self,
        tab: Option<&Tab>,
        now: i64,
        composer_focused: bool,
        voice_ring: Option<Hsla>,
        theme: &Theme,
        content: Rc<Cell<gpui::Size<Pixels>>>,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        if tab.and_then(|tab| tab.layout()).is_none() {
            return empty_state(
                theme,
                Some(">_"),
                Some("No panes in this tab"),
                "Start an agent or a shell, or pick a session from the sidebar.".into(),
                None,
            )
            .flex_1()
            .child(match self.starting_pane {
                Some(kind) => starting_button("empty-starting", kind, now, theme, true),
                None => div()
                    .flex()
                    .gap(px(space::S))
                    .child(
                        button(
                            "empty-new-agent",
                            icon_text(Icon::Bot, "New agent", button_fg(theme, true)),
                            theme,
                            true,
                        )
                        .child(kbd(self.keymap.label(Command::SplitAgent), theme, true))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.new_pane(SessionKind::Agent, Axis::Vertical, window, cx);
                        })),
                    )
                    .child(
                        secondary_button(
                            "empty-new-shell",
                            icon_text(Icon::SquareTerminal, "New shell", theme.text),
                            theme,
                        )
                        .child(kbd(self.keymap.label(Command::SplitShell), theme, false))
                        .tooltip(tooltip(theme, "New shell pane in the workspace folder", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.new_pane(SessionKind::Shell, Axis::Vertical, window, cx);
                        })),
                    )
                    .into_any_element(),
            })
            .relative()
            .child(content_canvas(content));
        }
        let focused = tab.and_then(|tab| tab.focused());
        let rects = tab.map(Tab::rects).unwrap_or_default();
        let dividers = tab.map(Tab::dividers).unwrap_or_default();
        let several = rects.len() > 1;
        let zoomed = tab.is_some_and(|tab| tab.zoomed().is_some());
        let half_gap = px(theme::PANE_HALF_GAP);
        // The panes' 0..1 space in px, as laid out in the last frame (`content_canvas`).
        let area = f32::from(content.get().width) - 2.0 * theme::PANE_HALF_GAP;
        let panes: Vec<_> = rects
            .into_iter()
            .map(|(session, rect)| {
                let is_focused = focused == Some(session);
                let width = rect.w as f32 * area - 2.0 * theme::PANE_HALF_GAP;
                div()
                    .absolute()
                    .left(relative(rect.x as f32))
                    .top(relative(rect.y as f32))
                    .w(relative(rect.w as f32))
                    .h(relative(rect.h as f32))
                    .p(half_gap)
                    .child(self.pane_frame(
                        session,
                        several,
                        zoomed,
                        width,
                        is_focused,
                        is_focused && composer_focused,
                        voice_ring.filter(|_| is_focused),
                        now,
                        theme,
                        cx,
                    ))
            })
            .collect();
        let dividers: Vec<_> = dividers
            .into_iter()
            .map(|divider| pane_divider(divider, theme, cx))
            .collect();
        div()
            .flex_1()
            .min_h_0()
            .p(half_gap)
            .relative()
            .child(content_canvas(content))
            .child(
                div()
                    .id("tab-panes")
                    .size_full()
                    .relative()
                    .children(panes)
                    .children(dividers)
                    // Every divider drag lands here: `bounds` is this area, the panes' 0..1 space.
                    .on_drag_move(cx.listener(
                        |shell, event: &DragMoveEvent<Divider>, window, cx| {
                            let divider = *event.drag(cx);
                            cx.set_active_drag_cursor_style(divider_cursor(divider.axis), window);
                            let bounds = event.bounds;
                            let at = event.event.position - bounds.origin;
                            let x = f64::from(at.x / bounds.size.width);
                            let y = f64::from(at.y / bounds.size.height);
                            let ratio = divider.ratio_at(x, y);
                            // ponytail: live ratio, one `session.resize` per changed column count
                            // like the session panel's edge; throttle if a TUI flickers.
                            if shell
                                .with_tab(window, |tabs| tabs.drag_divider(divider.index, ratio))
                                .unwrap_or(false)
                            {
                                shell.pane_dragging = true;
                                cx.notify();
                            }
                        },
                    )),
            )
    }

    /// One pane: its header, the terminal (or why it is not shown; at reading width, centered,
    /// for an agent pane with it on), its composer, its border. Every pane has the compact
    /// header, a single and a zoomed one too; among several panes the unfocused ones get a dim.
    /// `zoomed`: the pane fills the tab, its header shows the zoom notice. `voice_ring`: the
    /// listening ring over the border; `width`: the pane's width in px in the last frame (the
    /// header's room).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pane_frame(
        &self,
        session: Ulid,
        several: bool,
        zoomed: bool,
        width: f32,
        is_focused: bool,
        composer_focused: bool,
        voice_ring: Option<Hsla>,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let info = self.sidebar.sessions().iter().find(|s| s.id == session);
        let content = if info.is_some_and(|info| self.chat_on(info)) {
            self.render_chat(session, is_focused, now, theme, cx)
        } else {
            match self.panes.get(&session) {
                Some(pane) => pane.clone().into_any_element(),
                None => {
                    let (sentence, detail) = self.pane_placeholder(session);
                    message(theme, None, sentence, detail)
                        .size_full()
                        .into_any_element()
                }
            }
        };
        let border = status::pane_border(theme, info.map(|s| s.state), several, is_focused);
        let reading = info.is_some_and(|info| self.reading_on(info));
        let color = (info.and_then(|info| info.color.as_deref()))
            .and_then(|name| theme.workspace_color(name));
        let accent_border = theme.accent;
        let frame = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.terminal_bg))
            .on_drop(
                cx.listener(move |shell, dragged: &DraggedPath, window, cx| {
                    if shell.workspace == Some(dragged.project_id) {
                        shell.drop_tree_path(session, dragged, window, cx);
                    }
                }),
            );
        self.pane_drop_target(frame, session, cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, window, cx| {
                    shell.focus_session_pane(session);
                    shell.focus_pane(window, cx);
                    cx.notify();
                }),
            )
            .on_drag_move::<ExternalPaths>(cx.listener(
                move |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    // A modal dialog covers the pane: its own box takes the drop, and this
                    // pane's catcher (painted first) must not take the slot from it.
                    if !shell.modal_open() && event.bounds.contains(&event.event.position) {
                        shell.drop_hover =
                            Some((DropTarget::Pane(session), event.drag(cx).clone()));
                    } else if shell
                        .drop_hover
                        .as_ref()
                        .is_some_and(|(hovered, _)| *hovered == DropTarget::Pane(session))
                    {
                        shell.drop_hover = None;
                    }
                },
            ))
            .child(drop_catcher(DropTarget::Pane(session), cx))
            .children(info.map(|info| self.pane_header(info, color, width, zoomed, now, theme, cx)))
            // Reading width: the terminal element is narrower, so the PTY gets fewer columns
            // (the program wraps there); nothing reflows its output.
            .child(
                div().flex_1().min_h_0().flex().justify_center().child(
                    div()
                        .h_full()
                        .w_full()
                        .when(reading, |el| el.max_w(px(theme::READING_WIDTH)))
                        .child(content),
                ),
            )
            .children(self.composers.shown(session).map(|draft| {
                self.composer_panel(
                    session,
                    draft,
                    is_focused,
                    composer_focused,
                    width,
                    now,
                    theme,
                    cx,
                )
            }))
            .when(several && !is_focused, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgb(theme.terminal_bg))
                        .opacity(theme.unfocused_pane_dim),
                )
            })
            // Drawn above the terminal so its corners stay rounded.
            .when_some(border, |el, border| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(radius::MEDIUM))
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(border)),
                )
            })
            // The drop ring: an overlay, so the terminal keeps its size and the pointer.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::transparent_black())
                    .drag_over::<DraggedPath>(move |style, _, _, _| {
                        style.border_color(rgb(accent_border))
                    }),
            )
            .children(self.pane_drop_overlay(session, theme, cx))
            .when_some(voice_ring, |el, color| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(radius::MEDIUM))
                        .border(px(theme::VOICE_RING_WIDTH))
                        .border_color(color),
                )
            })
    }

    /// An agent pane at reading width: its own toggle, else `[ui] reading_width`. Shell and run
    /// panes are always full width.
    pub(crate) fn reading_on(&self, info: &Session) -> bool {
        info.kind == SessionKind::Agent
            && *self
                .reading_width
                .get(&info.id)
                .unwrap_or(&self.reading_default)
    }

    /// Reading width on / off for `session`'s pane (its header button, and the palette).
    pub(crate) fn toggle_reading_width(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        let on = (self.sidebar.sessions().iter())
            .find(|s| s.id == session)
            .is_some_and(|info| self.reading_on(info));
        self.reading_width.insert(session, !on);
        cx.notify();
    }

    /// The status of `info`'s pane, each part only when it has something to say: the task
    /// folder, the status pill, the conflict warning, `⧗ N queued` sends and the context share.
    /// The pane header shows them; `context`: whether there is room for the context share.
    pub(crate) fn pane_status(
        &self,
        info: &Session,
        context: bool,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let session = info.id;
        let mut parts = Vec::new();
        if let Some(folder) = pane_text::folder(info.task_id.and_then(|id| self.board.task(id))) {
            parts.push(
                div()
                    .flex_none()
                    .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                    .truncate()
                    .text_color(rgb(theme.text_muted))
                    .child(folder)
                    .into_any_element(),
            );
        }
        let pill = pane_text::pill_text(
            info.state,
            info.exit_code,
            info.state_changed_at,
            now,
            self.turns.get(session),
        );
        parts.push(status_pill(info, pill, theme).into_any_element());
        let paths = self.conflicts.for_session(session);
        if !paths.is_empty() {
            parts.push(
                conflict_badge(
                    format!("pane-{session}"),
                    &paths,
                    theme,
                    move |shell| shell.conflicts.dismiss_session(session),
                    cx,
                )
                .flex_none()
                .max_w(relative(theme::PANE_CONFLICT_MAX_FRACTION))
                .into_any_element(),
            );
        }
        let queued = self.composers.queue(session).len();
        if queued > 0 {
            parts.push(
                div()
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("⧗ {queued} queued"))
                    .into_any_element(),
            );
        }
        if let Some(share) = (self.usages.get(session))
            .and_then(usage::context_share)
            .filter(|_| context)
        {
            parts.push(
                div()
                    .id(SharedString::from(format!("pane-context-{session}")))
                    .flex_none()
                    .text_color(rgb(if share.warning {
                        theme.status_needs_input
                    } else {
                        theme.text_muted
                    }))
                    .tooltip(tooltip(theme, &share.hover, ""))
                    .child(if share.warning {
                        format!("⚠ {}", share.label)
                    } else {
                        share.label
                    })
                    .into_any_element(),
            );
        }
        if self
            .panes
            .get(&session)
            .is_some_and(|pane| pane.read(cx).in_copy_mode())
        {
            parts.push(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .rounded_full()
                    .whitespace_nowrap()
                    .bg(rgb(theme.accent_bg))
                    .child(
                        div()
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.accent_text))
                            .child("Copy mode"),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme.text_muted))
                            .child("v select · y copy · Esc exit"),
                    )
                    .into_any_element(),
            );
        }
        parts
    }

    /// An agent pane's controls: the Terminal | Chat switch, the Reading width toggle and `Share`
    /// (its menu: Follow in Zed), each when `room` has it.
    /// Nothing for a shell or run pane.
    pub(crate) fn pane_controls(
        &self,
        info: &Session,
        room: pane_text::HeaderRoom,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        if info.kind != SessionKind::Agent {
            return Vec::new();
        }
        let session = info.id;
        let reading = self.reading_on(info);
        let following = self.follow.is_on(session);
        let reading_toggle = icon_button_in(
            SharedString::from(format!("pane-reading-{session}")),
            Icon::BookOpen,
            if reading {
                theme.text
            } else {
                theme.text_muted
            },
            theme,
        )
        .when(reading, |el| el.bg(rgb(theme.surface_raised)))
        .tooltip(tooltip(
            theme,
            if reading {
                "Reading width: on (the agent wraps at about 130 columns). Click for full width"
            } else {
                "Reading width: off (full width). Click to wrap at about 130 columns"
            },
            "",
        ))
        .on_click(cx.listener(move |shell, _, _, cx| shell.toggle_reading_width(session, cx)));
        let share = button_frame(
            "pane-share",
            icon_text(
                Icon::Share2,
                "Share",
                if following {
                    theme.text
                } else {
                    theme.text_muted
                },
            ),
        )
        .flex_none()
        .px(px(space::S))
        .py_0()
        .text_size(font_size::META)
        .when(following, |el| {
            el.bg(rgb(theme.card))
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
        })
        .when(!following, |el| el.text_color(rgb(theme.text_muted)))
        // Following is picked, not disabled: it keeps the pointer (it opens the Share menu).
        .pressable_unless(following, Look::Chip, theme)
        .tooltip(tooltip(
            theme,
            if following {
                "Share: following this agent's edits in Zed"
            } else {
                "Share this agent's work"
            },
            "",
        ))
        .on_press(
            true,
            cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                shell.open_share_menu(session, event.position(), window, cx);
            }),
        );
        let mut controls = Vec::new();
        if room.switch {
            controls.extend(self.pane_mode_switch(info, theme, cx));
        }
        if room.reading {
            controls.push(reading_toggle.into_any_element());
        }
        if room.share {
            controls.push(share.into_any_element());
        }
        controls
    }

    /// The inline rename field of `session`'s pane while it is edited (F2, the pane menu, or a
    /// double-click on a tab that keeps its default name, ADR 0031); `None` otherwise. The
    /// header has no title, so the field takes the header's free space.
    fn pane_rename(
        &self,
        session: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let renaming =
            (self.renaming.as_ref()).filter(|r| r.target == RenameTarget::Session(session))?;
        // Mouse downs stay in the field: the pane's handler would focus the terminal, and the
        // blur would commit half-typed text.
        Some(
            div()
                .flex_1()
                .min_w_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .child(self.render_rename(&renaming.draft, theme, cx))
                .into_any_element(),
        )
    }

    /// A pane's compact header (SPEC §11), on every pane, a single and a zoomed one too: the
    /// agent chip (tinted by the pane color, which also draws a line under the header; its
    /// tooltip is the pane's full title), the status parts, then the zoom notice while `zoomed`
    /// and the agent controls. It has no title: the tab shows the focused pane's. While the pane
    /// is renamed its field takes the free space. Right-click: the pane menu.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pane_header(
        &self,
        info: &Session,
        color: Option<u32>,
        width: f32,
        zoomed: bool,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = info.id;
        let context_chars = (self.usages.get(session))
            .and_then(usage::context_share)
            .map_or(0, |share| share.label.chars().count());
        let room = pane_text::header_room(
            width,
            cx.global::<theme::Fonts>().ui_size,
            context_chars,
            self.chat_available(info),
        );
        let title = (run::pane_title(info, self.ports.get(session)))
            .unwrap_or_else(|| info.agent_profile.clone());
        let zoom_notice = zoomed.then(|| {
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(notice(theme.text_muted, "⤢ Pane zoomed".to_owned()))
                .child(
                    button_frame(
                        "pane-unzoom",
                        icon_text(Icon::Minimize2, "Show all panes", theme.text_muted),
                    )
                    .px(px(space::S))
                    .py_0()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .pressable(Look::Chip, true, theme)
                    .tooltip(tooltip(
                        theme,
                        "Show all panes",
                        self.keymap.label(Command::ZoomPane),
                    ))
                    .on_press(
                        true,
                        cx.listener(|shell, _: &gpui::ClickEvent, window, cx| {
                            shell.run(&Run(Command::ZoomPane), window, cx);
                        }),
                    ),
                )
        });
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            // Transparent without a color, so the header keeps its height.
            .border_b(px(theme::PANE_ACCENT_WIDTH))
            .border_color(match color {
                Some(color) => Hsla::from(rgb(color)),
                None => gpui::transparent_black(),
            })
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::META)
            .child(
                agent_chip(self.agent_badge(&info.agent_profile), theme.text, theme)
                    .when_some(color, |chip, color| {
                        chip.bg(theme.tint(color)).border_color(rgb(color))
                    })
                    .id(SharedString::from(format!("pane-chip-{session}")))
                    .tooltip(tooltip(theme, &title, "")),
            )
            .children(self.pane_status(info, room.context, now, theme, cx))
            // The free space pushes the controls right; the rename field takes it.
            .child(
                self.pane_rename(session, theme, cx)
                    .unwrap_or_else(|| div().flex_1().min_w_0().into_any_element()),
            )
            .children(zoom_notice)
            .children(self.pane_controls(info, room, theme, cx))
            .relative()
            .child(record_bounds(Anchor::PaneHeader(session), cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                    shell.open_pane_menu(session, event.position, window, cx);
                }),
            );
        self.draggable_pane(header, info, theme)
    }

    /// A tab's status dot from its most urgent pane, always still and one color per state:
    /// amber needs input, red exited, indigo while an agent works, green for done or a live run
    /// process, else a hollow gray ring (idle, starting). `(color, hollow)`; `None` for an
    /// empty tab.
    pub(crate) fn tab_dot(
        &self,
        tab: &Tab,
        urgent: Option<&Session>,
        theme: &Theme,
    ) -> Option<(u32, bool)> {
        let urgent = urgent?;
        let sessions = self.sidebar.sessions();
        let run_live = tab.sessions().iter().any(|id| {
            sessions.iter().any(|s| {
                s.id == *id
                    && s.state != SessionState::Exited
                    && mkrew_core::run::command_name(s).is_some()
            })
        });
        Some(match urgent.state {
            SessionState::NeedsInput => (theme.status_needs_input, false),
            SessionState::Exited => (theme.status_exited, false),
            SessionState::Working if urgent.kind == SessionKind::Agent => {
                (theme.status_working, false)
            }
            _ if run_live => (theme.status_done, false),
            SessionState::Done => (theme.status_done, false),
            _ => (theme.status_idle, true),
        })
    }

    /// Why a pane on screen has no terminal yet.
    pub(crate) fn pane_placeholder(&self, session: Ulid) -> (SharedString, Option<SharedString>) {
        // An exit explains a failed attach better than its error does.
        if let Some(s) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session && s.state == SessionState::Exited)
        {
            let text = match s.exit_code {
                Some(code) => format!("✕ Session exited (code {code})."),
                None => "✕ Session exited.".to_owned(),
            };
            return (text.into(), None);
        }
        match self.attach_errors.get(&session) {
            Some(error) => (
                "✕ Cannot attach the session.".into(),
                Some(error.clone().into()),
            ),
            None => ("Attaching…".into(), None),
        }
    }
}

/// The button of a pane being started: "Starting agent" with the loading dots, no click.
pub(crate) fn starting_button(
    id: &'static str,
    kind: SessionKind,
    now: i64,
    theme: &Theme,
    primary: bool,
) -> gpui::AnyElement {
    let label = match kind {
        SessionKind::Agent => "Starting agent",
        SessionKind::Shell => "Starting shell",
    };
    let color = if primary {
        theme.text_disabled
    } else {
        theme.text_muted
    };
    disabled_button(id, label, theme, primary)
        .opacity(opacity::FULL)
        .gap(px(space::S))
        .cursor(CursorStyle::OperationNotAllowed)
        .child(loading_dots(now, rgb(color)))
        .into_any_element()
}

/// Fire-and-forget `session.detach`; a failure is logged.
pub(crate) fn detach(connection: &Connection, session_id: Ulid, cx: &mut Context<'_, Shell>) {
    let reply = connection.request(Request::SessionDetach(SessionRef { session_id }));
    cx.background_executor()
        .spawn(async move {
            match reply.await {
                Ok(Outcome::Result(_)) => {}
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%session_id, "session.detach failed: {}", body.message);
                }
                Err(error) => tracing::warn!(%session_id, "session.detach failed: {error:#}"),
            }
        })
        .detach();
}

/// What an OS file drag hovers (§20B.5, ADR 0095).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropTarget {
    /// An agent or shell pane.
    Pane(Ulid),
    /// The Prompt box of the open task dialog.
    TaskPrompt,
}

/// Delivers an OS file drop on `target` (see `Shell::drop_hover`).
// ponytail: works around gpui-pre 0.3.7 keeping keyboard modality during OLE drags
// (window.rs:5415 skips FileDrop), which makes every hover-gated mouse-up listener (`on_drop`,
// `capture_any_mouse_up`) miss drops after typing. Use plain `.on_drop` on the pane when
// upstream treats FileDrop as mouse input.
pub(crate) fn drop_catcher(
    target: DropTarget,
    cx: &Context<'_, Shell>,
) -> impl IntoElement + use<> {
    let view = cx.entity();
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let view = view.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                // A plain click has no active drag: stale paths are never typed.
                if phase != DispatchPhase::Capture || !cx.has_active_drag() {
                    return;
                }
                view.update(cx, |shell, cx| {
                    let hovered = shell.drop_hover.take_if(|(hovered, _)| *hovered == target);
                    if let Some((_, paths)) = hovered {
                        cx.stop_propagation();
                        match target {
                            DropTarget::Pane(session) => {
                                shell.drop_paths(session, &paths, window, cx);
                            }
                            DropTarget::TaskPrompt => shell.drop_prompt_paths(&paths, window, cx),
                        }
                    }
                });
            });
        },
    )
    .absolute()
    .size_full()
}

/// A pane's status pill (SPEC §11): status mark + `Working · 1h 25m` (with the last turn's
/// summary when done); hovering it shows the status source and last change. In a narrow header
/// the text ends with an ellipsis before the controls lose room.
pub(crate) fn status_pill(
    session: &Session,
    pill: String,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let (state, changed_at) = (session.state, session.state_changed_at);
    div()
        .id(SharedString::from(format!("pane-status-{}", session.id)))
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .h(theme::rems_px(theme::STATUS_PILL_HEIGHT))
        .px(px(space::S))
        .rounded_full()
        .whitespace_nowrap()
        .bg(rgb(theme.card))
        .text_color(rgb(theme.text_muted))
        .tooltip(status_tooltip(
            theme,
            state,
            session.state_source,
            changed_at,
        ))
        .child(status_mark(state, theme))
        .child(div().min_w_0().truncate().child(pill))
}

fn divider_cursor(axis: Axis) -> CursorStyle {
    match axis {
        Axis::Vertical => CursorStyle::ResizeLeftRight,
        Axis::Horizontal => CursorStyle::ResizeUpDown,
    }
}

/// The grab strip over a split's divider: centred on the gap between its two children, as long
/// as the split. Dragging it is handled by the panes area (`render_tab`), which knows the size.
fn pane_divider(divider: Divider, theme: &Theme, cx: &mut Context<'_, Shell>) -> gpui::AnyElement {
    let area = divider.area;
    let at = |start: f64, len: f64| relative((start + len * divider.ratio) as f32);
    let grip = px(theme::GRIP_WIDTH);
    let centre = px(-theme::GRIP_WIDTH / 2.0);
    // A zero-thick line along the divider; the strip hangs half on each side of it.
    let (line, strip) = match divider.axis {
        Axis::Vertical => (
            div()
                .left(at(area.x, area.w))
                .top(relative(area.y as f32))
                .h(relative(area.h as f32)),
            div().top_0().bottom_0().left(centre).w(grip),
        ),
        Axis::Horizontal => (
            div()
                .top(at(area.y, area.h))
                .left(relative(area.x as f32))
                .w(relative(area.w as f32)),
            div().left_0().right_0().top(centre).h(grip),
        ),
    };
    line.absolute()
        .child(
            grip_hover(strip.id(("pane-divider", divider.index)), theme)
                .absolute()
                .cursor(divider_cursor(divider.axis))
                .on_drag(divider, |_, _, _, cx| cx.new(|_| EmptyView))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|shell, _, window, cx| shell.end_pane_drag(window, cx)),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|shell, _, window, cx| shell.end_pane_drag(window, cx)),
                ),
        )
        .into_any_element()
}

/// Paints nothing; stores the size of its parent (the tab content area) for a window torn
/// off from here (`HostUi::content`) and for the pane headers' room (`render_tab`). A new
/// size draws the frame again, so the headers never keep the room of an older size.
fn content_canvas(content: Rc<Cell<gpui::Size<Pixels>>>) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            // `refresh` does nothing while the frame is drawn; this notifies the view after it.
            if content.replace(bounds.size) != bounds.size {
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}
