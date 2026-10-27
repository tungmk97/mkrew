//! The one renderer of every popover menu, the key handler that goes with it and the focus
//! rules (ADR 0081). Each menu builds `Rows`; a click and a key reach the same `action`.

use crate::*;

/// What a row does; the number is the swatch picked (0 for an item).
pub(crate) type MenuAction = Rc<dyn Fn(&mut Shell, usize, &mut Window, &mut Context<'_, Shell>)>;
pub(crate) type Rows = Vec<MenuRow<MenuAction>>;

/// An action that ignores the swatch number (every item).
pub(crate) fn act(
    f: impl Fn(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> MenuAction {
    Rc::new(
        move |shell: &mut Shell, _: usize, window: &mut Window, cx: &mut Context<'_, Shell>| {
            f(shell, window, cx);
        },
    )
}

/// The action of a color row: it gets the picked color (`None` = no color).
pub(crate) fn act_swatch(
    f: impl Fn(&mut Shell, Option<&'static str>, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> MenuAction {
    Rc::new(
        move |shell: &mut Shell, ix: usize, window: &mut Window, cx: &mut Context<'_, Shell>| {
            f(shell, menu_model::swatch_color(ix), window, cx);
        },
    )
}

/// Where a menu is drawn.
pub(crate) enum Place {
    /// At a window point (a right-click, or the bottom-left of what owns the menu).
    At(Point<Pixels>),
    /// Under the button that holds it (drawn in its place in the tree).
    Dropdown,
}

/// The parts of the `Shell` a menu draws with (the board's free fns get it from `CardContext`).
#[derive(Clone, Copy)]
pub(crate) struct MenuState<'a> {
    pub(crate) focus: &'a FocusHandle,
    pub(crate) selected: Option<usize>,
    pub(crate) swatch: usize,
}

/// The ✓ column, the lead, the label (and its second line), the detail and the shortcut.
fn item_row(
    state: MenuState<'_>,
    id: &'static str,
    ix: usize,
    item: MenuItem<MenuAction>,
    slot: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let row_id = SharedString::from(format!("{id}-{ix}"));
    let enabled = item.enabled;
    let selected = state.selected == Some(ix);
    let color = match item.tone {
        Tone::Normal => theme.text,
        // The destructive color the Run and workspace menus already use.
        Tone::Danger => theme.status_exited,
        Tone::Accent => theme.accent_text,
    };
    let checked = item.check == Some(true);
    let lead = lead_element(&item.lead, id, ix, color, theme);
    let (action, toggle) = (item.action.clone(), item.toggle);
    // A row that cannot act: the same box without the hover, faded, no click.
    let frame = if enabled {
        menu_item(row_id, theme)
    } else {
        div()
            .id(row_id)
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text_muted))
            .opacity(opacity::DISABLED)
    };
    frame
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .when(slot, |el| {
            el.child(
                div()
                    .flex_none()
                    .w(px(space::M))
                    .text_color(rgb(theme.accent_text))
                    .child(if checked { "✓" } else { "" }),
            )
        })
        .children(lead)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_color(rgb(color))
                        .when(checked, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(item.label),
                )
                .children(item.sub.map(|sub| meta_line(theme, sub).truncate())),
        )
        .children(
            item.detail
                .map(|detail| div().flex_none().text_size(font_size::META).child(detail)),
        )
        .children(
            item.keys
                .map(|keys| div().flex_none().text_size(font_size::META).child(keys)),
        )
        .when_some(item.tip, |el, tip| el.tooltip(tooltip(theme, &tip, "")))
        .when(enabled, |el| {
            el.on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on what lies under the menu (a card, a pane).
                cx.stop_propagation();
                shell.run_menu_item(&action, 0, toggle, window, cx);
            }))
        })
        .into_any_element()
}

