//! The keys a page gives back to Mkrew (spec 7.4): while a web view has the keyboard GPUI
//! sees nothing, so an init script catches Mkrew's combos and posts them over IPC.

use crate::keymap::{BROWSER_CONTEXT, Keymap};
use gpui::Keystroke;

/// `keystroke` as the init script names a `keydown`: `ctrl-`, `alt-`, `shift-`, `cmd-`, then
/// the key, so both sides agree whatever order the keymap wrote.
pub fn key_name(keystroke: &Keystroke) -> String {
    let m = &keystroke.modifiers;
    let mut name = String::new();
    for (on, prefix) in [
        (m.control, "ctrl-"),
        (m.alt, "alt-"),
        (m.shift, "shift-"),
        (m.platform, "cmd-"),
    ] {
        if on {
            name.push_str(prefix);
        }
    }
    name.push_str(&keystroke.key);
    name
}

/// Ctrl / Cmd + these edit text in a page's fields: they never leave the page.
const TEXT_KEYS: [&str; 6] = ["a", "c", "v", "x", "y", "z"];

/// The combos a page posts back: every global binding and every browser pane binding that
/// holds Ctrl, Alt or Cmd (or is an F-key), minus the text editing keys.
pub fn forwarded(keymap: &Keymap) -> Vec<String> {
    let mut keys: Vec<String> = keymap
        .bindings()
        .iter()
        .filter(|(command, _)| matches!(command.context(), None | Some(BROWSER_CONTEXT)))
        .map(|(_, keystroke)| keystroke)
        .filter(|keystroke| {
            let m = &keystroke.modifiers;
            let function = keystroke.key.starts_with('f') && keystroke.key.len() > 1;
            let modified = m.control || m.alt || m.platform;
            let text = (m.control || m.platform)
                && !m.alt
                && !m.shift
                && TEXT_KEYS.contains(&keystroke.key.as_str());
            (modified || function) && !text
        })
        .map(key_name)
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Runs in every page before its own scripts: posts focus, and posts and swallows the keys.
pub fn init_script(keys: &[String]) -> String {
    format!(
        r#"(() => {{
  const post = (m) => window.ipc.postMessage(JSON.stringify(m));
  window.__mkrewKeys = new Set({keys});
  const codes = {{ ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down",
    Enter: "enter", Escape: "escape", Space: "space", Tab: "tab", Backspace: "backspace",
    Delete: "delete", Home: "home", End: "end", PageUp: "pageup", PageDown: "pagedown",
    Equal: "=", Minus: "-", Comma: ",", Period: ".", Slash: "/", Backslash: "\\",
    BracketLeft: "[", BracketRight: "]", Semicolon: ";", Quote: "'", Backquote: "`" }};
  const key = (code) => code.startsWith("Key") ? code.slice(3).toLowerCase()
    : code.startsWith("Digit") ? code.slice(5)
    : /^F\d+$/.test(code) ? code.toLowerCase()
    : codes[code];
  addEventListener("mousedown", () => post({{ t: "focus" }}), true);
  addEventListener("focusin", () => post({{ t: "focus" }}), true);
  addEventListener("keydown", (e) => {{
    const k = key(e.code);
    if (!k) return;
    const name = (e.ctrlKey ? "ctrl-" : "") + (e.altKey ? "alt-" : "")
      + (e.shiftKey ? "shift-" : "") + (e.metaKey ? "cmd-" : "") + k;
    if (!window.__mkrewKeys.has(name)) return;
    e.preventDefault();
    e.stopPropagation();
    post({{ t: "key", k: name }});
  }}, true);
}})();"#,
        keys = json_list(keys)
    )
}

/// Replaces the key list in a live page (the keymap changed, ADR 0073).
pub fn set_keys_script(keys: &[String]) -> String {
    format!("window.__mkrewKeys = new Set({});", json_list(keys))
}

fn json_list(keys: &[String]) -> String {
    serde_json::to_string(keys).unwrap_or_else(|_| "[]".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{Keymap, Os};
    use gpui::Keystroke;

    #[test]
    fn key_names_put_modifiers_in_one_order() {
        let name = |keys| key_name(&Keystroke::parse(keys).unwrap());
        assert_eq!(name("shift-alt-b"), "alt-shift-b");
        assert_eq!(name("cmd-alt-b"), "alt-cmd-b");
        assert_eq!(name("ctrl-shift-p"), "ctrl-shift-p");
        assert_eq!(name("f11"), "f11");
    }

    #[test]
    fn the_page_gives_back_global_combos_and_keeps_its_text_keys() {
        let keymap = Keymap::defaults_for(Os::Windows);
        let keys = forwarded(&keymap);
        // Browser commands (SplitBrowser alt-shift-b, BrowserAddress ctrl-l) arrive with Task 4;
        // global ones stand in.
        assert!(keys.contains(&"alt-shift-a".to_owned()), "SplitAgent");
        assert!(
            keys.contains(&"alt-left".to_owned()),
            "Focus left is global"
        );
        // Terminal and editor bindings stay with the page.
        assert!(!keys.iter().any(|k| k == "ctrl-shift-c"), "terminal Copy");
        assert!(!keys.iter().any(|k| k == "ctrl-s"), "editor Save");
        // Text editing keys never leave the page, even if bound globally.
        for text in ["ctrl-c", "ctrl-v", "ctrl-x", "ctrl-a", "ctrl-z", "ctrl-y"] {
            assert!(!keys.iter().any(|k| k == text), "{text}");
        }
        // An unmodified key never leaves the page (except F-keys).
        assert!(keys.iter().all(|k| k.contains('-') || k.starts_with('f')));
    }

    #[test]
    fn the_scripts_carry_the_key_list() {
        let keys = ["alt-shift-b".to_owned()];
        let script = init_script(&keys);
        assert!(script.contains(r#"["alt-shift-b"]"#));
        assert!(script.contains("window.ipc.postMessage"));
        assert!(set_keys_script(&keys).contains(r#"["alt-shift-b"]"#));
    }
}
