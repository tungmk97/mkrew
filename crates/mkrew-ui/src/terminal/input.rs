//! Keyboard and paste input → bytes for the PTY (SPEC §10, §13). Pure, so it is unit-tested.
//!
//! Encodings follow xterm: CSI sequences for cursor/editing keys, `CSI 1;<mod>X` when modified,
//! SS3 for arrows in application-cursor mode, C0 control bytes for Ctrl+key, ESC prefix for Alt.

use gpui::Keystroke;

/// Terminal modes that change what keys and pastes send.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputMode {
    /// DECCKM: arrows/Home/End send `ESC O x` instead of `ESC [ x`.
    pub app_cursor: bool,
    /// Pastes are wrapped in `ESC [200~` … `ESC [201~`.
    pub bracketed_paste: bool,
}

/// Bytes a key press sends to the PTY, or `None` if the terminal should ignore it.
pub fn key_to_bytes(keystroke: &Keystroke, mode: InputMode) -> Option<Vec<u8>> {
    let m = keystroke.modifiers;
    if m.platform {
        return None;
    }
    // xterm modifier parameter: 1 + shift + 2·alt + 4·ctrl.
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);

    if let Some(bytes) = special_key(&keystroke.key, m.control, m.shift, modifier, mode) {
        return Some(with_alt(bytes, m.alt && !is_csi(&keystroke.key)));
    }
    // AltGr arrives as Ctrl+Alt with the composed char in `key_char`: send the char as is.
    let altgr = m.control && m.alt;
    let ctrl_only = m.control && !m.alt;
    if let Some(text) = keystroke.key_char.as_deref()
        && !ctrl_only
    {
        return Some(with_alt(text.as_bytes().to_vec(), m.alt && !altgr));
    }
    if m.control {
        return control_byte(&keystroke.key).map(|byte| with_alt(vec![byte], m.alt));
    }
    None
}

/// Plain Ctrl+C: copies the selection when there is one, else it is ^C for the program
/// (like Windows Terminal, ADR 0022).
pub fn is_copy_key(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    keystroke.key == "c" && m.control && !m.shift && !m.alt && !m.platform
}

/// What a key press in a terminal pane does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Copy the selection (plain Ctrl+C while text is selected, also in an exited pane).
    Copy,
    Send(Vec<u8>),
    Ignore,
}

/// The pane's answer to `keystroke`: `selected` = text is selected, `exited` = the session
/// ended (nothing more is sent).
pub fn key_action(
    keystroke: &Keystroke,
    selected: bool,
    exited: bool,
    mode: InputMode,
) -> KeyAction {
    if selected && is_copy_key(keystroke) {
        return KeyAction::Copy;
    }
    match key_to_bytes(keystroke, mode) {
        Some(bytes) if !exited => KeyAction::Send(bytes),
        _ => KeyAction::Ignore,
    }
}

/// Bytes for pasting `text`: newlines become CR (what Enter sends); in bracketed mode the text
/// is wrapped and stripped of ESC so it cannot end the bracket early.
pub fn paste_bytes(text: &str, mode: InputMode) -> Vec<u8> {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if mode.bracketed_paste {
        let mut out = b"\x1b[200~".to_vec();
        out.extend(text.bytes().filter(|&byte| byte != 0x1b));
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        text.into_bytes()
    }
}

/// Alternate scroll: `lines` wheel lines (positive = up) as that many Up/Down arrow presses.
pub fn wheel_bytes(lines: i32, mode: InputMode) -> Vec<u8> {
    let arrow: &[u8] = match (lines > 0, mode.app_cursor) {
        (true, false) => b"\x1b[A",
        (true, true) => b"\x1bOA",
        (false, false) => b"\x1b[B",
        (false, true) => b"\x1bOB",
    };
    arrow.repeat(lines.unsigned_abs() as usize)
}

