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

/// The shared frame of [`button`], [`quiet_button`] and [`danger_button`]: a control-height
/// row, no colors, no hover (gpui allows one `hover` per element). `label` is text or an
/// [`icon_svg`].
pub(crate) fn button_frame(id: &'static str, label: impl IntoElement) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(space::S))
        .h(theme::rems_px(theme::CONTROL_HEIGHT))
        .px(px(space::M))
        .rounded(px(radius::MEDIUM))
        .cursor_pointer()
        .whitespace_nowrap()
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
            .font_weight(FontWeight(weight::SEMIBOLD))
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

/// A secondary button: transparent with a 1 px border, soft text.
pub(crate) fn secondary_button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    button_frame(id, label)
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.control_border))
        .text_color(rgb(theme.text_dim))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A ghost [`button`] whose hover shows `hover` (e.g. one step above a raised card) and the
/// full text color.
pub(crate) fn quiet_button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
    hover: u32,
) -> gpui::Stateful<gpui::Div> {
    let text = theme.text;
    button_frame(id, label)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)).text_color(rgb(text)))
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
        .flex_none()
        .items_center()
        .gap(px(space::S))
        .h(theme::rems_px(theme::CONTROL_HEIGHT))
        .px(px(space::M))
        .rounded(px(radius::MEDIUM))
        .whitespace_nowrap()
        .text_size(font_size::BODY)
        .child(label);
    if primary {
        // A muted fill, not a faded accent: it must not read as the next step.
        base.bg(rgb(theme.border))
            .text_color(rgb(theme.text_disabled))
            .font_weight(FontWeight(weight::SEMIBOLD))
    } else {
        base.opacity(opacity::DISABLED)
            .text_color(rgb(theme.text_muted))
    }
}

