//! Keyboard and paste input → bytes for the PTY (SPEC §10, §13). Pure, so it is unit-tested.
//!
//! Encodings follow xterm: CSI sequences for cursor/editing keys, `CSI 1;<mod>X` when modified,
//! SS3 for arrows in application-cursor mode, C0 control bytes for Ctrl+key, ESC prefix for Alt.
//! macOS Option composes unless `[terminal] option_as_meta` (ADR 0099).

use gpui::{App, Keystroke, Pixels, ScrollDelta};

use crate::keymap::Os;

/// Terminal modes that change what keys and pastes send.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputMode {
    /// DECCKM: arrows/Home/End send `ESC O x` instead of `ESC [ x`.
    pub app_cursor: bool,
    /// Pastes are wrapped in `ESC [200~` … `ESC [201~`.
    pub bracketed_paste: bool,
}

/// What the system and `[terminal] option_as_meta` change about key encoding (ADR 0099). A GPUI
/// global, set at start and whenever `settings.toml` changes, read by every pane on each key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEnv {
    pub os: Os,
    /// macOS: Option sends ESC plus the key (Meta) instead of the character it composes.
    pub option_as_meta: bool,
}

impl gpui::Global for KeyEnv {}

impl KeyEnv {
    /// This computer, with the setting's value.
    pub const fn current(option_as_meta: bool) -> Self {
        Self {
            os: Os::CURRENT,
            option_as_meta,
        }
    }

    /// The global; this computer's default until it is set.
    pub fn of(cx: &App) -> Self {
        cx.try_global::<Self>()
            .copied()
            .unwrap_or(Self::current(false))
    }
}