/// Keys whose encoding already carries the modifiers (no extra ESC prefix for Alt).
fn is_csi(key: &str) -> bool {
    matches!(
        key,
        "up" | "down"
            | "right"
            | "left"
            | "home"
            | "end"
            | "insert"
            | "delete"
            | "pageup"
            | "pagedown"
    ) || function_key(key).is_some()
}

fn special_key(
    key: &str,
    ctrl: bool,
    shift: bool,
    modifier: u8,
    mode: InputMode,
) -> Option<Vec<u8>> {
    let bytes = match key {
        "enter" => b"\r".to_vec(),
        // Word rubout: ^W. PSReadLine's default Windows mode binds Ctrl+w (not Alt+Backspace),
        // like readline and pi-tui; `\x08` reads as a plain Backspace in most TUIs.
        "backspace" if ctrl => vec![0x17],
        "backspace" => vec![0x7f],
        "tab" if shift => b"\x1b[Z".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => vec![0x1b],
        "space" if ctrl => vec![0x00],
        "up" => cursor_key(b'A', modifier, mode),
        "down" => cursor_key(b'B', modifier, mode),
        "right" => cursor_key(b'C', modifier, mode),
        "left" => cursor_key(b'D', modifier, mode),
        "home" => cursor_key(b'H', modifier, mode),
        "end" => cursor_key(b'F', modifier, mode),
        "insert" => tilde_key(2, modifier),
        "delete" => tilde_key(3, modifier),
        "pageup" => tilde_key(5, modifier),
        "pagedown" => tilde_key(6, modifier),
        _ => function_key(key)?(modifier),
    };
    Some(bytes)
}

fn cursor_key(code: u8, modifier: u8, mode: InputMode) -> Vec<u8> {
    if modifier > 1 {
        format!("\x1b[1;{modifier}{}", code as char).into_bytes()
    } else if mode.app_cursor {
        vec![0x1b, b'O', code]
    } else {
        vec![0x1b, b'[', code]
    }
}

fn tilde_key(number: u8, modifier: u8) -> Vec<u8> {
    if modifier > 1 {
        format!("\x1b[{number};{modifier}~").into_bytes()
    } else {
        format!("\x1b[{number}~").into_bytes()
    }
}

type Encoder = fn(u8) -> Vec<u8>;

/// F1–F12: SS3 P–S for F1–F4, `CSI n ~` above.
fn function_key(key: &str) -> Option<Encoder> {
    let encoder: Encoder = match key {
        "f1" => |m| ss3_key(b'P', m),
        "f2" => |m| ss3_key(b'Q', m),
        "f3" => |m| ss3_key(b'R', m),
        "f4" => |m| ss3_key(b'S', m),
        "f5" => |m| tilde_key(15, m),
        "f6" => |m| tilde_key(17, m),
        "f7" => |m| tilde_key(18, m),
        "f8" => |m| tilde_key(19, m),
        "f9" => |m| tilde_key(20, m),
        "f10" => |m| tilde_key(21, m),
        "f11" => |m| tilde_key(23, m),
        "f12" => |m| tilde_key(24, m),
        _ => return None,
    };
    Some(encoder)
}

fn ss3_key(code: u8, modifier: u8) -> Vec<u8> {
    if modifier > 1 {
        format!("\x1b[1;{modifier}{}", code as char).into_bytes()
    } else {
        vec![0x1b, b'O', code]
    }
}

/// C0 byte for Ctrl+key: letters → 0x01–0x1a, plus the classic punctuation aliases.
fn control_byte(key: &str) -> Option<u8> {
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    match c.to_ascii_lowercase() {
        c @ 'a'..='z' => Some(c as u8 & 0x1f),
        '@' | '2' => Some(0x00),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '/' | '7' => Some(0x1f),
        '8' | '?' => Some(0x7f),
        _ => None,
    }
}