/// A ghost [`button`] that discards work (Discard, Undo's confirm): danger text, so it never
/// reads as the accent's safe default.
pub(crate) fn danger_button(
    id: &'static str,
    label: impl IntoElement,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.danger_bg;
    button_frame(id, label)
        .text_color(rgb(theme.danger_text))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// Keyboard focus for the controls of a modal dialog (ADR 0081). gpui keeps the focus handle
/// of an element that has a tab index in its element state, so a control needs no field on
/// `Shell`. Every control here is a `Stateful<Div>` ([`button`], [`secondary_button`],
/// [`quiet_button`], [`danger_button`], [`icon_button`], [`segment`], chips, check rows).
pub(crate) trait DialogControl: Sized {
    /// A Tab stop that shows the `focus_ring` while the keyboard has it. It keeps a 1 px
    /// border, transparent at rest, so the ring shifts nothing. Enter / Space click it (gpui).
    fn focus_stop(self, theme: &Theme) -> Self;
    /// [`DialogControl::focus_stop`] for a control that already draws its own 1 px border (a
    /// secondary button, a chip): only the ring color changes.
    fn focusable_bordered(self, theme: &Theme) -> Self;
    /// ← / → on a focused chip move the keyboard to its neighbour in a row of `len`; the
    /// ends keep it.
    fn chip_arrows(self, index: usize, len: usize) -> Self;
    /// The panel of a modal dialog: it has the keyboard through `focus` (a Tab stop when the
    /// dialog has `has_text` fields, so Shift+Tab can come back to them), `guard` runs first
    /// in the capture phase ([`Shell::dialog_guard`]) and `on_key` gets every key.
    fn dialog_root(
        self,
        focus: &FocusHandle,
        has_text: bool,
        guard: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
        on_key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self;
}

impl DialogControl for gpui::Stateful<gpui::Div> {
    fn focus_stop(self, theme: &Theme) -> Self {
        self.border(px(theme::BORDER_WIDTH))
            .border_color(gpui::transparent_black())
            .focusable_bordered(theme)
    }

    fn focusable_bordered(self, theme: &Theme) -> Self {
        let ring = rgb(theme.focus_ring);
        self.tab_index(0)
            .focus_visible(move |style| style.border_color(ring))
    }

    fn chip_arrows(self, index: usize, len: usize) -> Self {
        self.on_key_down(move |event, window, cx| {
            let Some(step) = dialog_focus::group_step(&event.keystroke, index, len) else {
                return;
            };
            cx.stop_propagation();
            match step {
                dialog_focus::Step::Prev => window.focus_prev(cx),
                dialog_focus::Step::Next => window.focus_next(cx),
                dialog_focus::Step::Stay => {}
            }
        })
    }

    fn dialog_root(
        self,
        focus: &FocusHandle,
        has_text: bool,
        guard: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
        on_key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.track_focus(&focus.clone().tab_stop(has_text))
            .capture_key_down(guard)
            .on_key_down(on_key)
    }
}

/// A keyboard shortcut hint (`Keymap::label` text): a small mono key cap. On an accent
/// button it is drawn in `on_accent` at reduced opacity.
pub(crate) fn kbd(text: String, theme: &Theme, on_accent: bool) -> gpui::Div {
    let base = div().flex_none().text_size(font_size::META);
    if on_accent {
        base.px(px(space::XS))
            .text_color(rgb(theme.on_accent))
            .opacity(opacity::SECONDARY)
            .child(text)
    } else {
        base.px(px(space::XS))
            .rounded(px(radius::TAG))
            .border(px(theme::BORDER_WIDTH))
            .border_b(px(theme::KBD_BOTTOM_BORDER))
            .border_color(rgb(theme.selected))
            .bg(rgb(theme.row_hover))
            .font_family(theme.terminal_font_family.clone())
            .text_size(font_size::OVERLINE)
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
            icon_svg(glyph, theme::ICON_SIZE_SMALL, theme.text_subtle).when(close, |icon| {
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
    let (hover, text) = (theme.hover, theme.text);
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(theme::CONTROL_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::MEDIUM))
        .cursor_pointer()
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)).text_color(rgb(text)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(icon_svg(glyph, theme::ICON_SIZE, color))
}

/// A segmented control's container (title bar views, sidebar filter, Board, Git): `chrome` with
/// a divider ring. Children are [`segment`]s.
pub(crate) fn segmented(theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .p(px(theme::SEGMENT_PADDING))
        .rounded(px(radius::SEGMENTED))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.divider))
        .bg(rgb(theme.chrome))
}

/// One segment of a [`segmented`] control: selected = `selected` fill and full text, else
/// transparent and muted with a hover.
pub(crate) fn segment(
    id: impl Into<gpui::ElementId>,
    active: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let (hover, text) = (theme.hover, theme.text);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(space::XS))
        .h(theme::rems_px(theme::SEGMENT_HEIGHT))
        .px(px(space::M))
        .rounded(px(radius::CHIP))
        .cursor_pointer()
        .whitespace_nowrap()
        .text_size(font_size::BODY)
        .when(active, |el| {
            el.bg(rgb(theme.selected)).text_color(rgb(text))
        })
        .when(!active, |el| {
            el.text_color(rgb(theme.text_muted))
                .hover(move |style| style.bg(rgb(hover)).text_color(rgb(text)))
                .active(|style| style.opacity(opacity::ACTIVE))
        })
}

