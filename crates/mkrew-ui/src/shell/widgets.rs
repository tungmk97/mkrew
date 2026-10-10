//! Shared widgets: buttons, text fields, menus, status marks, tooltips.

use crate::*;

/// A centered message: optional title, one sentence, optional smaller detail (an
/// [`empty_state`] without a glyph).
pub(crate) fn message(
    theme: &Theme,
    title: Option<&'static str>,
    sentence: SharedString,
    detail: Option<SharedString>,
) -> gpui::Div {
    empty_state(theme, None, title, sentence, detail)
}

/// A calm empty state (ADR 0027): an optional glyph tile, an optional title, one muted
/// sentence and an optional smaller detail, centered in a narrow column.
pub(crate) fn empty_state(
    theme: &Theme,
    glyph: Option<&'static str>,
    title: Option<&'static str>,
    sentence: SharedString,
    detail: Option<SharedString>,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(space::S))
        .p(px(space::XL))
        .font_family(theme.ui_font_family.clone())
        .when_some(glyph, |el, glyph| {
            el.child(
                div()
                    .size(theme::rems_px(theme::EMPTY_STATE_TILE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::MEDIUM))
                    .bg(rgb(theme.surface))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_color(rgb(theme.text_muted))
                    .child(glyph),
            )
        })
        .children(title.map(|title| {
            div()
                .text_size(font_size::CARD_TITLE)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(title)
        }))
        .child(
            div()
                .max_w(px(theme::EMPTY_STATE_MAX_WIDTH))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(sentence),
        )
        .children(detail.map(|detail| {
            div()
                .max_w(px(theme::READABLE_WIDTH))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(detail)
        }))
}

/// A one-line notice in the tab bar, shortened to fit; `color` sets errors apart from warnings.
pub(crate) fn notice(color: u32, text: String) -> gpui::Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_size(font_size::META)
        .text_color(rgb(color))
        .child(text)
}

/// The shared frame of [`button`], [`quiet_button`] and [`danger_button`]: no colors, no hover
/// (gpui allows one `hover` per element).
pub(crate) fn button_frame(
    id: &'static str,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .child(label.into())
}

/// A small text button: primary (accent, `on_accent` text) or ghost (muted, hover bg).
pub(crate) fn button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    if primary {
        button_frame(id, label)
            .bg(rgb(theme.accent))
            .text_color(rgb(theme.on_accent))
            .font_weight(FontWeight(weight::MEDIUM))
            .hover(|style| style.opacity(opacity::HOVER))
            .active(|style| style.opacity(opacity::ACTIVE))
    } else {
        quiet_button(id, label, theme, theme.hover)
    }
}

/// The idle Run button: a green fill (`Start`'s color) with `on_accent` text, like a primary
/// [`button`] in green.
pub(crate) fn run_fill_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    button_frame(id, label)
        .bg(rgb(run::action_colors(RunAction::Start, theme).fg))
        .text_color(rgb(theme.on_accent))
        .font_weight(FontWeight(weight::MEDIUM))
        .hover(|style| style.opacity(opacity::HOVER))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A ghost glyph button of a live Run: the glyph in the action's color, its tint on hover.