/// Bytes a key press sends to the PTY, or `None` if the terminal should ignore it.
///
/// On macOS an Option+key press is not ours unless `env.option_as_meta`: GPUI puts the composed
/// character in `key_char` (Option+B is "∫"), and the key must reach the input method so that
/// dead keys compose (Option+E, then E, is "é"). The input method types the result through
/// `ime::Typing`.
pub fn key_to_bytes(keystroke: &Keystroke, mode: InputMode, env: KeyEnv) -> Option<Vec<u8>> {
    let m = keystroke.modifiers;
    if m.platform {
        return None;
    }
    // xterm modifier parameter: 1 + shift + 2·alt + 4·ctrl.
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);

    if let Some(bytes) = special_key(&keystroke.key, m.control, m.shift, modifier, mode) {
        // Shift+Enter already starts with ESC: Alt adds nothing.
        let shift_enter = m.shift && keystroke.key == "enter";
        return Some(with_alt(
            bytes,
            m.alt && !is_csi(&keystroke.key) && !shift_enter,
        ));
    }
    if env.os == Os::Mac && m.alt && !m.control {
        return option_bytes(keystroke, env.option_as_meta);
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

/// Option+key on macOS (not Ctrl, not a special key): `None` = the input method composes it;
/// with `option_as_meta`, ESC plus the key as typed without Option (GPUI reports Shift+letter as
/// shift plus the lowercase letter, and Shift on any other key as the shifted char).
fn option_bytes(keystroke: &Keystroke, option_as_meta: bool) -> Option<Vec<u8>> {
    if !option_as_meta {
        return None;
    }
    let key = if keystroke.key == "space" {
        " "
    } else {
        keystroke.key.as_str()
    };
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let c = if keystroke.modifiers.shift {
        c.to_ascii_uppercase()
    } else {
        c
    };
    let mut bytes = vec![0x1b];
    bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    Some(bytes)
}

/// Plain Ctrl+C on Windows and Linux: copies the selection when there is one, else it is ^C for
/// the program (like Windows Terminal, ADR 0022). Never on macOS, where Ctrl+C is always ^C and
/// Cmd+C is the keymap's Copy (ADR 0099).
pub fn is_copy_key(keystroke: &Keystroke, os: Os) -> bool {
    let m = keystroke.modifiers;
    os != Os::Mac && keystroke.key == "c" && m.control && !m.shift && !m.alt && !m.platform
}

/// Plain Ctrl+V on Windows and Linux: the pane pastes the clipboard; ^V goes to the program only
/// when there is nothing to paste (ADR 0026). Never on macOS: Ctrl+V is ^V, Cmd+V pastes.
pub fn is_paste_key(keystroke: &Keystroke, os: Os) -> bool {
    let m = keystroke.modifiers;
    os != Os::Mac && keystroke.key == "v" && m.control && !m.shift && !m.alt && !m.platform
}

/// What a key press in a terminal pane does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Copy the selection (plain Ctrl+C while text is selected, also in an exited pane; not on
    /// macOS).
    Copy,
    /// Paste the clipboard (plain Ctrl+V in a live pane, ADR 0026; not on macOS).
    Paste,
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
    env: KeyEnv,
) -> KeyAction {
    if selected && is_copy_key(keystroke, env.os) {
        return KeyAction::Copy;
    }
    if !exited && is_paste_key(keystroke, env.os) {
        return KeyAction::Paste;
    }
    match key_to_bytes(keystroke, mode, env) {
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

/// Lines a trackpad's pixel scrolling counts per line of travel when they go to a program as
/// arrow keys or wheel reports: one swipe with momentum is dozens of lines, and each one would
/// be a key press for `less` or an agent TUI. Scrolling the history stays 1 to 1.
// ponytail: a fixed 0.5 tuned by reading, not by feel; make it `[terminal]` setting if a Mac
// user finds it too slow or too fast (docs/HUMAN_CHECKS.md).
pub const PIXEL_WHEEL_SCALE: f32 = 0.5;

/// Whole wheel lines of one scroll event (positive = up, into the history) and the fraction to
/// carry to the next one. `to_program`: the lines become wheel reports or arrow keys, not
/// history scrolling. A notched wheel gives lines as they are; precise (pixel) deltas are
/// scaled by [`PIXEL_WHEEL_SCALE`] when `to_program`. The carry of the other direction is
/// dropped: it is not part of this swipe.
pub fn wheel_lines(
    delta: ScrollDelta,
    line_height: Pixels,
    to_program: bool,
    carry: f32,
) -> (i32, f32) {
    let lines = match delta {
        ScrollDelta::Lines(lines) => lines.y,
        ScrollDelta::Pixels(pixels) => {
            let lines = f32::from(pixels.y) / f32::from(line_height);
            if to_program {
                lines * PIXEL_WHEEL_SCALE
            } else {
                lines
            }
        }
    };
    let carry = if carry * lines < 0.0 { 0.0 } else { carry };
    let total = lines + carry;
    // Whole lines of one wheel event: far inside i32.
    (total.trunc() as i32, total.fract())
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

/// Mouse tracking: `lines` wheel lines (positive = up) as that many xterm wheel button reports
/// (64 up, 65 down) at the 0-based cell `col`, `row`. SGR form when `sgr`, else the legacy
/// `CSI M` bytes, whose coordinates stop at 223.
pub fn wheel_report_bytes(lines: i32, col: usize, row: usize, sgr: bool) -> Vec<u8> {
    let button: u8 = if lines > 0 { 64 } else { 65 };
    let report = if sgr {
        format!("\x1b[<{button};{};{}M", col + 1, row + 1).into_bytes()
    } else {
        // 32 + the 1-based coordinate; `min(222)` keeps it ≤ 255, so the cast cannot truncate.
        let coord = |n: usize| (n.min(222) + 33) as u8;
        vec![0x1b, b'[', b'M', 32 + button, coord(col), coord(row)]
    };
    report.repeat(lines.unsigned_abs() as usize)
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
        // Shift+Enter: the Alt+Enter form, which agents read as "new line" (Claude, Codex,
        // Copilot, omp). Without kitty / win32-input mode nothing else reaches them through
        // ConPTY intact; shells see Alt+Enter, which pwsh and readline leave unbound.
        "enter" if shift => vec![0x1b, b'\r'],
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
    use gpui::{Modifiers, ScrollDelta, point, px};

    const NORMAL: InputMode = InputMode {
        app_cursor: false,
        bracketed_paste: false,
    };
    const APP: InputMode = InputMode {
        app_cursor: true,
        bracketed_paste: true,
    };
    const WIN: KeyEnv = KeyEnv {
        os: Os::Windows,
        option_as_meta: false,
    };
    const MAC: KeyEnv = KeyEnv {
        os: Os::Mac,
        option_as_meta: false,
    };
    const MAC_META: KeyEnv = KeyEnv {
        os: Os::Mac,
        option_as_meta: true,
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
        key_to_bytes(&key(spec, key_char), mode, WIN)
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
        assert!(is_copy_key(&key("ctrl-c", None), Os::Windows));
        assert!(
            !is_copy_key(&key("ctrl-shift-c", None), Os::Windows),
            "bound Copy, handled by the keymap"
        );
        assert!(
            !is_copy_key(&key("ctrl-alt-c", None), Os::Windows),
            "AltGr / Alt+^C"
        );
        assert!(!is_copy_key(&key("c", Some("c")), Os::Windows));
        assert!(!is_copy_key(&key("ctrl-x", None), Os::Windows));
        // Without a selection it is still ^C for the program.
        assert_eq!(bytes("ctrl-c", None, NORMAL), Some(vec![0x03]));
    }

    #[test]
    fn a_selection_is_copied_even_from_an_exited_pane() {
        let ctrl_c = key("ctrl-c", None);
        assert_eq!(
            key_action(&ctrl_c, true, false, NORMAL, WIN),
            KeyAction::Copy
        );
        assert_eq!(
            key_action(&ctrl_c, true, true, NORMAL, WIN),
            KeyAction::Copy
        );
        assert_eq!(
            key_action(&ctrl_c, false, false, NORMAL, WIN),
            KeyAction::Send(vec![0x03])
        );
        assert_eq!(
            key_action(&ctrl_c, false, true, NORMAL, WIN),
            KeyAction::Ignore
        );
        let a = key("a", Some("a"));
        assert_eq!(key_action(&a, true, true, NORMAL, WIN), KeyAction::Ignore);
        assert_eq!(
            key_action(&a, true, false, NORMAL, WIN),
            KeyAction::Send(b"a".to_vec())
        );
    }

    #[test]
    fn plain_ctrl_v_is_paste_only_while_live() {
        let ctrl_v = key("ctrl-v", None);
        assert_eq!(
            key_action(&ctrl_v, false, false, NORMAL, WIN),
            KeyAction::Paste
        );
        // A selection does not change it (Ctrl+C copies, Ctrl+V pastes).
        assert_eq!(
            key_action(&ctrl_v, true, false, NORMAL, WIN),
            KeyAction::Paste
        );
        assert_eq!(
            key_action(&ctrl_v, false, true, NORMAL, WIN),
            KeyAction::Ignore
        );
        // Ctrl+Shift+V stays the keymap's; AltGr (ctrl-alt) and Win+V are not paste.
        for other in ["ctrl-shift-v", "ctrl-alt-v", "win-v", "v"] {
            assert_ne!(
                key_action(&key(other, None), false, false, NORMAL, WIN),
                KeyAction::Paste,
                "{other}"
            );
        }
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
    fn wheel_lines_become_mouse_wheel_reports() {
        assert_eq!(
            wheel_report_bytes(2, 4, 9, true),
            b"\x1b[<64;5;10M\x1b[<64;5;10M".to_vec()
        );
        assert_eq!(
            wheel_report_bytes(-1, 0, 0, true),
            b"\x1b[<65;1;1M".to_vec()
        );
        assert_eq!(
            wheel_report_bytes(-1, 4, 9, false),
            vec![0x1b, b'[', b'M', 32 + 65, 32 + 5, 32 + 10]
        );
        assert_eq!(
            wheel_report_bytes(1, 500, 0, false),
            vec![0x1b, b'[', b'M', 32 + 64, 255, 33],
            "legacy coordinates clamp"
        );
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

    /// Shift+Enter is the Alt+Enter form (ESC CR): Claude, Codex, Copilot and omp insert a
    /// new line instead of submitting; plain and Ctrl+Enter still submit.
    #[test]
    fn shift_enter_is_a_newline_for_agents() {
        for mode in [NORMAL, APP] {
            assert_eq!(bytes("shift-enter", None, mode), Some(b"\x1b\r".to_vec()));
            // One ESC, not two (ESC ESC CR would read as Esc, then Alt+Enter).
            assert_eq!(
                bytes("alt-shift-enter", None, mode),
                Some(b"\x1b\r".to_vec())
            );
            assert_eq!(bytes("enter", None, mode), Some(b"\r".to_vec()));
            assert_eq!(bytes("ctrl-enter", None, mode), Some(b"\r".to_vec()));
        }
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

    #[test]
    fn mac_ctrl_c_and_ctrl_v_are_never_copy_or_paste() {
        // ADR 0099: on a Mac Ctrl+C interrupts even over a selection, Ctrl+V is ^V.
        // Cmd+C / Cmd+V are the keymap's Copy and Paste.
        let ctrl_c = key("ctrl-c", None);
        assert!(!is_copy_key(&ctrl_c, Os::Mac));
        assert_eq!(
            key_action(&ctrl_c, true, false, NORMAL, MAC),
            KeyAction::Send(vec![0x03])
        );
        let ctrl_v = key("ctrl-v", None);
        assert!(!is_paste_key(&ctrl_v, Os::Mac));
        assert_eq!(
            key_action(&ctrl_v, false, false, NORMAL, MAC),
            KeyAction::Send(vec![0x16])
        );
        // Windows and Linux keep ADR 0022 and ADR 0026.
        for os in [Os::Windows, Os::Linux] {
            assert!(is_copy_key(&ctrl_c, os), "{os:?}");
            assert!(is_paste_key(&ctrl_v, os), "{os:?}");
        }
    }

    #[test]
    fn mac_cmd_keys_are_not_for_the_program() {
        // A bound Cmd key never gets here (the keymap spends it); an unbound one is ignored.
        for spec in ["win-c", "win-v", "win-k", "win-a"] {
            let keystroke = key(spec, None);
            assert_eq!(
                key_action(&keystroke, true, false, NORMAL, MAC),
                KeyAction::Ignore,
                "{spec}"
            );
        }
    }

    #[test]
    fn mac_option_composes_by_default() {
        // Option+B is "∫", Option+2 "€", Option+E the dead accent "´": the key press sends
        // nothing, so GPUI hands it to the input method, which types the result (ime.rs).
        for (spec, composed) in [("alt-b", "∫"), ("alt-2", "€"), ("alt-e", "´")] {
            let keystroke = key(spec, Some(composed));
            assert_eq!(key_to_bytes(&keystroke, NORMAL, MAC), None, "{spec}");
            assert_eq!(
                key_action(&keystroke, false, false, NORMAL, MAC),
                KeyAction::Ignore,
                "{spec}"
            );
        }
        // Keys that are not text keep their encoding.
        assert_eq!(
            key_to_bytes(&key("alt-left", None), NORMAL, MAC),
            Some(b"\x1b[1;3D".to_vec())
        );
        assert_eq!(
            key_to_bytes(&key("alt-backspace", None), NORMAL, MAC),
            Some(b"\x1b\x7f".to_vec())
        );
        assert_eq!(
            key_to_bytes(&key("alt-enter", None), NORMAL, MAC),
            Some(b"\x1b\r".to_vec())
        );
        // Windows is unchanged: Alt prefixes ESC.
        assert_eq!(
            key_to_bytes(&key("alt-b", Some("b")), NORMAL, WIN),
            Some(b"\x1bb".to_vec())
        );
    }

    #[test]
    fn mac_option_as_meta_sends_escape_and_the_plain_key() {
        let cases: [(&str, &str, &[u8]); 6] = [
            ("alt-b", "∫", b"\x1bb"),
            ("alt-f", "ƒ", b"\x1bf"),
            // GPUI reports Shift+letter as shift plus the lowercase letter.
            ("alt-shift-b", "ı", b"\x1bB"),
            ("alt-1", "¡", b"\x1b1"),
            // Shift on a non-letter arrives as the shifted char with shift off.
            ("alt-!", "⁄", b"\x1b!"),
            ("alt-space", " ", b"\x1b "),
        ];
        for (spec, composed, expected) in cases {
            assert_eq!(
                key_to_bytes(&key(spec, Some(composed)), NORMAL, MAC_META),
                Some(expected.to_vec()),
                "{spec}"
            );
        }
        // Ctrl+Option keeps its C0 form.
        assert_eq!(
            key_to_bytes(&key("ctrl-alt-c", None), NORMAL, MAC_META),
            Some(vec![0x1b, 0x03])
        );
        // The setting does nothing on other systems.
        let win_meta = KeyEnv {
            os: Os::Windows,
            option_as_meta: true,
        };
        assert_eq!(
            key_to_bytes(&key("ctrl-alt-q", Some("@")), NORMAL, win_meta),
            Some(b"@".to_vec()),
            "AltGr still types"
        );
        assert_eq!(
            key_to_bytes(&key("alt-b", Some("b")), NORMAL, win_meta),
            Some(b"\x1bb".to_vec())
        );
    }

    fn pixels(y: f32) -> ScrollDelta {
        ScrollDelta::Pixels(point(px(0.0), px(y)))
    }

    #[test]
    fn a_notched_wheel_is_whole_lines_for_the_history_and_for_programs() {
        let lines = ScrollDelta::Lines(point(0.0, 3.0));
        assert_eq!(wheel_lines(lines, px(20.0), false, 0.0), (3, 0.0));
        assert_eq!(wheel_lines(lines, px(20.0), true, 0.0), (3, 0.0));
    }

    #[test]
    fn trackpad_pixels_move_a_program_half_as_far_as_the_history() {
        // 46 px of a 23 px line: two lines of history, one wheel report or arrow key.
        assert_eq!(wheel_lines(pixels(46.0), px(23.0), false, 0.0), (2, 0.0));
        assert_eq!(wheel_lines(pixels(46.0), px(23.0), true, 0.0), (1, 0.0));
        assert_eq!(wheel_lines(pixels(-46.0), px(23.0), true, 0.0), (-1, 0.0));
        assert_eq!(PIXEL_WHEEL_SCALE, 0.5);
    }

    #[test]
    fn a_fraction_carries_in_the_same_direction() {
        assert_eq!(wheel_lines(pixels(10.0), px(20.0), false, 0.0), (0, 0.5));
        assert_eq!(wheel_lines(pixels(15.0), px(20.0), false, 0.5), (1, 0.25));
        // No movement keeps what was carried.
        assert_eq!(wheel_lines(pixels(0.0), px(20.0), false, 0.5), (0, 0.5));
    }

    #[test]
    fn a_change_of_direction_drops_the_carried_fraction() {
        // Half a line up was carried; the first move down starts from zero, not from +0.5.
        assert_eq!(wheel_lines(pixels(-10.0), px(20.0), false, 0.5), (0, -0.5));
        assert_eq!(wheel_lines(pixels(10.0), px(20.0), false, -0.5), (0, 0.5));
        // 1.25 lines down after 0.5 up: one line, not zero.
        assert_eq!(
            wheel_lines(pixels(-25.0), px(20.0), false, 0.5),
            (-1, -0.25)
        );
    }
}
