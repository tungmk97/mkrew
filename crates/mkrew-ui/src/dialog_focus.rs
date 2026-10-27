//! Keyboard rules for the controls of a modal dialog (ADR 0085): which key moves the focus,
//! which keys a focused button takes, which auto-repeats are dropped. Pure and tested
//! headlessly; `shell/focus.rs` applies them and `shell/widgets.rs` draws the ring.

use gpui::Keystroke;

/// What a key press means to the dialog's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    /// Tab (`true`) / Shift+Tab (`false`): focus the next / previous control.
    Move(bool),
    /// A focused control takes the key (Enter and Space click it, typing does nothing): the
    /// dialog's own key handler must not see it.
    Absorb,
    /// Not ours: the dialog's key handler decides (Esc, arrows, Ctrl+Enter, its own text).
    Pass,
}

/// `on_control`: a control, not the dialog's own text, has the keyboard. `edge`: the dialog's
/// own fields have none left in the Tab direction (always true for a dialog without fields), so
/// Tab leaves them for the controls.
pub fn nav(keystroke: &Keystroke, on_control: bool, edge: bool) -> Nav {
    let m = keystroke.modifiers;
    if m.control || m.alt || m.platform {
        return Nav::Pass;
    }
    match keystroke.key.as_str() {
        "tab" if on_control || edge => Nav::Move(!m.shift),
        _ if !on_control => Nav::Pass,
        "escape" | "up" | "down" | "left" | "right" | "home" | "end" | "pageup" | "pagedown" => {
            Nav::Pass
        }
        _ => Nav::Absorb,
    }
}

/// Whether a key down must be dropped before a focused control sees it. gpui clicks a focused
/// control on the release of an Enter / Space it saw pressed, and an auto-repeating key down
/// records again: the Enter that opened a dialog, held a moment too long, would click whatever
/// has focus. Only unmodified Enter / Space matter; a held key in a text field is typing and is
/// not routed here (the caller checks that a control has the keyboard).
pub fn blocks_repeat(keystroke: &Keystroke, held: bool) -> bool {
    held && !keystroke.modifiers.modified() && matches!(keystroke.key.as_str(), "enter" | "space")
}

/// Where ← / → send the keyboard inside a row of chips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Prev,
    Next,
    /// At an end of the row: the key is taken and nothing moves.
    Stay,
}

/// What ← / → do on chip `index` of a row of `len`; `None` for every other key.
pub fn group_step(keystroke: &Keystroke, index: usize, len: usize) -> Option<Step> {
    if keystroke.modifiers.modified() {
        return None;
    }
    match keystroke.key.as_str() {
        "left" => Some(if index > 0 { Step::Prev } else { Step::Stay }),
        "right" => Some(if index + 1 < len {
            Step::Next
        } else {
            Step::Stay
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(keys: &str) -> Keystroke {
        Keystroke::parse(keys).unwrap()
    }

    #[test]
    fn tab_moves_from_a_control_and_from_the_edge_of_the_fields_only() {
        assert_eq!(nav(&press("tab"), true, false), Nav::Move(true));
        assert_eq!(nav(&press("shift-tab"), true, false), Nav::Move(false));
        assert_eq!(nav(&press("tab"), false, true), Nav::Move(true));
        assert_eq!(nav(&press("shift-tab"), false, true), Nav::Move(false));
        // In the middle of the fields the dialog's own Tab (the next field) acts.
        assert_eq!(nav(&press("tab"), false, false), Nav::Pass);
        assert_eq!(nav(&press("shift-tab"), false, false), Nav::Pass);
    }

    #[test]
    fn a_focused_control_takes_enter_space_and_typing_but_not_the_dialogs_keys() {
        for keys in ["enter", "space", "a", "backspace", "shift-a"] {
            assert_eq!(nav(&press(keys), true, false), Nav::Absorb, "{keys}");
        }
        for keys in [
            "escape",
            "up",
            "down",
            "left",
            "right",
            "home",
            "end",
            "pageup",
            "pagedown",
            "ctrl-enter",
            "ctrl-a",
            "ctrl-tab",
        ] {
            assert_eq!(nav(&press(keys), true, false), Nav::Pass, "{keys}");
        }
    }

    #[test]
    fn the_dialogs_own_text_keeps_every_key_but_the_edge_tab() {
        for keys in ["enter", "space", "a", "escape", "up", "ctrl-enter"] {
            assert_eq!(nav(&press(keys), false, true), Nav::Pass, "{keys}");
        }
    }

    #[test]
    fn only_an_unmodified_repeating_enter_or_space_is_dropped() {
        for keys in ["enter", "space"] {
            assert!(blocks_repeat(&press(keys), true), "{keys}");
            assert!(
                !blocks_repeat(&press(keys), false),
                "{keys}: the first press"
            );
        }
        for keys in ["ctrl-enter", "shift-space", "escape", "tab", "a"] {
            assert!(!blocks_repeat(&press(keys), true), "{keys}");
        }
    }

    #[test]
    fn arrows_move_inside_a_row_of_chips_and_stop_at_its_ends() {
        let step = |keys: &str, index| group_step(&press(keys), index, 3);
        assert_eq!(step("left", 1), Some(Step::Prev));
        assert_eq!(step("right", 1), Some(Step::Next));
        assert_eq!(step("left", 0), Some(Step::Stay));
        assert_eq!(step("right", 2), Some(Step::Stay));
        assert_eq!(step("up", 1), None);
        assert_eq!(step("shift-left", 1), None);
        assert_eq!(step("enter", 1), None);
        assert_eq!(
            group_step(&press("right"), 0, 1),
            Some(Step::Stay),
            "a row of one"
        );
    }
}
