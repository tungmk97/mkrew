//! The widget bar (ADR 0102): its switch, its hide toggles and the `limits.get` read. The
//! drawing and the right-click menu follow in the same file.

use crate::*;
use mkrew_ui::widget_bar::{self as model, Click, Level, Side, Widget, WidgetId};

impl Shell {
    /// `[ui] widget_bar` on or off, saved. A settings file that does not parse keeps the
    /// choice for this run; the palette says why (§20.0) and a toast says it was not saved.
    pub(crate) fn set_widget_bar(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        if self.widget_bar == on {
            return;
        }
        self.widget_bar = on;
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "widget_bar", &on.to_string())
        {
            self.settings_write_failed("the widget bar", error, cx);
        }
        cx.notify();
    }

    /// Ctrl+Alt+Shift+J, the palette's "View: Show/hide widget bar".
    pub(crate) fn toggle_widget_bar(&mut self, cx: &mut Context<'_, Self>) {
        self.set_widget_bar(!self.widget_bar, cx);
    }

    /// The limits the daemon holds, for a window that connects after they were reported;
    /// events keep them live afterwards.
    pub(crate) fn load_limits(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let listed = connection.call::<Vec<mkrew_protocol::AgentLimits>>(Request::LimitsGet);
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update(cx, |shell, cx| match listed {
                Ok(listed) => {
                    if shell.widget_data.load_limits(listed) && shell.widget_bar {
                        cx.notify();
                    }
                }
                Err(error) => tracing::warn!("limits.get failed: {error:#}"),
            });
        })
        .detach();
    }
}

/// Seconds east of UTC at Unix time `at`, from the OS time zone (0 when it cannot say).
fn local_offset(at: i64) -> i32 {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(at, 0)
        .single()
        .map_or(0, |time| time.offset().local_minus_utc())
}

fn level_color(level: Level, theme: &Theme) -> u32 {
    match level {
        Level::Muted => theme.text_muted,
        Level::Warn => theme.status_needs_input,
        Level::Danger => theme.danger_text,
    }
}

fn icon_of(id: WidgetId) -> Icon {
    match id {
        WidgetId::Folder => Icon::Folder,
        WidgetId::Git => Icon::GitBranch,
        WidgetId::Limits => Icon::Bot,
        WidgetId::Machine => Icon::Server,
        WidgetId::Version => Icon::Info,
    }
}