pub(crate) fn run_icon_button(
    id: &'static str,
    label: &'static str,
    action: RunAction,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let colors = run::action_colors(action, theme);
    button_frame(id, label)
        .text_color(rgb(colors.fg))
        .hover(move |style| style.bg(rgb(colors.tint)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A secondary button: surface background with a faint border (the empty state's New shell).
pub(crate) fn secondary_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    button_frame(id, label)
        .bg(rgb(theme.surface))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .text_color(rgb(theme.text))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A quiet [`button`] whose hover shows `hover` (e.g. one step above a raised card).
pub(crate) fn quiet_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
    hover: u32,
) -> gpui::Stateful<gpui::Div> {
    button_frame(id, label)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A [`button`] that cannot be used right now: faded, no hover, no click.
pub(crate) fn disabled_button(
    id: &'static str,
    label: &'static str,
    theme: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    let base = div()
        .id(id)
        .flex()
        .items_center()
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .opacity(opacity::DISABLED)
        .text_size(font_size::BODY)
        .child(label);
    if primary {
        base.bg(rgb(theme.accent)).text_color(rgb(theme.on_accent))
    } else {
        base.text_color(rgb(theme.text_muted))
    }
}

/// A [`button`] that discards work (Undo's confirm): drawn in the diff "deleted" colors, so
/// it never reads as the accent's safe default.
pub(crate) fn danger_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let text = theme.text;
    button_frame(id, label)
        .bg(rgb(theme.diff_del_bg))
        .text_color(rgb(theme.diff_del_text))
        .font_weight(FontWeight(weight::MEDIUM))
        // Stronger than an opacity change on the faint background.
        .hover(move |style| style.text_color(rgb(text)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A keyboard shortcut chip (`Keymap::label` text). On an accent button it is drawn in
/// `on_accent` at reduced opacity.
pub(crate) fn kbd(text: String, theme: &Theme, on_accent: bool) -> gpui::Div {
    let base = div()
        .flex_none()
        .px(px(space::XS))
        .rounded(px(radius::SMALL))
        .text_size(font_size::META);
    if on_accent {
        base.text_color(rgb(theme.on_accent))
            .opacity(opacity::SECONDARY)
            .child(text)
    } else {
        base.bg(rgb(theme.surface_raised))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .text_color(rgb(theme.text_muted))
            .child(text)
    }
}

/// Covers a group of title bar controls while a dialog is open: it swallows the mouse, so the
/// controls neither click nor hover.
pub(crate) fn inert_layer() -> gpui::Div {
    div().absolute().inset_0().occlude()
}

/// One ─ □ ✕ button of the Windows title bar. `area` makes Windows treat it as the system
/// caption button (hover shows here; the click and the Snap Layouts flyout are the system's).
pub(crate) fn window_control(
    id: &'static str,
    glyph: &'static str,
    area: WindowControlArea,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let close = matches!(area, WindowControlArea::Close);
    let (hover, on_hover) = (theme.hover, theme.on_accent);
    let red = theme.status_exited;
    div()
        .id(id)
        .window_control_area(area)
        .flex_none()
        .w(theme::rems_px(theme::WINDOW_CONTROL_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| {
            if close {
                style.bg(rgb(red)).text_color(rgb(on_hover))
            } else {
                style.bg(rgb(hover))
            }
        })
        .child(glyph)
}

/// A square ghost button holding one glyph (theme toggle, sidebar +).
pub(crate) fn icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(theme::ICON_BUTTON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(glyph)
}

/// What a key does to a one-line field of the Files find bar.
pub(crate) enum FieldEdit {
    /// Backspace, Ctrl+Backspace or Ctrl+A.
    Key(FindKey),
    Insert(String),
}

impl FieldEdit {
    pub(crate) fn apply(self, input: &mut Input) {
        match self {
            FieldEdit::Key(key) => {
                key.edit(input);
            }
            FieldEdit::Insert(text) => {
                input.insert(&text);
            }
        }
    }
}

/// The edit `keystroke` makes to a find / replace field: Ctrl+V (a clipboard without text
/// pastes nothing; line breaks become spaces), Backspace, Ctrl+Backspace, Ctrl+A, or typed
/// text.
pub(crate) fn field_edit(keystroke: &gpui::Keystroke, cx: &App) -> Option<FieldEdit> {
    match find_key(keystroke)? {
        FindKey::Paste => {
            let text = cx.read_from_clipboard().and_then(|item| item.text());
            Some(FieldEdit::Insert(
                text.unwrap_or_default().replace(['\r', '\n'], " "),
            ))
        }
        FindKey::Insert(text) => Some(FieldEdit::Insert(text)),
        key @ (FindKey::Backspace | FindKey::DeleteWord | FindKey::SelectAll) => {
            Some(FieldEdit::Key(key))
        }
        FindKey::Close | FindKey::Older | FindKey::Newer => None,
    }
}

/// Platform text input (IME, Unikey) for the field focused by `focus`, fed to `on_key` (see
/// [`ime::Typing`]). An invisible layer over the field: only the focused field registers.
pub(crate) fn typing_input(
    focus: &FocusHandle,
    on_key: ime::OnKey<Shell>,
    marked: &Rc<ime::Composing<FocusHandle>>,
    cx: &Context<'_, Shell>,
) -> impl IntoElement + use<> {
    let (focus, view, marked) = (focus.clone(), cx.entity(), marked.clone());
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let typing = ime::Typing {
                view,
                on_key,
                marked,
                focus: focus.clone(),
                caret: bounds,
            };
            window.handle_input(&focus, typing, cx);
        },
    )
    .absolute()
    .size_full()
}

/// A text input (task dialog, follow-up, composer): its lines with a caret at the end while
/// `focused`, the lines highlighted while `selected` (Ctrl+A), or a muted placeholder when
/// empty. Callers make clicks focus it.
pub(crate) fn text_field(
    id: &'static str,
    text: &str,
    placeholder: &'static str,
    focused: bool,
    selected: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let caret = || {
        div()
            .flex_none()
            .w(px(theme::BORDER_WIDTH))
            .h(font_size::BODY)
            .bg(rgb(theme.text))
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.len() - 1;
    div()
        .id(id)
        .flex()
        .flex_col()
        .px(px(space::S))
        .py(px(space::S))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if focused {
            theme.pane_focus_border
        } else {
            theme.border
        }))
        .bg(rgb(theme.surface_raised))
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text))
        .cursor(CursorStyle::IBeam)
        .when(text.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .when(focused, |row| row.child(caret()))
                    .child(div().text_color(rgb(theme.text_muted)).child(placeholder)),
            )
        })
        .when(!text.is_empty(), |el| {
            el.children(lines.into_iter().enumerate().map(|(i, line)| {
                // An empty line still takes a line's height.
                let line = if line.is_empty() { " " } else { line };
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .min_w_0()
                            .when(selected, |el| el.bg(rgb(theme.terminal_selection_bg)))
                            .child(line.to_owned()),
                    )
                    .when(focused && !selected && i == last, |row| row.child(caret()))
            }))
        })
}

