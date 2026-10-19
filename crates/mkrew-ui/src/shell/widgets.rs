//! Shared widgets: buttons, text fields, menus, status marks, tooltips.

use crate::*;
use gpui::{Animation, AnimationExt, pulsating_between};

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
/// (gpui allows one `hover` per element). `label` is text or an [`icon_svg`].
pub(crate) fn button_frame(id: &'static str, label: impl IntoElement) -> gpui::Stateful<gpui::Div> {
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
        .child(label)
}

/// A small text button: primary (accent, `on_accent` text) or ghost (muted, hover bg).
pub(crate) fn button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    if primary {
        button_frame(id, label)
            .bg(rgb(theme.accent))
            .text_color(rgb(button_fg(theme, true)))
            .font_weight(FontWeight(weight::MEDIUM))
            .hover(|style| style.opacity(opacity::HOVER))
            .active(|style| style.opacity(opacity::ACTIVE))
    } else {
        quiet_button(id, label, theme, theme.hover)
    }
}

/// The label color of [`button`]: `on_accent` on the primary fill, else `text_muted`. An
/// [`icon_text`] inside a button takes the same color (an svg does not inherit it).
pub(crate) fn button_fg(theme: &Theme, primary: bool) -> u32 {
    if primary {
        theme.on_accent
    } else {
        theme.text_muted
    }
}

/// A button label with a leading icon: `glyph`, then `text`, on one line, both in `color`.
pub(crate) fn icon_text(glyph: Icon, text: impl IntoElement, color: u32) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, color))
        .child(text)
}

/// A secondary button: surface background with a faint border (the empty state's New shell).
pub(crate) fn secondary_button(
    id: &'static str,
    label: impl IntoElement,
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
    label: impl IntoElement,
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
    label: impl IntoElement,
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
    label: impl IntoElement,
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

/// One minimize / maximize / close button of the Windows title bar. `area` makes Windows treat
/// it as the system caption button (hover shows here; the click and the Snap Layouts flyout are
/// the system's). The close icon turns `on_accent` on the red hover through the `id` group: an
/// svg does not inherit its parent's hover color.
pub(crate) fn window_control(
    id: &'static str,
    glyph: Icon,
    area: WindowControlArea,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let close = matches!(area, WindowControlArea::Close);
    let (hover, on_hover) = (theme.hover, theme.on_accent);
    let red = theme.status_exited;
    div()
        .id(id)
        .group(id)
        .window_control_area(area)
        .flex_none()
        .w(theme::rems_px(theme::WINDOW_CONTROL_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |style| style.bg(rgb(if close { red } else { hover })))
        .child(
            icon_svg(glyph, theme::ICON_SIZE_SMALL, theme.text_muted).when(close, |icon| {
                icon.group_hover(id, move |style| style.text_color(rgb(on_hover)))
            }),
        )
}

/// A square ghost button holding one icon (theme toggle, sidebar +).
pub(crate) fn icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    icon_button_in(id, glyph, theme.text_muted, theme)
}

/// An [`icon_button`] whose icon is drawn in `color`: a state (the phone icon is accent while
/// phone access is on).
pub(crate) fn icon_button_in(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    color: u32,
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
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(icon_svg(glyph, theme::ICON_SIZE, color))
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
    typing_target: &ime::FocusedField<Shell>,
    cx: &Context<'_, Shell>,
) -> impl IntoElement + use<> {
    let (focus, view, marked, typing_target) = (
        focus.clone(),
        cx.entity(),
        marked.clone(),
        typing_target.clone(),
    );
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            // Voice input types into the field that has the keyboard (§20G).
            if focus.is_focused(window) {
                typing_target.replace(Some((focus.clone(), on_key)));
            }
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
    let selection = (selected && !text.is_empty()).then_some(0..text.len());
    field_frame(id, text, text.len(), selection, placeholder, focused, theme).0
}

/// A press or drag in a [`caret_field`]: the byte under the pointer, whether it extends the
/// selection (Shift, a drag) and the click count (2 = word, 3 = line).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Caret {
    pub at: usize,
    pub extend: bool,
    pub clicks: usize,
}