fn lead_element(
    lead: &Lead,
    id: &str,
    ix: usize,
    color: u32,
    theme: &Theme,
) -> Option<gpui::AnyElement> {
    Some(match lead {
        Lead::None => return None,
        Lead::Icon(glyph) => icon_svg(*glyph, theme::ICON_SIZE_SMALL, color).into_any_element(),
        Lead::Agent(badge) => agent_chip(badge.clone(), theme.text, theme).into_any_element(),
        Lead::Avatar { name, color } => workspace_avatar(
            name,
            color
                .as_deref()
                .and_then(|name| theme.workspace_color(name)),
            theme,
        )
        .into_any_element(),
        Lead::Layout(preset) => layout_thumb(*preset, theme).into_any_element(),
        Lead::Dot(dot) => {
            let (color, hollow) = match dot {
                Dot::Running => (theme.status_done, false),
                Dot::Exited => (theme.status_exited, false),
                Dot::Stopped => (theme.status_idle, true),
            };
            status_dot(
                SharedString::from(format!("{id}-dot-{ix}")),
                color,
                hollow,
                false,
            )
            .into_any_element()
        }
    })
}

/// A row of color swatches; the keyboard cursor rings one of them.
fn swatch_row(
    state: MenuState<'_>,
    id: &'static str,
    ix: usize,
    (current, count, enabled): (usize, usize, bool),
    action: MenuAction,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let selected = state.selected == Some(ix);
    let current = menu_model::swatch_color(current);
    div()
        .flex()
        .flex_wrap()
        .gap(px(space::S))
        .p(px(space::S))
        .rounded(px(radius::SMALL))
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .when(!enabled, |el| el.opacity(opacity::DISABLED))
        .children((0..count).map(|at| {
            let action = action.clone();
            color_swatch(
                id,
                menu_model::swatch_color(at),
                current,
                theme,
                selected && at == state.swatch,
                cx,
                move |shell, _, window, cx| {
                    if enabled {
                        shell.run_menu_item(&action, at, false, window, cx);
                    }
                },
            )
        }))
        .into_any_element()
}