/// One row of a unified diff (undo preview, Docs "View changes").
pub(crate) fn diff_row(line: &undo_dialog::DiffLine, theme: &Theme) -> gpui::Div {
    let (bg, fg) = match line.kind {
        LineKind::Added => (Some(theme.diff_add_bg), theme.diff_add_text),
        LineKind::Removed => (Some(theme.diff_del_bg), theme.diff_del_text),
        LineKind::Hunk => (Some(theme.diff_hunk_header), theme.text_muted),
        LineKind::Header => (None, theme.text_muted),
        LineKind::Context => (None, theme.text),
    };
    // An empty line still takes a line's height.
    let text = if line.text.is_empty() {
        " ".to_owned()
    } else {
        line.text.clone()
    };
    div()
        .px(px(space::S))
        .whitespace_nowrap()
        .text_color(rgb(fg))
        .when_some(bg, |el, bg| el.bg(rgb(bg)))
        .child(text)
}

/// The text of an `ai.generated`, or why there is none.
pub(crate) fn ai_result(done: &AiGeneratedEvent) -> Result<String, String> {
    match (&done.text, &done.error) {
        (Some(text), _) => Ok(text.clone()),
        (None, error) => Err(error.clone().unwrap_or_else(|| "no answer".to_owned())),
    }
}

/// A muted one-line note (e.g. "Loading comments…").
pub(crate) fn meta_line(theme: &Theme, text: SharedString) -> gpui::Div {
    div()
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .child(text)
}

/// A full-width row of a popover menu; callers add the label (and a shortcut on the right).
pub(crate) fn menu_item(id: SharedString, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    // One step above the menu's own `surface_raised`.
    let hover = theme.border;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// The floating panel of a menu drawn in a `deferred(anchored(..))` (workspace, switcher, Run ▾,
/// Run with, Several agents, tab, pane and Files menus). It occludes the mouse: gpui hit-tests every hitbox
/// until one blocks, so without `.occlude()` a click on the menu also reaches the row or card
/// painted under it (a color pick would navigate to the row behind the swatch).
pub(crate) fn menu_panel(
    id: impl Into<gpui::ElementId>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(theme::MENU_WIDTH))
        .flex()
        .flex_col()
        .p(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface_raised))
        .shadow_md()
        .font_family(theme.ui_font_family.clone())
        .occlude()
}

