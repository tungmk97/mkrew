//! "Link plan" in a machine window (spec §6.4): the plan file is on the other computer, so the
//! OS file picker gives way to a typed path. Confirming sends `session.link_plan`; the daemon
//! checks the file and its refusal shows under the field. Pure and tested headlessly.

use mkrew_protocol::SessionLinkPlanParams;
use ulid::Ulid;

use crate::base_dialog::Key;
use crate::text_box::Input;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDialog {
    pub session_id: Ulid,
    /// The machine whose computer holds the file.
    pub machine: String,
    pub path: Input,
    /// The `session.link_plan` in flight.
    pub busy: bool,
    pub error: Option<String>,
}

impl PlanDialog {
    pub fn new(session_id: Ulid, machine: String) -> Self {
        Self {
            session_id,
            machine,
            path: Input::default(),
            busy: false,
            error: None,
        }
    }

    /// Edits the path; `true` when it changed.
    pub fn key(&mut self, key: &Key) -> bool {
        match key {
            Key::Edit(key) => self.path.edit(key),
            Key::Insert(text) => self.insert(text),
            Key::Close | Key::Save | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text (a path is one line). `true` when it changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        self.path.insert(&line)
    }

    /// The `session.link_plan` to send; `None` while one is in flight or nothing is typed
    /// (then `error` says so).
    pub fn begin(&mut self) -> Option<SessionLinkPlanParams> {
        if self.busy {
            return None;
        }
        let path = self.path.trim();
        if path.is_empty() {
            self.error = Some(format!("Type the plan file on {}.", self.machine));
            return None;
        }
        let path = path.to_owned();
        self.busy = true;
        self.error = None;
        Some(SessionLinkPlanParams {
            session_id: self.session_id,
            path: Some(path),
        })
    }

    /// The daemon refused (no such file, not Markdown, …); the dialog stays open.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_path_is_sent_trimmed_one_at_a_time_and_refusals_stay_in_the_dialog() {
        let session = Ulid::new();
        let mut dialog = PlanDialog::new(session, "Home PC".into());
        assert_eq!(dialog.begin(), None);
        assert_eq!(
            dialog.error.as_deref(),
            Some("Type the plan file on Home PC.")
        );

        assert!(dialog.insert(" /srv/app/docs/plan.md\r\n"));
        let params = dialog.begin().unwrap();
        assert_eq!(
            params,
            SessionLinkPlanParams {
                session_id: session,
                path: Some("/srv/app/docs/plan.md".into()),
            }
        );
        assert_eq!(dialog.error, None);
        assert_eq!(dialog.begin(), None, "one request at a time");

        dialog.failed("No such file: /srv/app/docs/plan.md".into());
        assert!(!dialog.busy);
        assert_eq!(
            dialog.error.as_deref(),
            Some("No such file: /srv/app/docs/plan.md")
        );
        assert!(
            dialog.begin().is_some(),
            "a refused path can be corrected and sent again"
        );
    }
}