/// A status pill: a 6 px dot in `color` (hollow = a ring) and `label` in `fg` on `bg`.
pub(crate) fn status_pill(
    label: impl IntoElement,
    color: u32,
    hollow: bool,
    fg: u32,
    bg: impl Into<gpui::Fill>,
) -> gpui::Div {
    let dot = div()
        .flex_none()
        .size(theme::rems_px(theme::STATUS_DOT_LARGE))
        .rounded_full();
    let dot = if hollow {
        dot.border(px(theme::BORDER_WIDTH)).border_color(rgb(color))
    } else {
        dot.bg(rgb(color))
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .h(theme::rems_px(theme::PILL_HEIGHT))
        .px(px(space::S))
        .rounded(px(radius::PILL))
        .bg(bg)
        .whitespace_nowrap()
        .text_size(font_size::META)
        .text_color(rgb(fg))
        .child(dot)
        .child(label)
}

/// A small tinted chip (`[BE]` area, Git count): `text` in `fg` on `fg`'s tint.
pub(crate) fn tinted_chip(text: impl Into<SharedString>, fg: u32, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(theme::rems_px(theme::CHIP_HEIGHT))
        .px(px(space::XS))
        .rounded(px(radius::TAG))
        .bg(theme.tint(fg))
        .text_size(font_size::OVERLINE)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .text_color(rgb(fg))
        .child(text.into())
}

/// A section overline: 11/600 upper case in `text_subtle` (STAGED, CHANGES).
pub(crate) fn overline(text: &str, theme: &Theme) -> gpui::Div {
    div()
        .text_size(font_size::OVERLINE)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .text_color(rgb(theme.text_subtle))
        .child(text.to_uppercase())
}

/// A search box: a 32 px framed row with a search icon, `field` (its own frame removed) and an
/// optional trailing hint (a [`kbd`]). The frame turns accent while `focused`.
pub(crate) fn search_box(
    field: gpui::Stateful<gpui::Div>,
    trailing: Option<gpui::AnyElement>,
    focused: bool,
    theme: &Theme,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::S))
        .h(theme::rems_px(theme::INPUT_HEIGHT))
        .px(px(space::S))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if focused { theme.accent } else { theme.border }))
        .bg(rgb(theme.chrome))
        .child(icon_svg(
            Icon::Search,
            theme::ICON_SIZE_SMALL,
            theme.text_subtle,
        ))
        .child(
            field
                .flex_1()
                .min_w_0()
                .p_0()
                .border_0()
                .bg(gpui::transparent_black()),
        )
        .children(trailing)
}

/// What a key does to a one-line field of the Files find bar.
pub(crate) enum FieldEdit {
    /// An editing or caret key ([`Input::edit`]).
    Key(DialogKey),
    Insert(String),
}

impl FieldEdit {
    /// Applies it to `input`; Ctrl+C / Ctrl+X first put the selection on the clipboard.
    pub(crate) fn apply(self, input: &mut Input, cx: &mut App) {
        match self {
            FieldEdit::Key(key) => {
                copy_selection(&key, input, cx);
                input.edit(&key);
            }
            FieldEdit::Insert(text) => {
                input.insert(&text);
            }
        }
    }
}

