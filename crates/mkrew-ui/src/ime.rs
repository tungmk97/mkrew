//! Platform text input for the hand-rolled fields and the terminal (SPEC §10, §13): text from
//! an IME (Windows Vietnamese Telex, CJK), a macOS dead key or Option character, or sent as
//! Unicode input (Unikey, EVKey) arrives here instead of as key presses. It becomes key presses
//! for the field's own key handler: chars are typed, and composing text is typed in place and
//! erased with Backspace when it changes.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, InputHandler, KeyDownEvent, Keystroke, Pixels,
    Point, UTF16Selection, Window,
};

use crate::keymap::Os;

/// The composing text typed so far: the document the input method sees. It reads it
/// (`text_for_range`) and replaces parts of it (`replace_text_in_range` with a range); ranges
/// count UTF-16 units from its start. Text that is not composing any more belongs to the
/// program or the field: it is never offered, so it is never erased.
#[derive(Debug, Default)]
pub struct Marked(RefCell<String>);

impl Marked {
    /// Length in UTF-16 units.
    pub fn len_utf16(&self) -> usize {
        self.0.borrow().encode_utf16().count()
    }

    /// Nothing is composing any more.
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    /// The text of `range` (cut to what is composing) and the range it covers.
    pub fn slice(&self, range: Range<usize>) -> (String, Range<usize>) {
        let text = self.0.borrow();
        let from = byte_at(&text, range.start);
        let to = byte_at(&text, range.end).max(from);
        let units = |end: usize| text[..end].encode_utf16().count();
        (text[from..to].to_owned(), units(from)..units(to))
    }
}

/// Byte offset of UTF-16 unit `unit` in `text`: the end when `unit` is past it, the next char
/// when it falls inside a surrogate pair.
fn byte_at(text: &str, unit: usize) -> usize {
    let mut seen = 0;
    for (byte, c) in text.char_indices() {
        if seen >= unit {
            return byte;
        }
        seen += c.len_utf16();
    }
    text.len()
}

/// The composing text typed so far and the field (`K`, a focus handle) it was typed into.
/// Telex keeps composing across a click into another field; that field never typed it.
#[derive(Debug)]
pub struct Composing<K> {
    field: RefCell<Option<K>>,
    marked: Marked,
}

impl<K> Default for Composing<K> {
    fn default() -> Self {
        Self {
            field: RefCell::new(None),
            marked: Marked::default(),
        }
    }
}

impl<K: PartialEq + Clone> Composing<K> {
    /// The composing text of `field`: empty when it was typed into another one.
    pub fn marked_for(&self, field: &K) -> &Marked {
        let mut owner = self.field.borrow_mut();
        if owner.as_ref() != Some(field) {
            *owner = Some(field.clone());
            self.marked.clear();
        }
        &self.marked
    }
}

/// Key presses that replace `range` of the `marked` composing text (`None` = all of it) with
/// `text`; the result is composing again when `mark`. Control chars of `text` are dropped:
/// Enter, Tab and Backspace come as real key presses. Only the end of a line can be edited, so
/// the presses erase back to the start of `range`, then type `text` and what followed `range`.
pub fn keys(
    marked: &Marked,
    range: Option<Range<usize>>,
    text: &str,
    mark: bool,
) -> Vec<Keystroke> {
    let old = marked.0.take();
    let (from, to) = match range {
        Some(range) => {
            let from = byte_at(&old, range.start);
            (from, byte_at(&old, range.end).max(from))
        }
        None => (0, old.len()),
    };
    let typed: String = text.chars().filter(|c| !c.is_control()).collect();
    let (head, tail) = (&old[..from], &old[to..]);
    let backspace = Keystroke {
        key: "backspace".into(),
        ..Keystroke::default()
    };
    let mut presses: Vec<Keystroke> = old[from..].chars().map(|_| backspace.clone()).collect();
    presses.extend(typed.chars().chain(tail.chars()).map(|c| {
        let s = c.to_string();
        Keystroke {
            key: s.clone(),
            key_char: Some(s),
            ..Keystroke::default()
        }
    }));
    if mark {
        *marked.0.borrow_mut() = format!("{head}{typed}{tail}");
    }
    presses
}

/// A field's key handler, e.g. `Shell::composer_key_down`.
pub type OnKey<V> = fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<'_, V>);
/// The field whose input handler was registered on the last paint while focused, with its key
/// handler: voice text (SPEC §20G) is typed there the same way an IME commit is.
pub type FocusedField<V> = Rc<TypingTarget<V>>;