/// A field that edits `input` like a normal text box: the caret and selection drawn where
/// they are, a click puts the caret under the pointer, a drag or Shift+click selects, a
/// double / triple click selects the word / line. `on_caret` focuses the field and applies
/// the [`Caret`] (`Input::click`).
pub(crate) fn caret_field(
    id: &'static str,
    input: &Input,
    placeholder: &'static str,
    focused: bool,
    theme: &Theme,
    cx: &Context<'_, Shell>,
    on_caret: impl Fn(&mut Shell, Caret) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let (field, layout) = field_frame(
        id,
        input.as_str(),
        input.cursor(),
        input.selection(),
        placeholder,
        focused,
        theme,
    );
    let on_caret = Rc::new(on_caret);
    let at = move |position: Point<Pixels>| {
        layout
            .as_ref()
            .map_or(0, |layout| closest_byte(layout, position))
    };
    let at = Rc::new(at);
    let (down, down_at) = (Rc::clone(&on_caret), Rc::clone(&at));
    field
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, event: &MouseDownEvent, _, cx| {
                down(
                    shell,
                    Caret {
                        at: down_at(event.position),
                        extend: event.modifiers.shift,
                        clicks: event.click_count,
                    },
                );
                cx.notify();
            }),
        )
        .on_mouse_move(cx.listener(move |shell, event: &MouseMoveEvent, _, cx| {
            if event.dragging() {
                let caret = Caret {
                    at: at(event.position),
                    extend: true,
                    clicks: 1,
                };
                on_caret(shell, caret);
                cx.notify();
            }
        }))
}

/// The byte of `layout`'s text nearest to window point `position`: the line under it (the
/// first / last above / below the text), the char boundary closest along it.
fn closest_byte(layout: &gpui::TextLayout, position: Point<Pixels>) -> usize {
    let (bounds, line_height) = (layout.bounds(), layout.line_height());
    let lines = layout.line_layouts();
    let (mut top, mut start) = (bounds.top(), 0);
    for (i, line) in lines.iter().enumerate() {
        let height = line.size(line_height).height;
        if position.y < top + height || i + 1 == lines.len() {
            // Inside the line's rows: a y past its last row would find no row at all.
            let last_row = (height - px(theme::BORDER_WIDTH)).max(Pixels::ZERO);
            let y = (position.y - top).clamp(Pixels::ZERO, last_row);
            let within = point(position.x - bounds.left(), y);
            return start
                + line
                    .closest_index_for_position(within, line_height)
                    .unwrap_or_else(|i| i);
        }
        top += height;
        start += line.len() + 1;
    }
    0
}