/// One color choice of a color menu: a filled dot (`None` = an empty ring = no color); the
/// current one is ringed in the text color. A click calls `pick`; the element id is
/// `<id_prefix>-<color>`.
pub(crate) fn color_swatch(
    id_prefix: &'static str,
    color: Option<&'static str>,
    current: Option<&str>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
    pick: impl Fn(&mut Shell, Option<&'static str>, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> gpui::AnyElement {
    let fill = color.and_then(|name| theme.workspace_color(name));
    let ring = if color == current {
        theme.text
    } else {
        fill.unwrap_or(theme.text_muted)
    };
    div()
        .id(SharedString::from(format!(
            "{id_prefix}-{}",
            color.unwrap_or("none")
        )))
        .flex_none()
        .size(px(theme::WORKSPACE_SWATCH_SIZE))
        .rounded_full()
        .cursor_pointer()
        .border(px(theme::WORKSPACE_SWATCH_BORDER))
        .border_color(rgb(ring))
        .when_some(fill, |el, fill| el.bg(rgb(fill)))
        .hover(|style| style.opacity(opacity::HOVER))
        .tooltip(tooltip(theme, color.unwrap_or("No color"), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            pick(shell, color, window, cx);
        }))
        .into_any_element()
}

/// The pin before the name of a pinned workspace, session or tab.
pub(crate) fn pin_mark() -> gpui::Div {
    div()
        .flex_none()
        .text_size(font_size::META)
        .child(theme::PIN_GLYPH)
}

/// A thin progress bar: `fraction` (0–1) of the track in the accent color.
pub(crate) fn progress_bar(fraction: f32, height: f32, theme: &Theme) -> gpui::Div {
    div()
        .w_full()
        .h(px(height))
        .rounded(px(radius::SMALL))
        .overflow_hidden()
        .bg(rgb(theme.border))
        .child(div().h_full().w(relative(fraction)).bg(rgb(theme.accent)))
}

/// A conflict warning (SPEC §11, M5): ⚠ and the first file edited by several sessions, every
/// file on hover; "Dismiss" (shown on hover) clears it on every card and pane showing it.
pub(crate) fn conflict_badge(
    id: String,
    paths: &[&str],
    theme: &Theme,
    dismiss: impl Fn(&mut Shell) + 'static,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let group = SharedString::from(format!("conflict-{id}"));
    let hover = theme.border;
    div()
        .id(group.clone())
        .group(group.clone())
        .flex()
        .items_center()
        .gap(px(space::XS))
        .min_w_0()
        .text_size(font_size::META)
        .tooltip(tooltip(theme, &conflicts::hover(paths), ""))
        .child(
            div()
                .flex_none()
                .text_color(rgb(theme.status_needs_input))
                .child(conflicts::ICON),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(theme.text))
                .child(format!("Conflict: {}", conflicts::label(paths))),
        )
        .child(
            div()
                .id(SharedString::from(format!("conflict-dismiss-{id}")))
                .flex_none()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .text_color(rgb(theme.text_muted))
                .opacity(opacity::HIDDEN)
                .group_hover(group, |style| style.opacity(opacity::FULL))
                .hover(move |style| style.bg(rgb(hover)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .tooltip(tooltip(
                    theme,
                    "Hide this warning on every card and pane",
                    "",
                ))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    // Not a click on the card (which would open its terminal).
                    cx.stop_propagation();
                    dismiss(shell);
                    cx.notify();
                }))
                .child("Dismiss"),
        )
}