/// Draws `rows` as one popover. Items, swatches, headings, notes and separators come from
/// `rows`; `header` (the Repositories search box) sits above them. The panel takes the keyboard
/// (key context `Menu`, [`Shell::menu_key_down`]) and a click outside dismisses it.
pub(crate) fn render_menu(
    state: MenuState<'_>,
    id: &'static str,
    rows: Rows,
    header: Option<gpui::AnyElement>,
    place: Place,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let slot = rows
        .iter()
        .any(|row| matches!(row, MenuRow::Item(item) if item.check.is_some()));
    let rows: Vec<gpui::AnyElement> = rows
        .into_iter()
        .enumerate()
        .map(|(ix, row)| match row {
            MenuRow::Separator => div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
                .into_any_element(),
            MenuRow::Heading(text) => div()
                .px(px(space::S))
                .pt(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element(),
            MenuRow::Note(text) => div()
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element(),
            MenuRow::Item(item) => item_row(state, id, ix, item, slot, theme, cx),
            MenuRow::Swatches {
                current,
                count,
                enabled,
                action,
            } => swatch_row(state, id, ix, (current, count, enabled), action, theme, cx),
        })
        .collect();
    let panel = menu_panel(id, theme)
        .track_focus(state.focus)
        .key_context(MENU_CONTEXT)
        .on_key_down(cx.listener(Shell::menu_key_down))
        .on_mouse_down_out(cx.listener(|shell, _, window, cx| shell.dismiss_menus(window, cx)))
        .children(header)
        .children(rows);
    match place {
        Place::At(position) => deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element(),
        Place::Dropdown => deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element(),
    }
}

impl Shell {
    pub(crate) fn menu_state(&self) -> MenuState<'_> {
        MenuState {
            focus: &self.menu_focus,
            selected: self.menu_selected,
            swatch: self.menu_swatch,
        }
    }

    /// Whether any popover menu is open.
    pub(crate) fn menu_open(&self) -> bool {
        self.tab_menu.is_some()
            || self.layout_menu.is_some()
            || self.pane_menu.is_some()
            || self.share_menu.is_some()
            || self.workspace_menu.is_some()
            || self.file_menu.is_some()
            || self.file_tab_menu.is_some()
            || self.run_menu.is_some()
            || self.workspace_switcher
            || self.new_menu
            || self.run_with.is_some()
            || self.fan_out.is_some()
            || self.card_menu.is_some()
            || self.git_menu.is_some()
    }

    /// The rows of the open menu, rebuilt: the keyboard decides on the same rows the screen
    /// shows. One arm per menu.
    pub(crate) fn open_menu_rows(&self) -> Option<Rows> {
        if let Some(menu) = &self.tab_menu {
            return Some(self.tab_menu_rows(menu));
        }
        if let Some(menu) = &self.layout_menu {
            return Some(self.layout_menu_rows(menu));
        }
        if let Some(menu) = &self.pane_menu {
            return self.pane_menu_rows(menu);
        }
        if let Some(menu) = &self.share_menu {
            return Some(self.share_menu_rows(menu));
        }
        if let Some(project_id) = self.run_menu {
            return Some(self.run_menu_rows(project_id));
        }
        if let Some(menu) = &self.workspace_menu {
            return Some(self.workspace_menu_rows(menu));
        }
        if let Some(menu) = &self.file_menu {
            return Some(self.file_menu_rows(menu));
        }
        if let Some(menu) = &self.file_tab_menu {
            return Some(self.file_tab_menu_rows(menu));
        }
        if self.workspace_switcher {
            return self
                .workspace
                .map(|project_id| self.switcher_rows(project_id));
        }
        if self.new_menu {
            return Some(self.new_menu_rows());
        }
        if let Some(menu) = &self.card_menu {
            return Some(self.card_menu_rows(menu));
        }
        if let Some(menu) = &self.run_with {
            return Some(run_with_rows(menu, None));
        }
        if let Some(menu) = &self.fan_out {
            return Some(fan_out_rows(menu));
        }
        if let (Some(menu), Some(project_id)) = (self.git_menu, self.workspace) {
            let view = self.git.get(&project_id)?;
            return Some(match menu {
                GitMenu::ChangedBy => self.git_changed_by_rows(project_id, view),
                GitMenu::More => self.git_more_rows(project_id, view),
                GitMenu::Repos => self.git_repo_rows(project_id, &view.repo),
            });
        }
        None
    }

    /// What has the keyboard now, and the terminal session when it is that session's pane.
    fn keyboard_owner(&self, window: &Window, cx: &App) -> Option<(FocusHandle, Option<Ulid>)> {
        let terminal = self.focused_session().filter(|session| {
            self.panes
                .get(session)
                .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
        });
        window.focused(cx).map(|focus| (focus, terminal))
    }

    /// First step of every opener: closes any other menu and remembers where the keyboard is
    /// (a menu that is still open keeps what its predecessor remembered). Call it right before
    /// setting the menu's own field, after the opener's own focus changes.
    pub(crate) fn begin_menu(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let back = if self.menu_focus.contains_focused(window, cx) {
            self.menu_return.take()
        } else {
            self.keyboard_owner(window, cx)
        };
        self.close_menus();
        self.menu_return = back;
    }

    /// Last step of every opener, after its field is set: the menu takes the keyboard (opened by
    /// mouse or keyboard); a keyboard open also selects the first enabled row.
    pub(crate) fn finish_menu(
        &mut self,
        by_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let rows = self.open_menu_rows();
        self.menu_selected = rows
            .as_deref()
            .filter(|_| by_keyboard)
            .and_then(menu_model::first);
        self.menu_swatch = match (&rows, self.menu_selected) {
            (Some(rows), Some(at)) => menu_model::swatch_start(rows, at),
            _ => 0,
        };
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }

    /// Closes every menu and gives the keyboard back to what had it before the first opened
    /// (unless the keyboard already moved on). Nothing happens when no menu is open.
    pub(crate) fn dismiss_menus(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !self.menu_open() {
            return;
        }
        let back = self.menu_return.take();
        let held = self.menu_focus.contains_focused(window, cx);
        self.close_menus();
        if held {
            match back {
                Some((focus, session))
                    if session.is_none_or(|session| self.panes.contains_key(&session)) =>
                {
                    window.focus(&focus, cx);
                }
                _ => self.focus_pane(window, cx),
            }
        }
        cx.notify();
    }

    /// A click or Enter on a row: the menu closes first (the keyboard goes back), then the
    /// action runs, so an action that opens a dialog, a field or another menu takes the
    /// keyboard from there. A multi-select row only runs and the menu stays.
    pub(crate) fn run_menu_item(
        &mut self,
        action: &MenuAction,
        swatch: usize,
        toggle: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if toggle {
            action(self, swatch, window, cx);
            cx.notify();
            return;
        }
        self.dismiss_menus(window, cx);
        action(self, swatch, window, cx);
    }

    /// Keys of the open menu (the panel has the keyboard): see [`menu_model::key`].
    pub(crate) fn menu_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(rows) = self.open_menu_rows() else {
            return self.dismiss_menus(window, cx);
        };
        match menu_model::key(
            &event.keystroke,
            event.is_held,
            &rows,
            self.menu_selected,
            self.menu_swatch,
        ) {
            MenuKey::Select(at) => {
                self.menu_selected = Some(at);
                self.menu_swatch = menu_model::swatch_start(&rows, at);
            }
            MenuKey::Swatch(_, to) => self.menu_swatch = to,
            MenuKey::Activate(at, swatch) => {
                if let Some(action) = menu_model::action_of(&rows, at).cloned() {
                    return self.run_menu_item(&action, swatch, false, window, cx);
                }
            }
            MenuKey::Toggle(at) => {
                if let Some(action) = menu_model::action_of(&rows, at).cloned() {
                    return self.run_menu_item(&action, 0, true, window, cx);
                }
            }
            MenuKey::Close => return self.dismiss_menus(window, cx),
            MenuKey::Ignore => return,
        }
        cx.notify();
    }

    /// Selects the first row under `heading` (now, or as soon as the rows exist). The workspace
    /// menu's agent and shell lists load after it opens.
    pub(crate) fn menu_select_after(&mut self, heading: &'static str) {
        self.menu_pending = Some(heading);
        self.apply_menu_pending();
    }

    /// Applies a pending [`Self::menu_select_after`] when its rows are there.
    pub(crate) fn apply_menu_pending(&mut self) {
        let Some(heading) = self.menu_pending else {
            return;
        };
        let Some(rows) = self.open_menu_rows() else {
            return;
        };
        if let Some(at) = menu_model::after_heading(&rows, heading) {
            self.menu_selected = Some(at);
            self.menu_swatch = menu_model::swatch_start(&rows, at);
            self.menu_pending = None;
        }
    }
}

