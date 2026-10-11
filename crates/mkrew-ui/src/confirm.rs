//! The in-app confirmation dialog behind every `window.prompt` (SPEC §14): where each answer
//! sits and what the keys do. The binary draws it (`shell/confirm_prompt.rs`).

use gpui::{Keystroke, PromptButton};

/// The answer Esc picks: the caller's "Cancel" (`PromptButton::Cancel`), if any.
pub fn cancel_index(buttons: &[PromptButton]) -> Option<usize> {
    buttons.iter().position(PromptButton::is_cancel)
}

/// Answer indices left to right: Cancel first, then the others last to first, so answer 0
/// (the default) sits on the right like every Mkrew dialog's primary button.
pub fn visual_order(buttons: &[PromptButton]) -> Vec<usize> {
    let cancel = cancel_index(buttons);
    cancel
        .into_iter()
        .chain((0..buttons.len()).rev().filter(|ix| Some(*ix) != cancel))
        .collect()
}

/// What a key does in the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Close the dialog with this answer.
    Answer(usize),
    /// Move the keyboard ring to this answer.
    Select(usize),
    Ignore,
}

/// Esc answers Cancel; Enter / Space answer the selected button; Tab / → move right and
/// Shift+Tab / ← move left along `order` (from [`visual_order`]), wrapping.
pub fn key(keystroke: &Keystroke, order: &[usize], selected: usize, cancel: Option<usize>) -> Key {
    if order.is_empty() {
        return Key::Ignore;
    }
    let step = |right: bool| {
        let len = order.len();
        let at = order.iter().position(|ix| *ix == selected).unwrap_or(0);
        let next = if right {
            (at + 1) % len
        } else {
            (at + len - 1) % len
        };
        Key::Select(order[next])
    };
    match keystroke.key.as_str() {
        "escape" => cancel.map_or(Key::Ignore, Key::Answer),
        "enter" | "space" => Key::Answer(selected),
        "tab" if keystroke.modifiers.shift => step(false),
        "tab" | "right" => step(true),
        "left" => step(false),
        _ => Key::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buttons(labels: &[&str]) -> Vec<PromptButton> {
        labels
            .iter()
            .map(|label| PromptButton::from(*label))
            .collect()
    }

    fn press(keys: &str, order: &[usize], selected: usize, cancel: Option<usize>) -> Key {
        key(&Keystroke::parse(keys).unwrap(), order, selected, cancel)
    }

    #[test]
    fn cancel_sits_left_and_the_first_answer_right() {
        let close = buttons(&["Kill session", "Hide pane", "Cancel"]);
        assert_eq!(cancel_index(&close), Some(2));
        assert_eq!(visual_order(&close), vec![2, 1, 0]);
        assert_eq!(visual_order(&buttons(&["Remove", "Cancel"])), vec![1, 0]);
        // Cancel stays leftmost wherever the caller put it.
        assert_eq!(
            visual_order(&buttons(&["Cancel", "Save", "Don't save"])),
            vec![0, 2, 1]
        );
        // No Cancel: last answer to first.
        let plain = buttons(&["Yes", "No"]);
        assert_eq!(cancel_index(&plain), None);
        assert_eq!(visual_order(&plain), vec![1, 0]);
    }

    #[test]
    fn esc_answers_cancel_enter_and_space_answer_the_selected() {
        let order = [2, 1, 0];
        assert_eq!(press("escape", &order, 0, Some(2)), Key::Answer(2));
        assert_eq!(press("enter", &order, 1, Some(2)), Key::Answer(1));
        assert_eq!(press("space", &order, 0, Some(2)), Key::Answer(0));
        assert_eq!(press("a", &order, 0, Some(2)), Key::Ignore);
    }

    #[test]
    fn tab_and_arrows_walk_the_visual_order_and_wrap() {
        let order = [2, 1, 0];
        // 0 is rightmost: moving right wraps to the leftmost (Cancel).
        assert_eq!(press("tab", &order, 0, Some(2)), Key::Select(2));
        assert_eq!(press("right", &order, 0, Some(2)), Key::Select(2));
        assert_eq!(press("shift-tab", &order, 0, Some(2)), Key::Select(1));
        assert_eq!(press("left", &order, 0, Some(2)), Key::Select(1));
        // Leftmost: moving left wraps to the rightmost.
        assert_eq!(press("left", &order, 2, Some(2)), Key::Select(0));
        assert_eq!(press("tab", &order, 2, Some(2)), Key::Select(1));
    }

    #[test]
    fn without_cancel_esc_does_nothing_and_without_buttons_every_key_is_ignored() {
        assert_eq!(press("escape", &[1, 0], 0, None), Key::Ignore);
        for keys in ["enter", "space", "tab", "left", "escape"] {
            assert_eq!(press(keys, &[], 0, None), Key::Ignore, "{keys}");
        }
    }
}
