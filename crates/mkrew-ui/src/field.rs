//! Drawing a text field's [`Input`](crate::text_box::Input) like a normal text box (SPEC §13):
//! the text with its selection highlighted and the caret where it is, and the mouse handling
//! that puts the caret under the pointer (a drag or Shift+click selects, a double / triple
//! click selects the word / line). Shared by the shell's fields and the terminal's find bar.

use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Entity, HighlightStyle, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, Point, SharedString, StyledText, TextLayout, canvas, div, fill, px,
    rgb, size,
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
/// selected), the bytes of `selection` highlighted, a muted placeholder when empty. The text's
/// layout maps pointer positions to bytes once painted (`None` when empty).
pub fn caret_text(
    text: &str,
    cursor: usize,
    selection: Option<Range<usize>>,
    placeholder: impl Into<SharedString>,
    focused: bool,
    theme: &Theme,
) -> (AnyElement, Option<TextLayout>) {
    if text.is_empty() {
        let caret = div()
            .flex_none()
            .w(px(theme::BORDER_WIDTH))
            .h(font_size::BODY)
            .bg(rgb(theme.text));
        let empty = div()
            .flex()
            .items_center()
            .when(focused, |row| row.child(caret))
            .child(
                div()
                    .text_color(rgb(theme.text_subtle))
                    .child(placeholder.into()),
            );
        return (empty.into_any_element(), None);
    }
    let lit = HighlightStyle {
        background_color: Some(rgb(theme.terminal_selection_bg).into()),
        ..HighlightStyle::default()
    };
    let selection = selection.filter(|s| !s.is_empty());
    let show_caret = focused && selection.is_none();
    let styled = StyledText::new(text.to_owned()).with_highlights(selection.map(|s| (s, lit)));
    let layout = styled.layout().clone();
    let (caret_layout, caret_color) = (layout.clone(), rgb(theme.text));
    let cursor = cursor.min(text.len());
    let shown = div().relative().child(styled).when(show_caret, |el| {
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
    (shown.into_any_element(), Some(layout))
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