/// The field's frame and text: the caret at byte `cursor` while `focused` (none while
/// something is selected), the bytes of `selection` highlighted, a muted placeholder when
/// empty. The text's layout maps pointer positions to bytes once painted (`None` when empty).
fn field_frame(
    id: &'static str,
    text: &str,
    cursor: usize,
    selection: Option<std::ops::Range<usize>>,
    placeholder: &'static str,
    focused: bool,
    theme: &Theme,
) -> (gpui::Stateful<gpui::Div>, Option<gpui::TextLayout>) {
    let frame = div()
        .id(id)
        .relative()
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
        .cursor(CursorStyle::IBeam);
    if text.is_empty() {
        let caret = div()
            .flex_none()
            .w(px(theme::BORDER_WIDTH))
            .h(font_size::BODY)
            .bg(rgb(theme.text));
        let frame = frame.child(
            div()
                .flex()
                .items_center()
                .when(focused, |row| row.child(caret))
                .child(div().text_color(rgb(theme.text_muted)).child(placeholder)),
        );
        return (frame, None);
    }
    let lit = HighlightStyle {
        background_color: Some(rgb(theme.terminal_selection_bg).into()),
        ..HighlightStyle::default()
    };
    let styled = StyledText::new(text.to_owned()).with_highlights(
        selection
            .clone()
            .filter(|s| !s.is_empty())
            .map(|s| (s, lit)),
    );
    let layout = styled.layout().clone();
    let show_caret = focused && selection.is_none();
    let (caret_layout, caret_color) = (layout.clone(), rgb(theme.text));
    let cursor = cursor.min(text.len());
    let frame = frame.child(styled).when(show_caret, |el| {
        // Painted after the text's prepaint, so its layout knows where the caret byte sits.
        el.child(
            canvas(
                |_, _, _| (),
                move |_, (), window, _| {
                    if let Some(at) = caret_layout.position_for_index(cursor) {
                        let size = size(px(theme::BORDER_WIDTH), caret_layout.line_height());
                        window.paint_quad(fill(Bounds::new(at, size), caret_color));
                    }
                },
            )
            .absolute()
            .size_full(),
        )
    });
    (frame, Some(layout))
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

/// A round status dot: filled in `color`, or `hollow` (a ring: idle, stopped). `pulse` breathes
/// its opacity (working); gpui draws it still under reduce-motion. `id` keys the animation.
pub(crate) fn status_dot(
    id: impl Into<gpui::ElementId>,
    color: u32,
    hollow: bool,
    pulse: bool,
) -> gpui::AnyElement {
    let dot = div()
        .flex_none()
        .size(theme::rems_px(theme::STATUS_DOT_LARGE))
        .rounded_full()
        .when(hollow, |el| {
            el.border(px(theme::BORDER_WIDTH)).border_color(rgb(color))
        })
        .when(!hollow, |el| el.bg(rgb(color)));
    if !pulse {
        return dot.into_any_element();
    }
    dot.with_animation(
        id,
        Animation::new(theme::PULSE_PERIOD)
            .repeat_synced()
            .with_easing(pulsating_between(theme::PULSE_MIN_OPACITY, opacity::FULL))
            .with_max_fps(theme::PULSE_FPS),
        |dot, value| dot.opacity(value),
    )
    .into_any_element()
}

/// The voice level meter (§20G): five bars, tallest in the middle, that rise with `level`
/// (0..=1, [`crate::voice::Level::meter`]).
pub(crate) fn voice_meter(level: f32, color: gpui::Rgba) -> gpui::Div {
    const SHAPE: [f32; 5] = [0.55, 0.8, 1.0, 0.8, 0.55];
    div()
        .flex()
        .items_center()
        .h(theme::rems_px(theme::VOICE_BAR_MAX))
        .gap(theme::rems_px(theme::VOICE_BAR_GAP))
        .children(SHAPE.map(|weight| {
            let tall = theme::VOICE_BAR_MAX - theme::VOICE_BAR_MIN;
            div()
                .w(theme::rems_px(theme::VOICE_BAR_WIDTH))
                .h(theme::rems_px(theme::VOICE_BAR_MIN + tall * level * weight))
                .rounded_full()
                .bg(color)
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

/// An agent badge (ADR 0025) as a small square chip, so logos, glyphs and letters line up and
/// never read as part of the name. Logos (ADR 0058) have no border at rest; the border keeps its
/// width, so a caller's status `border_color` still rings them. `fg` paints text and logo.
pub(crate) fn agent_chip(badge: sidebar::Badge, fg: u32, theme: &Theme) -> gpui::Div {
    let chip = div()
        .flex_none()
        .size(theme::rems_px(theme::AGENT_CHIP_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .text_color(rgb(fg));
    match badge {
        sidebar::Badge::Logo(logo) => chip
            .border_color(gpui::transparent_black())
            .child(icons::logo_svg(logo, theme::AGENT_LOGO_SIZE, fg)),
        sidebar::Badge::Text(text) => chip
            .border_color(rgb(theme.border))
            .text_size(font_size::META)
            .font_weight(FontWeight(weight::SEMIBOLD))
            .child(text),
    }
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
