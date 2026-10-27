//! Copy mode of a terminal pane (SPEC §10): which keys move the vi cursor and select text.
//! Pure and tested headlessly; [`Emulator::copy_key`](crate::terminal::emulator::Emulator::copy_key)
//! applies them to alacritty's vi mode. These keys are not in `keymap.toml`, like the peek's.

use alacritty_terminal::vi_mode::ViMotion;
use gpui::Keystroke;

/// What `v` / Shift+V start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Select {
    Chars,
    Lines,
}

/// What a key press does while copy mode is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKey {
    Motion(ViMotion),
    /// PgUp (`up`) / PgDn: one screen, the cursor keeps its screen row.
    Page {
        up: bool,
    },
    /// `g` (the top of the history) / `G` (the bottom).
    Edge {
        top: bool,
    },
    /// `v` / Shift+V: start a selection, switch its kind, or drop it (same kind again).
    Select(Select),
    /// `y` / Enter: copy the selection and leave copy mode.
    Copy,
    /// Esc: drop the selection, or leave copy mode when there is none.
    Cancel,
}

/// The copy-mode meaning of `keystroke`; `None` = ignored (it never reaches the program).
pub fn copy_key(keystroke: &Keystroke) -> Option<CopyKey> {
    let m = keystroke.modifiers;
    if m.control || m.alt || m.platform {
        return None;
    }
    match keystroke.key.as_str() {
        "left" => return Some(CopyKey::Motion(ViMotion::Left)),
        "right" => return Some(CopyKey::Motion(ViMotion::Right)),
        "up" => return Some(CopyKey::Motion(ViMotion::Up)),
        "down" => return Some(CopyKey::Motion(ViMotion::Down)),
        "pageup" => return Some(CopyKey::Page { up: true }),
        "pagedown" => return Some(CopyKey::Page { up: false }),
        "enter" => return Some(CopyKey::Copy),
        "escape" => return Some(CopyKey::Cancel),
        _ => {}
    }
    let typed = match keystroke.key_char.as_deref() {
        Some(text) => text.to_owned(),
        None if m.shift => keystroke.key.to_uppercase(),
        None => keystroke.key.clone(),
    };
    Some(match typed.as_str() {
        "h" => CopyKey::Motion(ViMotion::Left),
        "l" => CopyKey::Motion(ViMotion::Right),
        "k" => CopyKey::Motion(ViMotion::Up),
        "j" => CopyKey::Motion(ViMotion::Down),
        "w" => CopyKey::Motion(ViMotion::SemanticRight),
        "b" => CopyKey::Motion(ViMotion::SemanticLeft),
        "e" => CopyKey::Motion(ViMotion::SemanticRightEnd),
        "0" => CopyKey::Motion(ViMotion::First),
        "$" => CopyKey::Motion(ViMotion::Last),
        "g" => CopyKey::Edge { top: true },
        "G" => CopyKey::Edge { top: false },
        "v" => CopyKey::Select(Select::Chars),
        "V" => CopyKey::Select(Select::Lines),
        "y" => CopyKey::Copy,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    /// A parsed key press; a printable key carries its char, like a real one.
    fn key(spec: &str) -> Option<CopyKey> {
        let mut keystroke = Keystroke::parse(spec).unwrap();
        let m = keystroke.modifiers;
        if keystroke.key.chars().count() == 1 && !m.control && !m.alt {
            keystroke.key_char = Some(if m.shift {
                keystroke.key.to_uppercase()
            } else {
                keystroke.key.clone()
            });
        }
        copy_key(&keystroke)
    }

    fn motion(motion: ViMotion) -> Option<CopyKey> {
        Some(CopyKey::Motion(motion))
    }

    #[test]
    fn arrows_and_hjkl_move_the_cursor() {
        for (spec, expected) in [
            ("left", ViMotion::Left),
            ("h", ViMotion::Left),
            ("down", ViMotion::Down),
            ("j", ViMotion::Down),
            ("up", ViMotion::Up),
            ("k", ViMotion::Up),
            ("right", ViMotion::Right),
            ("l", ViMotion::Right),
        ] {
            assert_eq!(key(spec), motion(expected), "{spec}");
        }
    }

    #[test]
    fn words_lines_and_edges() {
        assert_eq!(key("w"), motion(ViMotion::SemanticRight));
        assert_eq!(key("b"), motion(ViMotion::SemanticLeft));
        assert_eq!(key("e"), motion(ViMotion::SemanticRightEnd));
        assert_eq!(key("0"), motion(ViMotion::First));
        // `$` is Shift+4 on a US layout: the char decides, not the key.
        let dollar = Keystroke {
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            key: "4".into(),
            key_char: Some("$".into()),
        };
        assert_eq!(copy_key(&dollar), motion(ViMotion::Last));
        assert_eq!(key("g"), Some(CopyKey::Edge { top: true }));
        assert_eq!(key("shift-g"), Some(CopyKey::Edge { top: false }));
        assert_eq!(key("pageup"), Some(CopyKey::Page { up: true }));
        assert_eq!(key("pagedown"), Some(CopyKey::Page { up: false }));
    }

    #[test]
    fn select_copy_and_leave() {
        assert_eq!(key("v"), Some(CopyKey::Select(Select::Chars)));
        assert_eq!(key("shift-v"), Some(CopyKey::Select(Select::Lines)));
        assert_eq!(key("y"), Some(CopyKey::Copy));
        assert_eq!(key("enter"), Some(CopyKey::Copy));
        assert_eq!(key("escape"), Some(CopyKey::Cancel));
    }

    #[test]
    fn every_other_key_does_nothing_and_never_reaches_the_program() {
        for spec in [
            "x",
            "space",
            "tab",
            "f1",
            "ctrl-h",
            "alt-j",
            "ctrl-shift-v",
            "delete",
        ] {
            assert_eq!(key(spec), None, "{spec}");
        }
    }
}