/// A session's status mark (SPEC §11): three dots taking turns while working, else the glyph.
/// Fixed width, so rows with different states line up.
pub(crate) fn status_mark(state: SessionState, now: i64, theme: &Theme) -> gpui::AnyElement {
    let color = rgb(sidebar::status_color(theme, state));
    let mark = div()
        .flex_none()
        .w(theme::rems_px(theme::STATUS_MARK_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    if state != SessionState::Working {
        return mark
            .text_color(color)
            .child(status::icon(state))
            .into_any_element();
    }
    mark.child(working_dots(now, color)).into_any_element()
}

/// The `working` mark's three dots taking turns, lit per [`theme::STATUS_TICK`].
pub(crate) fn working_dots(now: i64, color: gpui::Rgba) -> gpui::Div {
    let lit = status::working_dot(now);
    div()
        .flex()
        .items_center()
        .gap(theme::rems_px(theme::STATUS_DOT_GAP))
        .children((0..status::WORKING_DOTS).map(move |i| {
            div()
                .size(theme::rems_px(theme::STATUS_DOT))
                .rounded_full()
                .bg(color)
                .opacity(if i == lit {
                    opacity::FULL
                } else {
                    opacity::STATUS_DOT_DIM
                })
        }))
}

/// The Mkrew logo glyph for the current appearance (`icon/mkrew-1-lanes/glyph-*.svg`): the dark
/// accent on dark, the light accent on light.
pub(crate) fn logo(theme: &Theme) -> gpui::Img {
    static DARK: LazyLock<Arc<gpui::Image>> = LazyLock::new(|| {
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            include_bytes!("../../../../icon/mkrew-1-lanes/glyph-dark.svg").to_vec(),
        ))
    });
    static LIGHT: LazyLock<Arc<gpui::Image>> = LazyLock::new(|| {
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            include_bytes!("../../../../icon/mkrew-1-lanes/glyph-light.svg").to_vec(),
        ))
    });
    let image = if theme.dark { &DARK } else { &LIGHT };
    gpui::img(Arc::clone(image))
        .flex_none()
        .size(theme::rems_px(theme::LOGO_SIZE))
}

/// An agent badge (ADR 0025) as a small square chip, so glyphs and letters line up and never
/// read as part of the name.
pub(crate) fn agent_chip(badge: String, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .size(theme::rems_px(theme::AGENT_CHIP_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .text_color(rgb(theme.text))
        .child(badge)
}

/// Tooltip of a status: its source and how long ago it changed, aged when it opens (a
/// done or needs-input status is not redrawn while it sits there).
pub(crate) fn status_tooltip(
    theme: &Theme,
    state: SessionState,
    source: StateSource,
    changed_at: i64,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let style = PopupStyle::of(theme);
    move |_, cx| {
        let text = status::hover(state, source, changed_at, status::now_ms()).into();
        let style = style.clone();
        cx.new(|_| Tooltip {
            style,
            text,
            keys: SharedString::default(),
        })
        .into()
    }
}

/// Builds a tooltip showing `text` and, if any, its keyboard shortcut.
pub(crate) fn tooltip(
    theme: &Theme,
    text: &str,
    keys: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let style = PopupStyle::of(theme);
    let text = SharedString::from(text.to_owned());
    let keys = keys.into();
    move |_, cx| {
        let (style, text, keys) = (style.clone(), text.clone(), keys.clone());
        cx.new(|_| Tooltip { style, text, keys }).into()
    }
}

/// The theme tokens a tooltip or drag preview draws with: these closures are built for every
/// hoverable element on every frame, so they carry this instead of a whole [`Theme`].
#[derive(Clone)]
pub(crate) struct PopupStyle {
    pub(crate) border: u32,
    pub(crate) surface_raised: u32,
    pub(crate) text: u32,
    pub(crate) text_muted: u32,
    pub(crate) ui_font_family: SharedString,
}

impl PopupStyle {
    pub(crate) fn of(theme: &Theme) -> Self {
        Self {
            border: theme.border,
            surface_raised: theme.surface_raised,
            text: theme.text,
            text_muted: theme.text_muted,
            ui_font_family: theme.ui_font_family.clone(),
        }
    }
}

pub(crate) struct Tooltip {
    pub(crate) style: PopupStyle,
    pub(crate) text: SharedString,
    pub(crate) keys: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = &self.style;
        div()
            .flex()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .shadow_sm()
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::META)
            .text_color(rgb(theme.text))
            .child(self.text.clone())
            .when(!self.keys.is_empty(), |el| {
                el.child(
                    div()
                        .text_color(rgb(theme.text_muted))
                        .child(self.keys.clone()),
                )
            })
    }
}