fn with_alt(mut bytes: Vec<u8>, alt: bool) -> Vec<u8> {
    if alt {
        bytes.insert(0, 0x1b);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    const NORMAL: InputMode = InputMode {
        app_cursor: false,
        bracketed_paste: false,
    };
    const APP: InputMode = InputMode {
        app_cursor: true,
        bracketed_paste: true,
    };

    fn key(spec: &str, key_char: Option<&str>) -> Keystroke {
        let mut parts: Vec<&str> = spec.split('-').collect();
        let key = parts.pop().unwrap().to_string();
        let mut modifiers = Modifiers::default();
        for part in parts {
            match part {
                "ctrl" => modifiers.control = true,
                "alt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "win" => modifiers.platform = true,
                other => panic!("unknown modifier {other}"),
            }
        }
        Keystroke {
            modifiers,
            key,
            key_char: key_char.map(str::to_string),
        }
    }

    fn bytes(spec: &str, key_char: Option<&str>, mode: InputMode) -> Option<Vec<u8>> {
        key_to_bytes(&key(spec, key_char), mode)
    }

    #[test]
    fn printables_send_their_text() {
        assert_eq!(bytes("a", Some("a"), NORMAL), Some(b"a".to_vec()));
        assert_eq!(bytes("shift-a", Some("A"), NORMAL), Some(b"A".to_vec()));
        assert_eq!(bytes("space", Some(" "), NORMAL), Some(b" ".to_vec()));
        assert_eq!(bytes("e", Some("é"), NORMAL), Some("é".as_bytes().to_vec()));
    }

    #[test]
    fn only_plain_ctrl_c_is_the_copy_key() {
        assert!(is_copy_key(&key("ctrl-c", None)));
        assert!(
            !is_copy_key(&key("ctrl-shift-c", None)),
            "bound Copy, handled by the keymap"
        );
        assert!(!is_copy_key(&key("ctrl-alt-c", None)), "AltGr / Alt+^C");
        assert!(!is_copy_key(&key("c", Some("c"))));
        assert!(!is_copy_key(&key("ctrl-x", None)));
        // Without a selection it is still ^C for the program.
        assert_eq!(bytes("ctrl-c", None, NORMAL), Some(vec![0x03]));
    }

    #[test]
    fn a_selection_is_copied_even_from_an_exited_pane() {
        let ctrl_c = key("ctrl-c", None);
        assert_eq!(key_action(&ctrl_c, true, false, NORMAL), KeyAction::Copy);
        assert_eq!(key_action(&ctrl_c, true, true, NORMAL), KeyAction::Copy);
        assert_eq!(
            key_action(&ctrl_c, false, false, NORMAL),
            KeyAction::Send(vec![0x03])
        );
        assert_eq!(key_action(&ctrl_c, false, true, NORMAL), KeyAction::Ignore);
        let a = key("a", Some("a"));
        assert_eq!(key_action(&a, true, true, NORMAL), KeyAction::Ignore);
        assert_eq!(
            key_action(&a, true, false, NORMAL),
            KeyAction::Send(b"a".to_vec())
        );
    }

    #[test]
    fn editing_keys() {
        assert_eq!(bytes("enter", None, NORMAL), Some(b"\r".to_vec()));
        assert_eq!(bytes("backspace", None, NORMAL), Some(vec![0x7f]));
        // Word rubout: ^W, bound by PSReadLine (Windows mode: Ctrl+w), readline and pi-tui.
        // ESC DEL stays Alt+Backspace, which PSReadLine's Windows mode does not bind.
        assert_eq!(bytes("ctrl-backspace", None, NORMAL), Some(vec![0x17]));
        assert_eq!(
            bytes("alt-backspace", None, NORMAL),
            Some(b"\x1b\x7f".to_vec())
        );
        assert_eq!(bytes("tab", None, NORMAL), Some(b"\t".to_vec()));
        assert_eq!(bytes("shift-tab", None, NORMAL), Some(b"\x1b[Z".to_vec()));
        assert_eq!(bytes("escape", None, NORMAL), Some(vec![0x1b]));
        assert_eq!(bytes("delete", None, NORMAL), Some(b"\x1b[3~".to_vec()));
        assert_eq!(bytes("pageup", None, NORMAL), Some(b"\x1b[5~".to_vec()));
        assert_eq!(
            bytes("ctrl-delete", None, NORMAL),
            Some(b"\x1b[3;5~".to_vec())
        );
    }

    #[test]
    fn arrows_follow_cursor_mode_and_modifiers() {
        assert_eq!(bytes("up", None, NORMAL), Some(b"\x1b[A".to_vec()));
        assert_eq!(bytes("left", None, NORMAL), Some(b"\x1b[D".to_vec()));
        assert_eq!(bytes("up", None, APP), Some(b"\x1bOA".to_vec()));
        assert_eq!(bytes("home", None, APP), Some(b"\x1bOH".to_vec()));
        assert_eq!(bytes("end", None, NORMAL), Some(b"\x1b[F".to_vec()));
        assert_eq!(bytes("ctrl-right", None, APP), Some(b"\x1b[1;5C".to_vec()));
        assert_eq!(bytes("shift-up", None, NORMAL), Some(b"\x1b[1;2A".to_vec()));
        assert_eq!(bytes("alt-left", None, NORMAL), Some(b"\x1b[1;3D".to_vec()));
    }

    #[test]
    fn wheel_lines_become_arrow_presses() {
        assert_eq!(wheel_bytes(2, NORMAL), b"\x1b[A\x1b[A".to_vec());
        assert_eq!(wheel_bytes(-1, NORMAL), b"\x1b[B".to_vec());
        assert_eq!(wheel_bytes(-2, APP), b"\x1bOB\x1bOB".to_vec());
    }

    #[test]
    fn ctrl_letters_send_c0_bytes() {
        assert_eq!(bytes("ctrl-c", None, NORMAL), Some(vec![0x03]));
        assert_eq!(bytes("ctrl-a", None, NORMAL), Some(vec![0x01]));
        assert_eq!(bytes("ctrl-z", None, NORMAL), Some(vec![0x1a]));
        assert_eq!(bytes("ctrl-shift-d", None, NORMAL), Some(vec![0x04]));
        assert_eq!(bytes("ctrl-[", None, NORMAL), Some(vec![0x1b]));
        assert_eq!(bytes("ctrl-space", Some(" "), NORMAL), Some(vec![0x00]));
        assert_eq!(bytes("ctrl-alt-c", None, NORMAL), Some(vec![0x1b, 0x03]));
    }

    #[test]
    fn alt_prefixes_escape() {
        assert_eq!(bytes("alt-b", Some("b"), NORMAL), Some(b"\x1bb".to_vec()));
        assert_eq!(bytes("alt-enter", None, NORMAL), Some(b"\x1b\r".to_vec()));
    }

    #[test]
    fn altgr_chars_pass_through() {
        assert_eq!(bytes("ctrl-alt-q", Some("@"), NORMAL), Some(b"@".to_vec()));
    }

    #[test]
    fn function_keys() {
        assert_eq!(bytes("f1", None, NORMAL), Some(b"\x1bOP".to_vec()));
        assert_eq!(bytes("f5", None, NORMAL), Some(b"\x1b[15~".to_vec()));
        assert_eq!(bytes("f12", None, NORMAL), Some(b"\x1b[24~".to_vec()));
        assert_eq!(bytes("shift-f1", None, NORMAL), Some(b"\x1b[1;2P".to_vec()));
    }

    #[test]
    fn ignored_keys() {
        assert_eq!(bytes("win-a", Some("a"), NORMAL), None);
        assert_eq!(bytes("shift", None, NORMAL), None);
        assert_eq!(bytes("ctrl-f13", None, NORMAL), None);
    }

    #[test]
    fn paste_normalizes_newlines() {
        assert_eq!(paste_bytes("a\r\nb\nc", NORMAL), b"a\rb\rc".to_vec());
    }

    #[test]
    fn bracketed_paste_wraps_and_strips_escape() {
        assert_eq!(
            paste_bytes("x\n\x1b[201~y", APP),
            b"\x1b[200~x\r[201~y\x1b[201~".to_vec()
        );
    }
}
