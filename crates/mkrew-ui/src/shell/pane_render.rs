//! Tab area, tabs, rename field, pane frame and its floating badges.

use crate::*;

impl Shell {
    /// The tab area of the main window's title bar: the pinned tabs, the other tabs, a rule
    /// between groups; then `+` outside the scrolled part. A click on a tab shows Terminals
    /// (`tab_chip`).
    pub(crate) fn render_tab_area(
        &self,
        project_id: Ulid,
        tabs: &Tabs,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let view = self.view(self.space_host(window));
        let active = tabs.active().map(|tab| tab.id);
        // `place` counts the tabs of other windows too: it is the reorder index.
        let drawn: Vec<(usize, &Tab)> = tabs
            .tabs()
            .iter()
            .enumerate()
            .filter(|(_, tab)| !self.is_detached_tab(project_id, tab.id))
            .collect();
        let pinned_count = tabs.tabs().iter().filter(|tab| tab.pinned).count();
        let pinned: Vec<bool> = drawn.iter().map(|(_, tab)| tab.pinned).collect();
        let items = area_items(&pinned);
        let host = self.host_of(window);
        let strip = (self.hosts.get(&host))
            .map(|ui| ui.strip.clone())
            .unwrap_or_default();
        let chips: Vec<gpui::AnyElement> = items
            .iter()
            .map(|item| match *item {
                AreaItem::Rule => title_rule(theme).into_any_element(),
                AreaItem::Tab(slot) => {
                    let (place, tab) = drawn[slot];
                    let tab_id = tab.id;
                    // The tab's own rename field takes its place, also while its pane is
                    // renamed.
                    if let Some(renaming) = self.tab_renaming(tab) {
                        self.render_rename(&renaming.draft, theme, cx)
                    } else {
                        // A file or the focused pane's title stands in for a default tab name.
                        let file = file_label(tab, self.documents.get(&project_id));
                        let label = (file.as_ref().map(|file| file.name.as_str()))
                            .or_else(|| diff_label(tab))
                            .or_else(|| {
                                web_label(tab)
                                    .and_then(|id| self.browsers.get(&id))
                                    .map(|b| b.page.label())
                            })
                            .or_else(|| session_label(tab, self.sidebar.sessions()));
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
                            if let Some(tabs) = shell.tabs.get_mut(&project_id) {
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
        // The key is the scroll target only: the active tab, while Terminals shows.
        let scroll_to = scroll_index(&items, active_slot)
            .filter(|_| view == View::Terminals)
            .map(|index| (SharedString::from(format!("tab/{active:?}")), index));
        self.tab_area_frame(
            host,
            chips,
            scroll_to,
            Some(self.render_new_tab_button(host, now, theme, window, cx)),
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
            .when(host.space().is_some(), |el| self.strip_drops(el, theme, cx))
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

    /// One tab of the title bar's tab area (main or detached): status icon, pane icons, pin,
    /// title, ×. `select` makes it the shown tab of its window (a click); `prefix` (the workspace name,
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
        // A file or the focused pane's title stands in for a default tab name; a dot marks
        // unsaved text and italics the preview tab.
        let file = file_label(tab, self.documents.get(&project_id));
        let label = (file.as_ref().map(|file| file.name.as_str()))
            .or_else(|| diff_label(tab))
            .or_else(|| {
                web_label(tab)
                    .and_then(|id| self.browsers.get(&id))
                    .map(|b| b.page.label())
            })
            .or_else(|| session_label(tab, self.sidebar.sessions()));
        // A double-click renames that pane while the tab keeps its default name, else the tab.
        let rename_pane = double_click_renames(tab, self.sidebar.sessions());
        // The most urgent pane (SPEC §11): the tooltip states its status in words.
        let urgent = status::most_urgent(self.sidebar.sessions(), &tab.sessions());
        let icons = tab_icons(tab, self.sidebar.sessions(), is_active);
        let group = SharedString::from(format!("tab-{tab_id}"));
        // Only a state to notice: Working or Needs input.
        let status_icon = icons.status.map(|status| {
            let (glyph, color) = match status {
                TabStatus::Working => (Icon::Hourglass, theme.status_working),
                TabStatus::NeedsInput => (Icon::Bell, theme.status_needs_input),
            };
            icon_svg(glyph, theme::TAB_ICON_SIZE, color)
        });
        // Every icon is `TAB_ICON_SIZE`: a brand logo in its brand color, an agent without
        // a logo as its glyph, shell and run panes in `text_muted`.
        let pane_icons: Vec<gpui::AnyElement> = icons
            .panes
            .iter()
            .map(|pane| match pane {
                PaneIcon::Agent(profile) => match self.agent_badge(profile) {
                    sidebar::Badge::Logo(logo) => {
                        icons::logo_svg(logo, theme::TAB_ICON_SIZE, theme.brand_color(logo))
                            .into_any_element()
                    }
                    sidebar::Badge::Text(text) => div()
                        .flex_none()
                        .pb(px(theme::TEXT_LIFT * 2.0))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(text)
                        .into_any_element(),
                },
                PaneIcon::Shell => {
                    icon_svg(Icon::SquareTerminal, theme::TAB_ICON_SIZE, theme.text_muted)
                        .into_any_element()
                }
                PaneIcon::Run => {
                    icon_svg(Icon::Play, theme::TAB_ICON_SIZE, theme.text_muted).into_any_element()
                }
                PaneIcon::File(_) => {
                    icon_svg(Icon::File, theme::TAB_ICON_SIZE, theme.text_muted).into_any_element()
                }
                PaneIcon::Diff => icon_svg(Icon::FileDiff, theme::TAB_ICON_SIZE, theme.text_muted)
                    .into_any_element(),
                PaneIcon::Web => {
                    icon_svg(Icon::Globe, theme::TAB_ICON_SIZE, theme.text_muted).into_any_element()
                }
            })
            .collect();
        // A file of a task worktree names it: a project file and a worktree file look alike.
        let worktree = match tab.focused() {
            Some(PaneKey::File(file)) => worktree_target::label(
                file.task,
                project_id,
                self.board.tasks(),
                self.board.is_loaded(),
            ),
            _ => None,
        };
        let title = label.unwrap_or(&tab.title);
        let dirty = file.as_ref().is_some_and(|file| file.dirty);
        let tip = match urgent {
            Some(pane) => format!(
                "{}\n{}",
                worktree_target::with_worktree(title, worktree.as_deref()),
                pane_text::pill_text(
                    pane.state,
                    pane.exit_code,
                    pane.state_changed_at,
                    status::now_ms(),
                    self.turns.get(pane.id),
                )
            ),
            None if dirty => {
                worktree_target::with_worktree(&format!("{title} (unsaved)"), worktree.as_deref())
            }
            None => worktree_target::with_worktree(title, worktree.as_deref()),
        };
        tab_frame(group.clone(), is_active, theme)
            // Shrinks to `TAB_MIN_WIDTH` before the tab area scrolls.
            .flex_shrink_1()
            .min_w(px(theme::TAB_MIN_WIDTH))
            .on_press(
                true,
                cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                    select(shell);
                    // A tab of the main window shows its terminals, also from Board or
                    // Docs.
                    if shell.host_of(window).space().is_some() {
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
            .children(status_icon)
            .children(pane_icons)
            .children((icons.more > 0).then(|| {
                div()
                    .flex_none()
                    .pb(px(theme::TEXT_LIFT * 2.0))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("+{}", icons.more))
            }))
            // The tooltip states the status in words and the time: color is never the only
            // signal.
            .tooltip(tooltip(theme, &tip, ""))
            .when(tab.pinned, |el| el.child(pin_mark()))
            .children(prefix.map(|name| {
                div()
                    .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                    .pb(px(theme::TEXT_LIFT * 2.0))
                    .truncate()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(name.to_owned())
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .max_w(px(theme::TAB_TITLE_MAX_WIDTH))
                    // See `TEXT_LIFT`: the name reads centered in the chip.
                    .pb(px(theme::TEXT_LIFT * 2.0))
                    .truncate()
                    .when(file.as_ref().is_some_and(|file| file.preview), |el| {
                        el.italic()
                    })
                    .child(format!("{}{title}", if dirty { "● " } else { "" })),
            )
            .child(
                row_action(
                    SharedString::from(format!("tab-close-{tab_id}")),
                    icon_svg(Icon::X, theme::TAB_ICON_SIZE, theme.text_muted),
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
            .pb(px(theme::TEXT_LIFT * 2.0))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.pane_focus_stripe))
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
        project_id: Ulid,
        tab: Option<&Tab>,
        now: i64,
        composer_focused: bool,
        voice_ring: Option<Hsla>,
        theme: &Theme,
        content: Rc<Cell<gpui::Size<Pixels>>>,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        if tab.and_then(|tab| tab.layout()).is_none() {
            return empty_state(
                theme,
                Some(">_"),
                Some("No panes in this tab"),
                if self.sidebar_drawn() && self.host_of(window).space().is_some() {
                    "Start an agent or a shell, or pick a session from the sidebar.".into()
                } else {
                    "Start an agent or a shell.".into()
                },
                None,
            )
            .flex_1()
            .bg(rgb(theme.pane_gap))
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
        let focused = tab.and_then(Tab::focused);
        let rects: Vec<_> = tab.map(Tab::rects).unwrap_or_default();
        let dividers = tab.map(Tab::dividers).unwrap_or_default();
        // Every pane drawn counts (a session, a file, the diff).
        let several = rects.len() > 1;
        let zoomed = tab.is_some_and(|tab| tab.zoomed().is_some());
        // The panes' 0..1 space in px, as laid out in the last frame (`content_canvas`).
        let area = f32::from(content.get().width);
        let panes: Vec<_> = rects
            .into_iter()
            .map(|(key, rect)| {
                let is_focused = focused == Some(&key);
                let width = rect.w as f32 * area;
                div()
                    .id(pane_id(&key))
                    .absolute()
                    .left(relative(rect.x as f32))
                    .top(relative(rect.y as f32))
                    .w(relative(rect.w as f32))
                    .h(relative(rect.h as f32))
                    .child(self.pane_frame(
                        project_id,
                        key,
                        several,
                        zoomed,
                        width,
                        is_focused,
                        is_focused && composer_focused,
                        voice_ring.filter(|_| is_focused),
                        now,
                        theme,
                        window,
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
            // Behind the panes, the same color as the title bar: shows only while they load.
            .bg(rgb(theme.pane_gap))
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

    /// One pane: the terminal, the chat, the file or the diff (or why it is not shown; at
    /// reading width, centered, for an agent pane with it on), a session's composer, a stripe
    /// along the full top edge (`status::pane_stripe`: needs input, the session color or the
    /// focus among several; ADR 0108). No border, no gap, square corners: panes meet at a 1 px
    /// divider line (`pane_divider`, ADR 0109). Among several the unfocused ones get a dim and a
    /// session pane a slim handle on the top edge that drags it. Transient signals float in the
    /// top-right corner (`pane_badges`). A file pane is the editor (`render_file_pane`).
    /// `zoomed`: the pane fills the tab. `voice_ring`: the listening ring over the pane;
    /// `width`: the pane's width in px in the last frame (the composer's room).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pane_frame(
        &self,
        project_id: Ulid,
        key: PaneKey,
        several: bool,
        zoomed: bool,
        width: f32,
        is_focused: bool,
        composer_focused: bool,
        voice_ring: Option<Hsla>,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let session = key.session();
        let info = session.and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        // An omp pane with TSP on shows its surface (or the starting state); in `fallback` and
        // `off` it is the terminal grid as before. It has no Chat mode (`chat::available`).
        let tsp_drawn =
            info.is_some_and(|info| self.tsp_pane_view(info) != mkrew_ui::tsp::PaneView::Terminal);
        let content = match &key {
            PaneKey::Session(id) => {
                if tsp_drawn {
                    self.render_tsp(*id, is_focused, width, now, theme, cx)
                } else if info.is_some_and(|info| self.chat_on(info)) {
                    self.render_chat(*id, is_focused, now, theme, cx)
                } else {
                    match self.panes.get(id) {
                        Some(pane) => pane.clone().into_any_element(),
                        None => {
                            let (sentence, detail) = self.pane_placeholder(*id);
                            message(theme, None, sentence, detail)
                                .size_full()
                                .into_any_element()
                        }
                    }
                }
            }
            PaneKey::File(path) => self.render_file_pane(project_id, path, window, theme, cx),
            PaneKey::Diff => self.render_diff_pane(project_id, theme, window, cx),
            PaneKey::Web(id) => {
                self.render_web_pane(project_id, *id, is_focused, theme, window, cx)
            }
        };
        // Reading width caps the terminal grid (and the Chat list); a surface lays out its own
        // columns (`page_column`), so the cap would only narrow its scroll area.
        let reading = info.is_some_and(|info| self.reading_on(info)) && !tsp_drawn;
        let color = (info.and_then(|info| info.color.as_deref()))
            .and_then(|name| theme.workspace_color(name));
        let stripe = status::pane_stripe(theme, info.map(|s| s.state), color, several, is_focused);
        let key_web_focus = match &key {
            PaneKey::Web(id) => self.browsers.get(id).map(|browser| &browser.focus),
            _ => None,
        };
        let frame = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.terminal_bg))
            // The drag handle's bar shows while the pointer is anywhere over the pane.
            .group(pane_hover_group(&key))
            // The file pane's own keys (Ctrl+S, Ctrl+F, ...), not the root's.
            .when(matches!(key, PaneKey::File(_)), |el| {
                el.key_context(EDITOR_CONTEXT)
            })
            .when(matches!(key, PaneKey::Web(_)), |el| {
                el.key_context(BROWSER_CONTEXT)
            })
            .when_some(key_web_focus, |el, focus| el.track_focus(focus));
        let frame = self.pane_drop_target(frame, project_id, &key, cx);
        frame
            .on_mouse_down(MouseButton::Left, {
                let key = key.clone();
                cx.listener(move |shell, _, window, cx| {
                    shell.focus_pane_key(project_id, &key);
                    shell.focus_pane(window, cx);
                    cx.notify();
                })
            })
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
            .children(session.and_then(|session| {
                self.composers.shown(session).map(|draft| {
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
                })
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
            .children(self.pane_drop_overlay(&key, theme, cx))
            .when_some(stripe, |el, stripe| {
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(theme::PANE_ACCENT_WIDTH))
                        .bg(rgb(stripe)),
                )
            })
            .children(info.and_then(|info| self.pane_badges(info, zoomed, theme, cx)))
            .child(self.pane_edge(project_id, &key, several, theme, cx))
            .when_some(voice_ring, |el, color| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
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

    /// Reading width on / off for `session`'s pane (the title bar toggle, and the palette).
    pub(crate) fn toggle_reading_width(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        let on = (self.sidebar.sessions().iter())
            .find(|s| s.id == session)
            .is_some_and(|info| self.reading_on(info));
        self.reading_width.insert(session, !on);
        cx.notify();
    }

    /// The tab's rename field when the rename in progress is of the tab or of one of its panes
    /// (F2 or a double-click on a tab that keeps its default name renames the focused pane,
    /// ADR 0031): either way the field takes the tab chip's place.
    pub(crate) fn tab_renaming(&self, tab: &Tab) -> Option<&Renaming> {
        (self.renaming.as_ref()).filter(|r| match &r.target {
            RenameTarget::Tab(id) => *id == tab.id,
            RenameTarget::Session(session) => tab.contains(&PaneKey::Session(*session)),
            RenameTarget::NewEntry(_) => false,
        })
    }

    /// The transient signals of `info`'s pane as a stack of floating badges in its top-right
    /// corner, each drawn only while it applies: the conflict warning, `⧗ N queued` sends,
    /// Copy mode with its hint and, while `zoomed`, "Pane zoomed" with "Show all panes".
    /// `None` in the normal case. The text badges have no mouse handling, so clicks pass through
    /// to the terminal; only the conflict Dismiss and "Show all panes" buttons take clicks.
    pub(crate) fn pane_badges(
        &self,
        info: &Session,
        zoomed: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let session = info.id;
        let mut badges: Vec<gpui::AnyElement> = Vec::new();
        let paths = self.conflicts.for_session(session);
        if !paths.is_empty() {
            badges.push(
                conflict_badge(
                    format!("pane-{session}"),
                    &paths,
                    theme,
                    move |shell| shell.conflicts.dismiss_session(session),
                    cx,
                )
                .flex_none()
                .max_w(px(theme::PANE_BADGE_MAX_WIDTH))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border))
                .bg(rgb(theme.surface_raised))
                .into_any_element(),
            );
        }
        let queued = self.composers.queue(session).len();
        if queued > 0 {
            badges.push(
                badge_frame(theme)
                    .child(format!("⧗ {queued} queued"))
                    .into_any_element(),
            );
        }
        if self
            .panes
            .get(&session)
            .is_some_and(|pane| pane.read(cx).in_copy_mode())
        {
            badges.push(
                badge_frame(theme)
                    .bg(rgb(theme.accent_bg))
                    .child(
                        div()
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.accent_text))
                            .child("Copy mode"),
                    )
                    .child("v select · y copy · Esc exit")
                    .into_any_element(),
            );
        }
        if zoomed {
            badges.push(
                badge_frame(theme)
                    .child("⤢ Pane zoomed")
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
                    .into_any_element(),
            );
        }
        (!badges.is_empty()).then(|| {
            div()
                .absolute()
                .top(px(space::S))
                .right(px(space::S))
                .flex()
                .flex_col()
                .items_end()
                .gap(px(space::XS))
                .font_family(theme.ui_font_family.clone())
                .children(badges)
                .into_any_element()
        })
    }

    /// The top edge of a pane: for a session it records the anchor of the pane's keyboard menu
    /// and, among `several` panes, it holds the drag handle at its centre (ADR 0110): a slim bar
    /// while the pointer is over the pane (hover group `pane_hover_group`), a chip with a six-dot
    /// grip while it is over the handle, and dragging the handle moves the pane onto another. A
    /// file, diff or browser pane has the handle but no anchor.
    /// Only the handle has a hitbox; the rest of the edge passes the mouse to the pane. The
    /// handle blocks the mouse (not the wheel): gpui hit-tests every hitbox until one blocks, so
    /// without it the terminal under it would also get the mouse down and start a selection.
    /// Drops are delivered to the hovered hitbox only, so the handle carries the pane frame's
    /// drop set (`pane_drops`); the frame's drag-move still sets the half that lights up.
    /// A lone pane has nothing to rearrange: its edge is only the Shift+F10 menu anchor of a
    /// session, with no hitbox and no mouse handling.
    fn pane_edge(
        &self,
        project_id: Ulid,
        key: &PaneKey,
        several: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let edge = div()
            .absolute()
            // Under the top stripe, so the handle's chip never hides it.
            .top(px(theme::PANE_ACCENT_WIDTH))
            .left_0()
            .right_0()
            .h(px(theme::PANE_GRIP_HEIGHT))
            .flex()
            .justify_center()
            .children(
                key.session()
                    .map(|session| record_bounds(Anchor::Pane(session), cx)),
            );
        if !several {
            return edge.into_any_element();
        }
        let group = grip_id(key);
        let handle = div()
            .relative()
            .w(px(theme::PANE_GRIP_WIDTH))
            .h_full()
            .flex()
            .justify_center()
            .group(group.clone());
        let handle = Self::pane_drops(handle, project_id, key, cx);
        let bar = div()
            .mt(px(space::XS))
            .w(px(theme::PANE_GRIP_BAR_WIDTH))
            .h(px(theme::PANE_GRIP_BAR_HEIGHT))
            .rounded_full()
            .bg(rgb(theme.text_muted))
            .opacity(opacity::HIDDEN)
            .group_hover(pane_hover_group(key), |style| {
                style.opacity(opacity::DIMMED)
            });
        // Over the bar once the pointer reaches the handle (opaque, so the bar does not show).
        let chip = div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .opacity(opacity::HIDDEN)
            .group_hover(group, |style| style.opacity(opacity::FULL))
            .child(icon_svg(
                Icon::GripHorizontal,
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ));
        let handle = self
            .draggable_pane(handle, project_id, key, theme)
            .block_mouse_except_scroll()
            .cursor(CursorStyle::OpenHand)
            .child(bar)
            .child(chip);
        edge.child(handle).into_any_element()
    }

    /// The title bar toggles of the focused agent pane, shown only while one has the focus:
    /// Terminal | Chat (only for a pane that can chat; accent while Chat shows), Reading
    /// width (accent while on) and, with `[features] share`, Share (accent while following;
    /// its menu has Follow in Zed).
    pub(crate) fn pane_toggles(
        &self,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let Some(info) = self
            .focused_agent(window)
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id))
        else {
            return Vec::new();
        };
        let session = info.id;
        let mut toggles = Vec::new();
        if self.chat_available(info) {
            let chat = self.chat_on(info);
            toggles.push(
                icon_button_in(
                    SharedString::from(format!("pane-chat-{session}")),
                    Icon::MessageSquare,
                    if chat { theme.accent } else { theme.text_muted },
                    theme,
                )
                .tooltip(tooltip(
                    theme,
                    if chat {
                        "Chat view: on. Click to show the terminal"
                    } else {
                        "Terminal view. Click to show the conversation as a chat"
                    },
                    self.keymap.label(Command::ToggleChat),
                ))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.toggle_chat(session, window, cx);
                }))
                .into_any_element(),
            );
        }
        // A surface (or its starting state) has no terminal to cap; in `fallback` and `off` the
        // grid is back and so is the toggle. Terminal | Chat is already gone through
        // `chat_available`; Share stays.
        if self.tsp_pane_view(info) == mkrew_ui::tsp::PaneView::Terminal {
            let reading = self.reading_on(info);
            toggles.push(
                icon_button_in(
                    SharedString::from(format!("pane-reading-{session}")),
                    Icon::AlignCenter,
                    if reading {
                        theme.accent
                    } else {
                        theme.text_muted
                    },
                    theme,
                )
                .tooltip(tooltip(
                    theme,
                    if reading {
                        "Reading width: on (the agent wraps at about 130 columns). Click for full width"
                    } else {
                        "Reading width: off (full width). Click to wrap at about 130 columns"
                    },
                    "",
                ))
                .on_click(
                    cx.listener(move |shell, _, _, cx| shell.toggle_reading_width(session, cx)),
                )
                .into_any_element(),
            );
        }
        if self.features.share {
            let following = self.follow.is_on(session);
            toggles.push(
                icon_button_in(
                    SharedString::from(format!("pane-share-{session}")),
                    Icon::Share2,
                    if following {
                        theme.accent
                    } else {
                        theme.text_muted
                    },
                    theme,
                )
                .tooltip(tooltip(
                    theme,
                    if following {
                        "Share: following this agent's edits in Zed"
                    } else {
                        "Share this agent's work"
                    },
                    "",
                ))
                .on_click(
                    cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                        shell.open_share_menu(session, event.position(), window, cx);
                    }),
                )
                .into_any_element(),
            );
        }
        toggles
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DropTarget {
    /// A pane: a terminal types the paths, a file or diff pane opens the files.
    Pane(PaneKey),
    /// The main window's tab strip: the files open as new tabs.
    Strip,
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
                        match &target {
                            DropTarget::Pane(PaneKey::Session(session)) => {
                                shell.drop_paths(*session, &paths, window, cx);
                            }
                            // The workspace of the window the pointer was released in.
                            DropTarget::Pane(_) => {
                                if let Some((project_id, _)) = shell.tab_scope(window) {
                                    shell.drop_os_files(project_id, &paths, window, cx);
                                }
                            }
                            DropTarget::Strip => {
                                if let Some(project_id) = shell.workspace_in(window) {
                                    shell.drop_os_files(project_id, &paths, window, cx);
                                }
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

fn divider_cursor(axis: Axis) -> CursorStyle {
    match axis {
        Axis::Vertical => CursorStyle::ResizeLeftRight,
        Axis::Horizontal => CursorStyle::ResizeUpDown,
    }
}

/// A split's divider: a 1 px `divider` line where its two children meet (the panes touch, no
/// gap; ADR 0109) and the grab strip centred on it, as long as the split. Dragging it is handled
/// by the panes area (`render_tab`), which knows the size.
fn pane_divider(divider: Divider, theme: &Theme, cx: &mut Context<'_, Shell>) -> gpui::AnyElement {
    let area = divider.area;
    let at = |start: f64, len: f64| relative((start + len * divider.ratio) as f32);
    let grip = px(theme::GRIP_WIDTH);
    let centre = px(-theme::GRIP_WIDTH / 2.0);
    let thin = px(theme::BORDER_WIDTH);
    let half_thin = px(-theme::BORDER_WIDTH / 2.0);
    // A zero-thick line along the divider; the rule and the strip hang half on each side of it.
    let (line, rule, strip) = match divider.axis {
        Axis::Vertical => (
            div()
                .left(at(area.x, area.w))
                .top(relative(area.y as f32))
                .h(relative(area.h as f32)),
            div().top_0().bottom_0().left(half_thin).w(thin),
            div().top_0().bottom_0().left(centre).w(grip),
        ),
        Axis::Horizontal => (
            div()
                .top(at(area.y, area.h))
                .left(relative(area.x as f32))
                .w(relative(area.w as f32)),
            div().left_0().right_0().top(half_thin).h(thin),
            div().left_0().right_0().top(centre).h(grip),
        ),
    };
    line.absolute()
        .child(rule.absolute().bg(rgb(theme.divider)))
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
/// off from here (`HostUi::content`) and for the panes' widths (`render_tab`). A new size
/// draws the frame again, so a pane never keeps the width of an older size.
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