impl Shell {
    /// The bar below the sidebar and the main area, or nothing while `[ui] widget_bar` is off.
    /// Detached windows never call this. The strip stays when every widget is hidden or has
    /// nothing to show, so a right-click can bring them back.
    pub(crate) fn render_widget_bar(
        &self,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.widget_bar {
            return None;
        }
        let project =
            (self.workspace).and_then(|id| self.sidebar.projects().iter().find(|p| p.id == id));
        let input = model::Input {
            workspace: project.map(|p| (p.name.as_str(), p.path.as_str())),
            git: self.git_summary(),
            data: &self.widget_data,
            machine_name: self.machine_name().unwrap_or("This computer"),
            version: env!("CARGO_PKG_VERSION"),
            hidden: &self.widgets_hidden,
            now_secs: status::now_ms() / 1000,
        };
        let em = cx.global::<theme::Fonts>().ui_size;
        let width = f32::from(window.viewport_size().width);
        let shown = model::fit(model::widget_list(&input, local_offset), width, em);
        let (mut left, mut right) = (Vec::new(), Vec::new());
        for widget in shown {
            let side = widget.id.side();
            let element = self.render_widget(widget, theme, cx);
            match side {
                Side::Left => left.push(element),
                Side::Right => right.push(element),
            }
        }
        let menu = (self.widget_menu.as_ref())
            .filter(|_| !self.modal_open())
            .map(|menu| self.render_widget_menu(menu, theme, cx));
        let group = || {
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(theme::WIDGET_BAR_GAP))
        };
        Some(
            div()
                .id("widget-bar")
                .flex_none()
                .w_full()
                .h(theme::rems_px(theme::WIDGET_BAR_HEIGHT))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(theme::WIDGET_BAR_GAP))
                .px(px(theme::WIDGET_BAR_PADDING))
                .bg(rgb(theme.chrome))
                .border_t(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|shell, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        shell.open_widget_menu(event.position, window, cx);
                    }),
                )
                .child(group().children(left))
                .child(group().children(right))
                .children(menu)
                .when(self.modal_open() || self.reconnecting(), |el| {
                    el.relative().child(inert_layer())
                })
                .into_any_element(),
        )
    }

    /// One widget: an icon and its label in a `pressable` element (hover, press and focus from
    /// `widgets.rs`), with its tooltip. The machine widget has no click and no pointer.
    fn render_widget(
        &self,
        widget: Widget,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (id, click) = (widget.id, widget.click);
        let capped = matches!(id, WidgetId::Folder | WidgetId::Git);
        let parts = widget.parts.into_iter().map(|part| {
            // The cap and the ellipsis sit on the div that holds the text itself.
            div()
                .when(capped, |el| {
                    el.min_w_0()
                        .max_w(theme::rems_px(theme::WIDGET_BAR_LABEL_MAX_WIDTH))
                        .truncate()
                })
                .when(!capped, |el| el.flex_none())
                .when(part.level != Level::Muted, |el| {
                    el.text_color(rgb(level_color(part.level, theme)))
                })
                .child(part.text)
        });
        let element = div()
            .id(SharedString::from(format!("widget-{}", id.id())))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .whitespace_nowrap()
            .tooltip(tooltip(theme, &widget.tooltip, ""))
            .child(icon_svg(
                icon_of(id),
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .children(parts),
            );
        if click == Click::None {
            return element.into_any_element();
        }
        element
            .pressable(Look::Text, true, theme)
            .on_press(
                true,
                cx.listener(move |shell, _, window, cx| shell.widget_clicked(click, window, cx)),
            )
            .into_any_element()
    }

    /// What a click on a widget does (spec section 4).
    pub(crate) fn widget_clicked(
        &mut self,
        click: Click,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match click {
            Click::None => {}
            Click::OpenFolder => {
                if let Some(project_id) = self.workspace {
                    self.open_workspace_in_explorer(project_id, cx);
                }
            }
            Click::Git => {
                let Some(project_id) = self.workspace else {
                    return;
                };
                if self.git_shown() {
                    self.git_key(git_view::Key::NextWrapping, window, cx);
                } else {
                    self.open_git(project_id, window, cx);
                }
            }
            Click::OpenLimits => {
                self.open_settings_dialog(window, cx);
                if let Some(dialog) = self.settings_dialog.as_mut() {
                    let _ = dialog.set_tab(settings_dialog::INTEGRATIONS_TAB);
                }
            }
            Click::CheckUpdate => self.check_updates(true, window, cx),
        }
    }
}

impl Shell {
    /// Right-click anywhere on the bar.
    pub(crate) fn open_widget_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.widget_menu = Some(WidgetMenu { position });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    /// Hides or shows one widget and saves `[ui] widgets_hidden`; ids this build does not know
    /// stay in the list. The menu stays open (a multi-select row).
    pub(crate) fn toggle_widget(&mut self, id: WidgetId, cx: &mut Context<'_, Self>) {
        self.widgets_hidden = model::toggled(&self.widgets_hidden, id);
        if let Err(error) =
            mkrew_core::settings::write_widgets_hidden(&self.settings_file, &self.widgets_hidden)
        {
            self.settings_write_failed("the widget bar", error, cx);
        }
        cx.notify();
    }

    /// One checkable row per widget, then "Hide widget bar".
    pub(crate) fn widget_menu_rows(&self) -> Rows {
        let mut rows: Rows = WidgetId::ALL
            .into_iter()
            .map(|id| {
                MenuItem::new(
                    id.title(),
                    act(move |shell, _, cx| shell.toggle_widget(id, cx)),
                )
                .checked(!model::is_hidden(&self.widgets_hidden, id))
                .toggle()
                .row()
            })
            .collect();
        rows.push(MenuRow::Separator);
        rows.push(
            MenuItem::new(
                "Hide widget bar",
                act(|shell, _, cx| shell.set_widget_bar(false, cx)),
            )
            .keys(self.keymap.label(Command::ToggleWidgetBar))
            .row(),
        );
        rows
    }

    pub(crate) fn render_widget_menu(
        &self,
        menu: &WidgetMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "widget-menu",
            self.widget_menu_rows(),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }
}