/// The edit `keystroke` makes to a find / replace field: Ctrl+V (a clipboard without text
/// pastes nothing; line breaks become spaces), typed text, or an editing / caret key.
pub(crate) fn field_edit(keystroke: &gpui::Keystroke, cx: &App) -> Option<FieldEdit> {
    match find_key(keystroke)? {
        FindKey::Paste => {
            let text = cx.read_from_clipboard().and_then(|item| item.text());
            Some(FieldEdit::Insert(
                text.unwrap_or_default().replace(['\r', '\n'], " "),
            ))
        }
        FindKey::Insert(text) => Some(FieldEdit::Insert(text)),
        FindKey::Edit(key) => Some(FieldEdit::Key(key)),
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
    typing_input_for(cx.entity(), focus, on_key, marked, typing_target)
}

/// [`typing_input`] from outside a `Context` (a closure holding the view).
pub(crate) fn typing_input_for(
    view: Entity<Shell>,
    focus: &FocusHandle,
    on_key: ime::OnKey<Shell>,
    marked: &Rc<ime::Composing<FocusHandle>>,
    typing_target: &ime::FocusedField<Shell>,
) -> impl IntoElement + use<> {
    let (focus, marked, typing_target) = (focus.clone(), marked.clone(), typing_target.clone());
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            // Voice input types into the field that has the keyboard (§20G), ringed while it
            // listens.
            if focus.is_focused(window) {
                typing_target.field.replace(Some((focus.clone(), on_key)));
                if let Some(color) = typing_target.ring.get() {
                    // Over the field's own border (this layer fills the box inside it).
                    let ring = bounds.dilate(px(theme::BORDER_WIDTH));
                    window.paint_quad(
                        gpui::outline(ring, color, gpui::BorderStyle::Solid)
                            .border_widths(px(theme::VOICE_RING_WIDTH))
                            .corner_radii(px(radius::MEDIUM)),
                    );
                }
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

/// A text that looks like a field but is not edited here (a setting's value before it is
/// clicked): the text, or a muted placeholder when empty.
pub(crate) fn text_field(
    id: &'static str,
    text: &str,
    placeholder: &'static str,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let (text, _) = caret_text(text, text.len(), None, placeholder, false, theme);
    field_frame(id, false, theme).child(text)
}

/// A field that edits `input` like a normal text box: the caret and selection drawn where
/// they are, a click puts the caret under the pointer, a drag or Shift+click selects, a
/// double / triple click selects the word / line. `on_caret` applies the [`Caret`]
/// (`Input::click`); callers make clicks focus the field.
pub(crate) fn caret_field(
    id: &'static str,
    input: &Input,
    placeholder: &'static str,
    focused: bool,
    theme: &Theme,
    cx: &Context<'_, Shell>,
    on_caret: impl Fn(&mut Shell, Caret) + 'static,
) -> gpui::Stateful<gpui::Div> {
    caret_field_for(
        &cx.entity(),
        id,
        input,
        placeholder,
        focused,
        theme,
        on_caret,
    )
}

/// [`caret_field`] from outside a `Context` (a closure holding the view).
pub(crate) fn caret_field_for(
    view: &Entity<Shell>,
    id: &'static str,
    input: &Input,
    placeholder: &'static str,
    focused: bool,
    theme: &Theme,
    on_caret: impl Fn(&mut Shell, Caret) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let (text, layout) = caret_text(
        input.as_str(),
        input.cursor(),
        input.selection(),
        placeholder,
        focused,
        theme,
    );
    caret_mouse(
        field_frame(id, focused, theme).child(text),
        view,
        layout,
        on_caret,
    )
}

/// A field's frame: the accent border while `focused`, an I-beam pointer.
fn field_frame(id: &'static str, focused: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .relative()
        .px(px(space::S))
        .py(px(space::S))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        // The focus ring: the accent.
        .border_color(rgb(if focused { theme.accent } else { theme.border }))
        .bg(rgb(theme.chrome))
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text))
        .cursor(CursorStyle::IBeam)
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
/// current one is ringed in the text color and `focused` rings it in the accent (the keyboard
/// cursor). A click calls `pick`; the element id is `<id_prefix>-<color>`.
pub(crate) fn color_swatch(
    id_prefix: &'static str,
    color: Option<&'static str>,
    current: Option<&str>,
    theme: &Theme,
    focused: bool,
    cx: &mut Context<'_, Shell>,
    pick: impl Fn(&mut Shell, Option<&'static str>, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> gpui::AnyElement {
    let fill = color.and_then(|name| theme.workspace_color(name));
    let ring = if focused {
        theme.accent
    } else if color == current {
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

/// The pin before the name of a pinned workspace or tab.
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

/// The voice waveform (§20G): one bar per height in `bars` (0..=1, [`crate::voice::bars`]).
pub(crate) fn voice_meter(bars: &[f32], color: gpui::Rgba) -> gpui::Div {
    let tall = theme::VOICE_BAR_MAX - theme::VOICE_BAR_MIN;
    div()
        .flex()
        .items_center()
        .h(theme::rems_px(theme::VOICE_BAR_MAX))
        .gap(theme::rems_px(theme::VOICE_BAR_GAP))
        .children(bars.iter().map(|bar| {
            div()
                .w(theme::rems_px(theme::VOICE_BAR_WIDTH))
                .h(theme::rems_px(theme::VOICE_BAR_MIN + tall * bar))
                .rounded_full()
                .bg(color)
        }))
}

/// The app mark at the left of the title bar: a small accent tile with a dark "m".
pub(crate) fn app_logo(theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .size(theme::rems_px(theme::APP_LOGO_TILE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::CHIP))
        .bg(rgb(theme.accent))
        .text_size(font_size::BODY)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .text_color(rgb(theme.on_accent))
        .child("m")
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
