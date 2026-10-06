//! The Git view's base menu (SPEC §12B): type the ref "Against base" compares with, or go back
//! to auto-detection (`origin/HEAD` → `main` → `master`). Confirming sends `git.set_base`.
//! Pure and tested headlessly; `main.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{GitSetBaseParams, GitStatus};
use ulid::Ulid;

use crate::task_dialog::{DialogKey, dialog_key};

/// What a key does in the base menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Close,
    /// Enter: use the typed ref (empty = auto-detect).
    Save,
    Backspace,
    Paste,
    Insert(String),
}

pub fn key(keystroke: &Keystroke) -> Option<Key> {
    match dialog_key(keystroke)? {
        DialogKey::Cancel => Some(Key::Close),
        DialogKey::Enter | DialogKey::Save => Some(Key::Save),
        DialogKey::Backspace => Some(Key::Backspace),
        DialogKey::Paste => Some(Key::Paste),
        DialogKey::Insert(text) => Some(Key::Insert(text)),
        DialogKey::SwitchField => None,
    }
}

/// The open base menu of one workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseDialog {
    pub project_id: Ulid,
    /// The ref being typed; starts as the configured base (empty when auto-detected).
    pub text: String,
    /// The base in use when the menu opened, as the menu describes it.
    pub current: String,
    pub busy: bool,
    pub error: Option<String>,
}

impl BaseDialog {
    /// The menu for the shown status (`None` while it is loading).
    pub fn new(project_id: Ulid, status: Option<&GitStatus>) -> BaseDialog {
        let base = status.and_then(|status| status.base_ref.clone());
        let auto = status.is_none_or(|status| status.base_auto);
        let current = match (&base, auto) {
            (Some(base), true) => format!("Now: {base} (auto-detected)"),
            (Some(base), false) => format!("Now: {base}"),
            (None, _) if status.is_some() => "No base branch found.".to_owned(),
            (None, _) => String::new(),
        };
        BaseDialog {
            project_id,
            text: if auto {
                String::new()
            } else {
                base.unwrap_or_default()
            },
            current,
            busy: false,
            error: None,
        }
    }

    /// Edits the typed ref; `true` when it changed.
    pub fn key(&mut self, key: &Key) -> bool {
        match key {
            Key::Backspace => self.text.pop().is_some(),
            Key::Insert(text) => self.insert(text),
            Key::Close | Key::Save | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text (a ref is one line). `true` when it changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let before = self.text.len();
        self.text
            .extend(text.chars().filter(|c| *c != '\r' && *c != '\n'));
        self.text.len() != before
    }

    /// The `git.set_base` to send: the typed ref, or auto-detection when `auto` or nothing is
    /// typed. `None` while one is in flight.
    pub fn begin(&mut self, auto: bool) -> Option<GitSetBaseParams> {
        if self.busy {
            return None;
        }
        self.busy = true;
        self.error = None;
        let typed = self.text.trim();
        Some(GitSetBaseParams {
            project_id: self.project_id,
            base_ref: (!auto && !typed.is_empty()).then(|| typed.to_owned()),
        })
    }

    /// The `git.set_base` failed (e.g. not a valid ref name); the menu stays open.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(base: Option<&str>, auto: bool) -> GitStatus {
        GitStatus {
            base_ref: base.map(Into::into),
            base_auto: auto,
            ..GitStatus::default()
        }
    }

    #[test]
    fn opens_with_the_configured_base_typed_and_describes_the_current_one() {
        let id = Ulid::new();
        let set = BaseDialog::new(id, Some(&status(Some("develop"), false)));
        assert_eq!(set.text, "develop");
        assert_eq!(set.current, "Now: develop");
        let auto = BaseDialog::new(id, Some(&status(Some("main"), true)));
        assert_eq!(auto.text, "");
        assert_eq!(auto.current, "Now: main (auto-detected)");
        let none = BaseDialog::new(id, Some(&status(None, true)));
        assert_eq!(none.current, "No base branch found.");
        assert_eq!(BaseDialog::new(id, None).current, "");
    }

    #[test]
    fn typing_edits_one_line_and_enter_sends_the_trimmed_ref() {
        let id = Ulid::new();
        let mut dialog = BaseDialog::new(id, Some(&status(None, true)));
        assert!(dialog.key(&Key::Insert("origin/dev".into())));
        assert!(dialog.key(&Key::Backspace));
        assert!(dialog.insert(" x\r\n"));
        assert!(!dialog.insert("\n"));
        assert_eq!(dialog.text, "origin/de x");
        dialog.text = "  release ".into();
        let params = dialog.begin(false).unwrap();
        assert_eq!(params.project_id, id);
        assert_eq!(params.base_ref.as_deref(), Some("release"));
        // One at a time.
        assert!(dialog.begin(false).is_none());
        dialog.failed("bad ref".into());
        assert!(!dialog.busy);
        assert_eq!(dialog.error.as_deref(), Some("bad ref"));
        assert_eq!(
            dialog.begin(false).unwrap().base_ref.as_deref(),
            Some("release")
        );
        assert_eq!(dialog.error, None);
    }

    #[test]
    fn auto_detect_or_an_empty_ref_clears_the_base() {
        let id = Ulid::new();
        let mut dialog = BaseDialog::new(id, Some(&status(Some("develop"), false)));
        assert_eq!(dialog.begin(true).unwrap().base_ref, None);
        let mut empty = BaseDialog::new(id, Some(&status(Some("main"), true)));
        empty.text = "   ".into();
        assert_eq!(empty.begin(false).unwrap().base_ref, None);
    }

    #[test]
    fn keys() {
        let parse = |keys: &str| key(&Keystroke::parse(keys).unwrap());
        assert_eq!(parse("escape"), Some(Key::Close));
        assert_eq!(parse("enter"), Some(Key::Save));
        assert_eq!(parse("backspace"), Some(Key::Backspace));
        assert_eq!(parse("ctrl-v"), Some(Key::Paste));
        assert_eq!(parse("tab"), None);
        assert_eq!(parse("ctrl-x"), None);
    }
}
