//! What a detached window shows (ADR 0096): title bar, its tab strip, the active tab.

use crate::*;
use mkrew_ui::window_place::Strip;

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
        if self.palette.is_some()
            && self.palette_host == host
            && !window.has_active_prompt()
            && !self.dialog_focus.contains_focused(window, cx)
        {
            window.focus(&self.dialog_focus, cx);
        }
        let now = status::now_ms();
        window.set_rem_size(px(cx.global::<theme::Fonts>().ui_size));
        cx.global::<MenuAnchors>()
            .clear(window.window_handle().window_id());
        let scope = self.tab_scope(window);
        let title = scope
            .and_then(|(project, tab_id)| {
                let tab = self.tabs.get(&project)?.tab(tab_id)?;
                let name = (self.sidebar.projects().iter())
                    .find(|p| p.id == project)?
                    .name
                    .clone();
                let label = session_label(tab, self.sidebar.sessions()).unwrap_or(&tab.title);
                Some(format!("{label} - {name}"))
            })
            .unwrap_or_else(|| "Mkrew".to_owned());
        window.set_window_title(&title);
        let content = self.hosts.entry(host).or_default().content.clone();
        let body = match scope {
            Some((project, tab_id)) => {
                let tab = self.tabs.get(&project).and_then(|tabs| tabs.tab(tab_id));
                self.render_tab(
                    tab,
                    now,
                    self.composer_focus.is_focused(window),
                    None,
                    &theme,
                    content,
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
            .child(self.render_detached_title_bar(key, &title, &theme, window, cx))
            .children(self.render_reconnect_banner(&theme, cx))
            .child(self.render_detached_strip(key, &theme, cx))
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
        if self.activation.first() == Some(&host) {
            overlays.extend(self.render_toasts(theme, cx));
        }
        overlays
    }

    /// The title bar of a detached window: logo, the shown tab's title, a drag area, "Move all
    /// back" and, on Windows, minimize, maximize and close (as the main bar, without its
    /// workspace controls).
    fn render_detached_title_bar(
        &self,
        key: WindowKey,
        title: &str,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        // An open dialog keeps the mouse too: the bar's button is inert, only the drag area
        // and the window controls stay live.
        let modal = self.modal_open() || self.reconnecting();
        let controls = DRAWN_CONTROLS && !window.is_fullscreen();
        let mut bar = div()
            .flex_none()
            .h(theme::rems_px(theme::TITLE_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::S))
            .pl(px(if cfg!(target_os = "macos") {
                theme::MAC_TRAFFIC_LIGHTS_WIDTH
            } else {
                space::M
            }))
            .bg(rgb(theme.chrome))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(app_logo(theme))
            // Not a control: the title is part of the drag area.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .window_control_area(WindowControlArea::Drag)
                    .when(cfg!(target_os = "macos"), |el| {
                        el.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, event: &MouseDownEvent, window, _| {
                                if event.click_count == 2 {
                                    window.titlebar_double_click();
                                } else {
                                    window.start_window_move();
                                }
                            }),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(font_size::BODY)
                            .text_color(rgb(theme.text_muted))
                            .child(title.to_owned()),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .when(!controls, |el| el.pr(px(space::M)))
                    .child(
                        icon_button("move-all-back", Icon::Minimize2, theme)
                            .tooltip(tooltip(theme, "Move all tabs back to the main window", ""))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.move_all_back(key, window, cx);
                            })),
                    )
                    .when(modal, |el| el.relative().child(inert_layer())),
            );
        if controls {
            // No click handlers: Windows hit-tests these areas and runs the action itself.
            let max_icon = if window.is_maximized() {
                Icon::Copy
            } else {
                Icon::Square
            };
            bar = bar.child(
                div()
                    .flex_none()
                    .h_full()
                    .flex()
                    .items_center()
                    // A thin rule sets the window's own controls apart from the app's.
                    .child(
                        div()
                            .w(px(theme::BORDER_WIDTH))
                            .h(theme::rems_px(theme::TITLE_RULE_HEIGHT))
                            .mr(px(space::XS))
                            .bg(rgb(theme.divider)),
                    )
                    .child(window_control(
                        "win-min",
                        Icon::Minus,
                        WindowControlArea::Min,
                        theme,
                    ))
                    .child(window_control(
                        "win-max",
                        max_icon,
                        WindowControlArea::Max,
                        theme,
                    ))
                    .child(window_control(
                        "win-close",
                        Icon::X,
                        WindowControlArea::Close,
                        theme,
                    )),
            );
        }
        bar
    }

    /// The strip of a detached window: a chip per tab (its workspace in front), the layout
    /// button and the ⋯ menu. No `+`: new tabs are made in the main window. The strip and chip
    /// bounds are kept in `HostUi::strip` for drops from other windows.
    fn render_detached_strip(
        &self,
        key: WindowKey,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let tabs = self.detached.tabs_of(key);
        let active = self.detached.active(key);
        let active_id = active.map(|DockTab::Terminal { tab_id, .. }| tab_id);
        let strip = (self.hosts.get(&HostId::Detached(key)))
            .map(|ui| ui.strip.clone())
            .unwrap_or_default();
        let chips: Rc<RefCell<Vec<Bounds<Pixels>>>> =
            Rc::new(RefCell::new(vec![Bounds::default(); tabs.len()]));
        let sessions = self.sidebar.sessions();
        let chip_elements: Vec<gpui::AnyElement> = tabs
            .iter()
            .enumerate()
            .filter_map(|(index, dock)| {
                let DockTab::Terminal { project, tab_id } = *dock;
                let tab = self.tabs.get(&project)?.tab(tab_id)?;
                // F2 on a single pane: its rename field takes the tab's place (no header).
                if let Some(renaming) = self.renaming.as_ref().filter(|r| {
                    r.target == RenameTarget::Tab(tab_id)
                        || tab.single().map(RenameTarget::Session).as_ref() == Some(&r.target)
                }) {
                    return Some(self.render_rename(&renaming.draft, theme, cx));
                }
                let workspace = (self.sidebar.projects().iter())
                    .find(|p| p.id == project)
                    .map(|p| p.name.as_str());
                let item = DraggedItem {
                    list: ReorderList::Detached(key),
                    index,
                    title: session_label(tab, sessions)
                        .unwrap_or(&tab.title)
                        .to_owned()
                        .into(),
                };
                let record = chips.clone();
                let chip = self
                    .tab_chip(
                        project,
                        tab,
                        active == Some(*dock),
                        workspace,
                        move |shell: &mut Shell| {
                            shell.detached.select(key, index);
                        },
                        theme,
                        cx,
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                if let Some(slot) = record.borrow_mut().get_mut(index) {
                                    *slot = bounds;
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    );
                Some(self.reorderable(chip.into_any_element(), item, true, space::XS, theme, cx))
            })
            .collect();
        div()
            .flex_none()
            .relative()
            .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .bg(rgb(theme.surface))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .on_drop(cx.listener(|shell, item: &DraggedItem, window, cx| {
                shell.drop_reordered(item, window, cx);
            }))
            .children(chip_elements)
            // Pushes the notices and the layout / ⋯ group to the right.
            .child(div().flex_1())
            .children(
                self.tab_error
                    .clone()
                    .map(|error| notice(theme.text, format!("✕ {error}"))),
            )
            .children(
                self.tab_notice
                    .clone()
                    .map(|text| notice(theme.text_muted, text)),
            )
            .child(
                icon_button("tab-split", Icon::Columns2, theme)
                    .tooltip(tooltip(
                        theme,
                        "Split or pick a layout",
                        self.keymap.label(Command::SplitRight),
                    ))
                    .on_click(cx.listener(|shell, event: &gpui::ClickEvent, window, cx| {
                        shell.open_layout_menu(event.position(), window, cx);
                    })),
            )
            .when_some(active_id, |bar, tab_id| {
                bar.child(
                    icon_button("tab-menu-button", Icon::Ellipsis, theme)
                        .tooltip(tooltip(theme, "Tab and pane menu", ""))
                        .on_click(cx.listener(
                            move |shell, event: &gpui::ClickEvent, window, cx| {
                                shell.open_tab_menu(tab_id, event.position(), window, cx);
                            },
                        )),
                )
            })
            // Last, so it reads the chips' bounds painted before it.
            .child(
                canvas(
                    move |bounds, _, _| {
                        *strip.borrow_mut() = Some(Strip {
                            bounds,
                            chips: chips.borrow().clone(),
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }
}
