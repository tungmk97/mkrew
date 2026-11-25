//! Drawing a text field's [`Input`](crate::text_box::Input) like a normal text box (SPEC §13):
//! the text with its selection highlighted and the caret where it is, and the mouse handling
//! that puts the caret under the pointer (a drag or Shift+click selects, a double / triple
//! click selects the word / line). Shared by the shell's fields and the terminal's find bar.

use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Entity, HighlightStyle, Hsla, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Point, StyledText, TextLayout, TextRun, canvas, div,
    fill, px, rgb, size,
};

use crate::preview::closest_byte;
use crate::task_dialog::DialogKey;
use crate::text_box::Input;
use crate::theme::{self, Theme, font_size};

/// Ctrl+C / Ctrl+X in a field: puts `input`'s selection on the clipboard (Cut then removes it
/// in `Input::edit`). Other keys, and nothing selected, do nothing.
pub fn copy_selection(key: &DialogKey, input: &Input, cx: &mut App) {
    if matches!(key, DialogKey::Copy | DialogKey::Cut)
        && let Some(text) = input.selected_text()
    {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
    }
}

/// A press or drag in a field: the byte under the pointer, whether it extends the selection
/// (Shift, a drag) and the click count (2 = word, 3 = line). Fields apply it with
/// `Input::click`.
#[derive(Debug, Clone, Copy)]
pub struct Caret {
    pub at: usize,
    pub extend: bool,
    pub clicks: usize,
}

/// The field's text: the caret at byte `cursor` while `focused` (none while something is
/// selected), the bytes of `selection` highlighted, a muted `placeholder` (text, or an element
/// that sets its own look) when empty. The text's
/// layout maps pointer positions to bytes once painted (`None` when empty).
pub fn caret_text(
    text: &str,
    cursor: usize,
    selection: Option<Range<usize>>,
    placeholder: impl IntoElement,
    focused: bool,
    theme: &Theme,
) -> (AnyElement, Option<TextLayout>) {
    if text.is_empty() {
        return (empty_field(placeholder, focused, theme), None);
    }
    let lit = HighlightStyle {
        background_color: Some(selection_bg(theme)),
        ..HighlightStyle::default()
    };
    let selection = selection.filter(|s| !s.is_empty());
    let show_caret = focused && selection.is_none();
    let styled = StyledText::new(text.to_owned()).with_highlights(selection.map(|s| (s, lit)));
    let (shown, layout) = caret_over(styled, cursor.min(text.len()), show_caret, theme);
    (shown, Some(layout))
}

/// An empty field: the caret while `focused`, then the muted `placeholder`.
pub fn empty_field(placeholder: impl IntoElement, focused: bool, theme: &Theme) -> AnyElement {
    let caret = div()
        .flex_none()
        .w(px(theme::BORDER_WIDTH))
        .h(font_size::BODY)
        .bg(rgb(theme.text));
    div()
        .flex()
        .items_center()
        .when(focused, |row| row.child(caret))
        .child(div().text_color(rgb(theme.text_subtle)).child(placeholder))
        .into_any_element()
}

/// The background of selected text in a field.
pub fn selection_bg(theme: &Theme) -> Hsla {
    rgb(theme.terminal_selection_bg).into()
}

/// `styled` with the caret drawn at byte `cursor` when `show`; the layout maps pointer positions
/// to bytes once painted.
pub fn caret_over(
    styled: StyledText,
    cursor: usize,
    show: bool,
    theme: &Theme,
) -> (AnyElement, TextLayout) {
    let layout = styled.layout().clone();
    let (caret_layout, caret_color) = (layout.clone(), rgb(theme.text));
    let shown = div().relative().child(styled).when(show, |el| {
        // Painted after the text's prepaint, so its layout knows where the caret byte sits. It
        // paints at the layout's window position, so it takes no room: a full-size layer would
        // sit below the text and add its height to a scrolling field's content, scrolling the
        // text out of view (the task dialog's prompt).
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
            .size_0(),
        )
    });
    (shown.into_any_element(), layout)
}

/// `runs` with the bytes of `range` on `bg` (runs cut at its ends): a selection over text that
/// has its own runs (colors, fonts).
pub fn highlight_runs(runs: Vec<TextRun>, range: Range<usize>, bg: Hsla) -> Vec<TextRun> {
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut start = 0;
    for run in runs {
        let end = start + run.len;
        let cuts = [
            start,
            range.start.clamp(start, end),
            range.end.clamp(start, end),
            end,
        ];
        for cut in cuts.windows(2).filter(|cut| cut[0] < cut[1]) {
            let mut part = run.clone();
            part.len = cut[1] - cut[0];
            if range.start <= cut[0] && cut[1] <= range.end {
                part.background_color = Some(bg);
            }
            out.push(part);
        }
        start = end;
    }
    out
}

/// Makes `field` put the caret under the pointer: a press calls `on_caret` with the byte
/// under it (Shift extends, the click count picks word / line), a drag extends from there.
/// `layout` is [`caret_text`]'s. Focusing the field stays the caller's.
pub fn caret_mouse<V: 'static, E: InteractiveElement>(
    field: E,
    view: &Entity<V>,
    layout: Option<TextLayout>,
    on_caret: impl Fn(&mut V, Caret) + 'static,
) -> E {
    let at = Rc::new(move |position: Point<Pixels>| {
        layout
            .as_ref()
            .map_or(0, |layout| closest_byte(layout, position))
    });
    let on_caret = Rc::new(on_caret);
    let (down, down_at) = (Rc::clone(&on_caret), Rc::clone(&at));
    let down_view = view.clone();
    let move_view = view.clone();
    field
        .on_mouse_down(
            MouseButton::Left,
            move |event: &MouseDownEvent, _, cx: &mut App| {
                down_view.update(cx, |view, cx| {
                    let caret = Caret {
                        at: down_at(event.position),
                        extend: event.modifiers.shift,
                        clicks: event.click_count,
                    };
                    down(view, caret);
                    cx.notify();
                });
            },
        )
        .on_mouse_move(move |event: &MouseMoveEvent, _, cx: &mut App| {
            if event.dragging() {
                move_view.update(cx, |view, cx| {
                    let caret = Caret {
                        at: at(event.position),
                        extend: true,
                        clicks: 1,
                    };
                    on_caret(view, caret);
                    cx.notify();
                });
            }
        })
}