/// What a menu opened from the keyboard hangs from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Anchor {
    /// The title bar's workspace switcher: where every owner falls back to.
    Switcher,
}

impl Anchor {
    /// Owners inside the cached sidebar view (it re-records them only when it re-renders).
    fn in_sidebar(&self) -> bool {
        match self {
            Self::Switcher => false,
        }
    }
}

/// Where owners were laid out in the last frame (recorded by [`record_bounds`] during
/// prepaint, cleared at the start of every `Shell` render, except the sidebar's keys: the
/// sidebar is a cached view that clears and records them itself).
#[derive(Clone, Default)]
pub(crate) struct MenuAnchors(Rc<RefCell<HashMap<Anchor, Bounds<Pixels>>>>);

impl gpui::Global for MenuAnchors {}

impl MenuAnchors {
    pub(crate) fn clear(&self) {
        self.0.borrow_mut().retain(|key, _| key.in_sidebar());
    }

    fn bottom_left(&self, key: &Anchor) -> Option<Point<Pixels>> {
        self.0.borrow().get(key).map(Bounds::bottom_left)
    }
}

/// A zero-cost layer that records its parent's bounds under `key` while the frame is laid
/// out. Put it as a child of the owner element; the owner must be positioned (`relative`).
pub(crate) fn record_bounds(key: Anchor, cx: &App) -> impl IntoElement + use<> {
    let anchors = cx.global::<MenuAnchors>().clone();
    canvas(
        move |bounds, _, _| {
            anchors.0.borrow_mut().insert(key, bounds);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

impl Shell {
    /// The bottom-left of `key`'s owner as painted in the last frame; else of the title bar's
    /// switcher; else just under the title bar.
    pub(crate) fn menu_anchor(&self, key: &Anchor, window: &Window, cx: &App) -> Point<Pixels> {
        let anchors = cx.global::<MenuAnchors>();
        anchors
            .bottom_left(key)
            .or_else(|| anchors.bottom_left(&Anchor::Switcher))
            .unwrap_or_else(|| {
                point(
                    px(space::M),
                    theme::rems_px(theme::TITLE_BAR_HEIGHT).to_pixels(window.rem_size()),
                )
            })
    }
}