/// See [`FocusedField`].
pub struct TypingTarget<V> {
    pub field: RefCell<Option<(FocusHandle, OnKey<V>)>>,
    /// While voice input listens, the focused field paints a ring in this color (§20G).
    pub ring: Cell<Option<gpui::Hsla>>,
}

impl<V> Default for TypingTarget<V> {
    fn default() -> Self {
        Self {
            field: RefCell::new(None),
            ring: Cell::new(None),
        }
    }
}

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
    fn send(
        &self,
        range: Option<Range<usize>>,
        text: &str,
        mark: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Only macOS's input methods are known to send ranges: elsewhere a range stays "all of
        // the composing text" as before (ADR 0099).
        let range = range.filter(|_| Os::CURRENT == Os::Mac);
        for keystroke in keys(self.marked.marked_for(&self.focus), range, text, mark) {
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

// The fields and the PTY only append and erase at the end, so the "document" is just the
// composing text: selection = end of it, and ranges are UTF-16 units into it.
impl<V: 'static> InputHandler for Typing<V> {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        let end = self.marked.marked_for(&self.focus).len_utf16();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Range<usize>> {
        let len = self.marked.marked_for(&self.focus).len_utf16();
        (len > 0).then_some(0..len)
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        let (text, actual) = self.marked.marked_for(&self.focus).slice(range);
        *adjusted = Some(actual);
        (!text.is_empty()).then_some(text)
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.send(range, text, false, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.send(range, text, true, window, cx);
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut App) {
        self.marked.marked_for(&self.focus).clear();
    }

    /// With an input mode that is not ASCII-capable (Japanese kana, Chinese, Korean) printable
    /// keys reach the input method first, as they must to compose; it types through
    /// `replace_text_in_range`. GPUI does not offer keys first to ASCII-capable modes such as
    /// Telex, VNI or Romaji. With a plain layout this changes nothing.
    fn prefers_ime_for_printable_keys(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
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
    use gpui::{Keystroke, Modifiers};

    use super::{Composing, Marked, keys};
    use crate::keymap::Os;
    use crate::task_dialog::{DialogKey, dialog_key};
    use crate::terminal::input::{InputMode, KeyEnv, key_to_bytes};
    use crate::text_box::{Input, TextBox};

    const MAC: KeyEnv = KeyEnv {
        os: Os::Mac,
        option_as_meta: false,
    };
    const WIN: KeyEnv = KeyEnv {
        os: Os::Windows,
        option_as_meta: false,
    };

    /// Windows Vietnamese Telex composing "việt": each step replaces the composing text,
    /// revising chars typed before; the last step commits.
    fn telex(marked: &Marked) -> Vec<Keystroke> {
        let mut presses = Vec::new();
        for composing in ["v", "vi", "vie", "viê", "việ"] {
            presses.extend(keys(marked, None, composing, true));
        }
        presses.extend(keys(marked, None, "việt", false));
        presses
    }

    /// What a shell line editor does with the bytes the terminal sends: DEL (0x7f) erases one char.
    fn shell_line(presses: &[Keystroke], env: KeyEnv) -> String {
        let mut line = String::new();
        for keystroke in presses {
            let bytes = key_to_bytes(keystroke, InputMode::default(), env).expect("typed key");
            if bytes == [0x7f] {
                line.pop();
            } else {
                line.push_str(std::str::from_utf8(&bytes).expect("utf-8"));
            }
        }
        line
    }

    #[test]
    fn composition_steps_end_as_the_committed_text_in_a_text_box() {
        let marked = Marked::default();
        let mut text = TextBox::default();
        text.insert("tôi ");
        for keystroke in telex(&marked) {
            if let Some(key) = dialog_key(&keystroke) {
                text.key(key);
            }
        }
        assert_eq!(text.text(), "tôi việt", "earlier text is never erased");
        assert_eq!(marked.len_utf16(), 0);
    }

    /// Ctrl+A, then Telex: the first composing char replaces the selection, the re-typing
    /// Backspaces only erase composing chars (plain Backspace stays one char).
    #[test]
    fn composition_after_select_all_replaces_the_selection_once() {
        let marked = Marked::default();
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
        assert_eq!(marked.len_utf16(), 0);
    }

    #[test]
    fn composition_steps_reach_the_terminal_as_the_committed_text() {
        for env in [WIN, MAC] {
            assert_eq!(shell_line(&telex(&Marked::default()), env), "việt");
        }
    }

    #[test]
    fn committed_text_types_each_char_and_drops_control_chars() {
        let typed: Vec<String> = keys(&Marked::default(), None, "ệ\u{8}a\r", false)
            .into_iter()
            .map(|k| k.key_char.unwrap_or(k.key))
            .collect();
        assert_eq!(typed, ["ệ", "a"]);
    }

    /// Windows Telex keeps composing when the click moves to another field: the next step
    /// must not erase that field's own text with the first field's composing text.
    #[test]
    fn composition_moved_to_another_field_never_erases_its_text() {
        let composing = Composing::<u8>::default();
        assert_eq!(keys(composing.marked_for(&1), None, "vi", true).len(), 2);
        let moved = keys(composing.marked_for(&2), None, "việ", true);
        assert!(moved.iter().all(|k| k.key != "backspace"), "{moved:?}");
        assert_eq!(moved.len(), 3);
        let next = keys(composing.marked_for(&2), None, "việt", false);
        assert_eq!(next.iter().filter(|k| k.key == "backspace").count(), 3);
    }

    #[test]
    fn a_range_inside_the_composing_text_erases_back_to_it() {
        let marked = Marked::default();
        let mut presses = keys(&marked, None, "viet", true);
        // The input method revises "et" (UTF-16 units 2..4) to "êt".
        presses.extend(keys(&marked, Some(2..4), "êt", true));
        assert_eq!(shell_line(&presses, MAC), "viêt");
        assert_eq!(marked.len_utf16(), 4);
    }

    #[test]
    fn a_range_in_the_middle_retypes_what_followed_it() {
        let marked = Marked::default();
        let mut presses = keys(&marked, None, "abc", true);
        presses.extend(keys(&marked, Some(1..2), "X", true));
        assert_eq!(shell_line(&presses, MAC), "aXc");
        assert_eq!(marked.slice(0..9).0, "aXc", "still composing");
    }

    #[test]
    fn a_range_past_the_composing_text_is_cut_to_it() {
        let marked = Marked::default();
        let mut presses = keys(&marked, None, "ab", true);
        presses.extend(keys(&marked, Some(5..9), "c", true));
        assert_eq!(shell_line(&presses, MAC), "abc");
        // With nothing composing a range erases nothing.
        let presses = keys(&Marked::default(), Some(0..1), "x", false);
        assert_eq!(shell_line(&presses, MAC), "x");
    }

    #[test]
    fn text_for_range_is_the_composing_text_cut_to_the_range() {
        let marked = Marked::default();
        keys(&marked, None, "việt", true);
        assert_eq!(marked.slice(1..3), ("iệ".to_owned(), 1..3));
        assert_eq!(marked.slice(2..99), ("ệt".to_owned(), 2..4));
        assert_eq!(marked.slice(9..12), (String::new(), 4..4));
        // Ranges count UTF-16 units: the emoji is two.
        let emoji = Marked::default();
        keys(&emoji, None, "a😀b", true);
        assert_eq!(emoji.len_utf16(), 4);
        assert_eq!(emoji.slice(1..3), ("😀".to_owned(), 1..3));
        assert_eq!(
            emoji.slice(1..2),
            ("😀".to_owned(), 1..3),
            "never half a pair"
        );
    }

    /// macOS Option+E then E: the first key is not typed by the terminal (it goes to the input
    /// method, which marks the accent and then replaces it), and Option+2 types its character.
    #[test]
    fn option_dead_key_goes_to_the_input_method_which_types_the_accent() {
        let option = |key: &str, composed: &str| Keystroke {
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            key: key.into(),
            key_char: Some(composed.into()),
        };
        assert_eq!(
            key_to_bytes(&option("e", "´"), InputMode::default(), MAC),
            None,
            "left to the input method"
        );
        // AppKit then calls setMarkedText("´"), and the next key insertText("é").
        let marked = Marked::default();
        let mut presses = keys(&marked, None, "´", true);
        presses.extend(keys(&marked, None, "é", false));
        assert_eq!(shell_line(&presses, MAC), "é");
        // Option+2: no marking, the input method inserts "€".
        assert_eq!(
            key_to_bytes(&option("2", "€"), InputMode::default(), MAC),
            None
        );
        let presses = keys(&Marked::default(), None, "€", false);
        assert_eq!(shell_line(&presses, MAC), "€");
    }
}
