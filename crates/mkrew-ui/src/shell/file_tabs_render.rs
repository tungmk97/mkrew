//! The Files view's tab strip and tab menu (SPEC §12A, ADR 0080).

use crate::*;

impl Shell {
    pub(crate) fn open_file_tab_menu(
        &mut self,
        project_id: Ulid,
        path: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.file_tab_menu = Some(FileTabMenu {
            project_id,
            path,
            position,
        });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    /// One tab per opened file: pin, name (its folder too when two tabs share a name), `●`
    /// for unsaved text, ✕ on the active tab and on hover; italic while transient.
    pub(crate) fn render_file_tabs(
        &self,
        project_id: Ulid,
        view: &FilesView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tabs = view.tabs();
        let active = view.active_index();
        let pinned_count = tabs.iter().filter(|tab| tab.pinned).count();
        let hover = theme.hover;
        let close_keys = self.keymap.label(Command::CloseFileTab);
        div()
            .id("file-tabs")
            .flex_none()
            .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .overflow_x_scroll()
            .track_scroll(&self.file_tabs_scroll)
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .on_drop(cx.listener(|shell, item: &DraggedItem, window, cx| {
                shell.drop_reordered(item, window, cx);
            }))
            .children(tabs.iter().enumerate().map(|(at, tab)| {
                let is_active = active == Some(at);
                let dirty = tab.is_dirty();
                let name = tab.name();
                let twin = tabs.iter().filter(|other| other.name() == name).count() > 1;
                let folder = (tab.path.rsplit_once('/'))
                    .filter(|_| twin)
                    .map(|(dir, _)| dir.to_owned());
                let group = SharedString::from(format!("file-tab-{}", tab.path));
                let item = DraggedItem {
                    list: ReorderList::FileTabs(project_id, tab.pinned),
                    index: if tab.pinned { at } else { at - pinned_count },
                    title: name.to_owned().into(),
                };
                let (click_path, middle_path, menu_path, close_path) = (
                    tab.path.clone(),
                    tab.path.clone(),
                    tab.path.clone(),
                    tab.path.clone(),
                );
                let element = div()
                    .id(group.clone())
                    .group(group.clone())
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .h(theme::rems_px(theme::CONTROL_HEIGHT))
                    .pl(px(space::M))
                    .pr(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::transparent_black())
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .text_size(font_size::BODY)
                    .when(is_active, |el| {
                        el.bg(rgb(theme.card))
                            .border_color(rgb(theme.selected))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!is_active, |el| {
                        el.text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .when(tab.transient && !dirty, |el| el.italic())
                    .tooltip(tooltip(theme, &tab.path, ""))
                    .on_click(
                        cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                            let how = if event.click_count() == 2 {
                                Open::Kept
                            } else {
                                Open::Transient
                            };
                            shell.open_file_tab(project_id, &click_path, how, window, cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |shell, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            shell.close_file_tabs(project_id, &middle_path, Close::One, window, cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            shell.open_file_tab_menu(
                                project_id,
                                menu_path.clone(),
                                event.position,
                                window,
                                cx,
                            );
                        }),
                    )
                    .when(tab.pinned, |el| el.child(pin_mark()))
                    .child(icon_svg(
                        Icon::File,
                        theme::ICON_SIZE_SMALL,
                        theme.text_subtle,
                    ))
                    .child(
                        div()
                            .max_w(px(theme::TAB_TITLE_MAX_WIDTH))
                            .truncate()
                            .child(format!("{}{name}", if dirty { "● " } else { "" })),
                    )
                    .children(folder.map(|folder| {
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_subtle))
                            .child(folder)
                    }))
                    .child(
                        row_action(
                            SharedString::from(format!("file-tab-close-{}", tab.path)),
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
                                close_keys.clone()
                            } else {
                                String::new()
                            },
                        ))
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                // Never activate the tab being closed.
                                cx.stop_propagation();
                                shell.close_file_tabs(
                                    project_id,
                                    &close_path,
                                    Close::One,
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .into_any_element();
                self.reorderable(element, item, true, space::XS, theme, cx)
            }))
    }

    /// The tab menu: Close, Close others, Close to the right, Close all, Pin / Unpin, Copy path.
    pub(crate) fn file_tab_menu_rows(&self, menu: &FileTabMenu) -> Rows {
        let project_id = menu.project_id;
        let view = self.files.get(&project_id);
        let target = view.and_then(|view| view.worktree_task_id);
        let pinned = (view.and_then(|view| view.tab_for(&menu.path))).is_some_and(|tab| tab.pinned);
        let close = |label: &'static str, which: Close, keys: String| {
            let path = menu.path.clone();
            let enabled =
                view.is_some_and(|view| !view.close_targets(&menu.path, which).is_empty());
            MenuItem::new(
                label,
                act(move |shell, window, cx| {
                    shell.close_file_tabs(project_id, &path, which, window, cx);
                }),
            )
            .keys(keys)
            .enabled(enabled)
            .row()
        };
        let (pin_path, copy_path) = (menu.path.clone(), menu.path.clone());
        vec![
            close(
                "Close",
                Close::One,
                self.keymap.label(Command::CloseFileTab),
            ),
            close("Close others", Close::Others, String::new()),
            close("Close to the right", Close::Right, String::new()),
            close("Close all", Close::All, String::new()),
            MenuRow::Separator,
            MenuItem::new(
                if pinned { "Unpin" } else { "Pin" },
                act(move |shell, _, cx| shell.pin_file_tab(project_id, &pin_path, !pinned, cx)),
            )
            .row(),
            MenuItem::new(
                "Copy path",
                act(move |shell, _, cx| {
                    shell.copy_file_path(project_id, target, &copy_path, cx);
                }),
            )
            .row(),
        ]
    }

    pub(crate) fn render_file_tab_menu(
        &self,
        menu: &FileTabMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "file-tab-menu",
            self.file_tab_menu_rows(menu),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }
}
