//! Platform text input for the hand-rolled fields and the terminal (SPEC §10, §13): text from
//! an IME (Windows Vietnamese Telex, CJK) or sent as Unicode input (Unikey, EVKey) arrives here
//! instead of as key presses. It becomes key presses for the field's own key handler: chars
//! are typed, and composing text is typed in place and erased with Backspace when it changes.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, InputHandler, KeyDownEvent, Keystroke, Pixels,
    Point, UTF16Selection, Window,
};

/// The composing chars typed so far and the field (`K`, a focus handle) they were typed into.
/// Telex keeps composing across a click into another field; that field never typed them.
#[derive(Debug)]
pub struct Composing<K> {
    field: RefCell<Option<K>>,
    marked: Cell<usize>,
}

impl<K> Default for Composing<K> {
    fn default() -> Self {
        Self {
            field: RefCell::new(None),
            marked: Cell::new(0),
        }
    }
}

impl<K: PartialEq + Clone> Composing<K> {
    /// The composing count for `field`: zero when the chars were typed into another one.
    pub fn marked_for(&self, field: &K) -> &Cell<usize> {
        let mut owner = self.field.borrow_mut();
        if owner.as_ref() != Some(field) {
            *owner = Some(field.clone());
            self.marked.set(0);
        }
        &self.marked
    }
}

/// Key presses that turn the `marked` composing chars into `text`; `text` is composing again
/// (and counted in `marked`) when `mark`. Control chars are dropped: Enter, Tab and Backspace
/// come as real key presses.
pub fn keys(marked: &Cell<usize>, text: &str, mark: bool) -> Vec<Keystroke> {
    let backspace = Keystroke {
        key: "backspace".into(),
        ..Keystroke::default()
    };
    let mut presses: Vec<Keystroke> = (0..marked.replace(0)).map(|_| backspace.clone()).collect();
    let mut typed = 0;
    for c in text.chars().filter(|c| !c.is_control()) {
        let s = c.to_string();
        presses.push(Keystroke {
            key: s.clone(),
            key_char: Some(s),
            ..Keystroke::default()
        });
        typed += 1;
    }
    if mark {
        marked.set(typed);
    }
    presses
}

/// A field's key handler, e.g. `Shell::composer_key_down`.
pub type OnKey<V> = fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<'_, V>);

/// The input handler of one focused field: feeds `view`'s `on_key`.
pub struct Typing<V: 'static> {
    pub view: Entity<V>,
    pub on_key: OnKey<V>,
    /// Composing chars typed so far, and into which field.
    pub marked: Rc<Composing<FocusHandle>>,
    /// The focused field this handler serves.
    pub focus: FocusHandle,
    /// Where the IME shows its candidate window.
    pub caret: Bounds<Pixels>,
}

impl<V: 'static> Typing<V> {
    fn send(&self, text: &str, mark: bool, window: &mut Window, cx: &mut App) {
        for keystroke in keys(self.marked.marked_for(&self.focus), text, mark) {
            let event = KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            };
            let on_key = self.on_key;
            self.view
                .update(cx, |view, cx| on_key(view, &event, window, cx));
        }
    }
}

// The fields only append and erase at the end, so ranges are ignored and the "document" is
// just the composing text: selection = end of it.
impl<V: 'static> InputHandler for Typing<V> {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        let end = self.marked.marked_for(&self.focus).get();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Range<usize>> {
        let len = self.marked.marked_for(&self.focus).get();
        (len > 0).then_some(0..len)
    }

    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        None
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.send(text, false, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.send(text, true, window, cx);
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut App) {
        self.marked.marked_for(&self.focus).set(0);
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        Some(self.caret)
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{Composing, keys};
    use crate::task_dialog::{DialogKey, dialog_key};
    use crate::terminal::input::{InputMode, key_to_bytes};
    use crate::text_box::{Input, TextBox};

    /// Windows Vietnamese Telex composing "việt": each step replaces the composing text,
    /// revising chars typed before; the last step commits.
    fn telex(marked: &Cell<usize>) -> Vec<gpui::Keystroke> {
        let mut presses = Vec::new();
        for composing in ["v", "vi", "vie", "viê", "việ"] {
            presses.extend(keys(marked, composing, true));
        }
        presses.extend(keys(marked, "việt", false));
        presses
    }

    #[test]
    fn composition_steps_end_as_the_committed_text_in_a_text_box() {
        let marked = Cell::new(0);
        let mut text = TextBox::default();
        text.insert("tôi ");
        for keystroke in telex(&marked) {
            if let Some(key) = dialog_key(&keystroke) {
                text.key(key);
            }
        }
        assert_eq!(text.text(), "tôi việt", "earlier text is never erased");
        assert_eq!(marked.get(), 0);
    }

    /// Ctrl+A, then Telex: the first composing char replaces the selection, the re-typing
    /// Backspaces only erase composing chars (plain Backspace stays one char).
    #[test]
    fn composition_after_select_all_replaces_the_selection_once() {
        let marked = Cell::new(0);
        let mut input = Input::from("tôi cũ");
        input.select_all();
        for keystroke in telex(&marked) {
            match dialog_key(&keystroke) {
                Some(DialogKey::Insert(text)) => {
                    input.insert(&text);
                }
                Some(key) => {
                    input.edit(&key);
                }
                None => {}
            }
        }
        assert_eq!(input, "việt");
        assert_eq!(marked.get(), 0);
    }

    #[test]
    fn composition_steps_reach_the_terminal_as_the_committed_text() {
        let marked = Cell::new(0);
        // What a shell line editor does with the bytes: DEL (0x7f) erases one char.
        let mut line = String::new();
        for keystroke in telex(&marked) {
            let bytes = key_to_bytes(&keystroke, InputMode::default()).expect("typed key");
            if bytes == [0x7f] {
                line.pop();
            } else {
                line.push_str(std::str::from_utf8(&bytes).expect("utf-8"));
            }
        }
        assert_eq!(line, "việt");
    }

    #[test]
    fn committed_text_types_each_char_and_drops_control_chars() {
        let marked = Cell::new(0);
        let typed: Vec<String> = keys(&marked, "ệ\u{8}a\r", false)
            .into_iter()
            .map(|k| k.key_char.unwrap_or(k.key))
            .collect();
        assert_eq!(typed, ["ệ", "a"]);
    }

    /// Windows Telex keeps composing when the click moves to another field: the next step
    /// must not erase that field's own text with the first field's composing count.
    #[test]
    fn composition_moved_to_another_field_never_erases_its_text() {
        let composing = Composing::<u8>::default();
        assert_eq!(keys(composing.marked_for(&1), "vi", true).len(), 2);
        let moved = keys(composing.marked_for(&2), "việ", true);
        assert!(moved.iter().all(|k| k.key != "backspace"), "{moved:?}");
        assert_eq!(moved.len(), 3);
        let next = keys(composing.marked_for(&2), "việt", false);
        assert_eq!(next.iter().filter(|k| k.key == "backspace").count(), 3);
    }
}
