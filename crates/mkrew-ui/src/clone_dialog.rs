//! The "Add workspace" dialog (ADR 0076): open a local folder, or clone a Git repository
//! (GitHub, GitLab, any URL `git clone` takes) into a folder and add the clone. Confirming
//! sends `project.clone`; its end arrives as `project.cloned`. Pure and tested headlessly.

use std::path::Path;

use mkrew_protocol::{ProjectCloneParams, ProjectClonedEvent};
use ulid::Ulid;

use crate::base_dialog::Key;
use crate::text_box::Input;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneDialog {
    /// The repository URL being typed.
    pub url: Input,
    /// The folder the clone goes in (the clone is a new folder inside it).
    pub parent: String,
    /// The `project.clone` in flight.
    pub cloning: Option<Ulid>,
    pub error: Option<String>,
}

impl CloneDialog {
    pub fn new(parent: String) -> CloneDialog {
        CloneDialog {
            url: Input::default(),
            parent,
            cloning: None,
            error: None,
        }
    }

    /// Edits the URL; `true` when it changed.
    pub fn key(&mut self, key: &Key) -> bool {
        match key {
            Key::Edit(key) => self.url.edit(key),
            Key::Insert(text) => self.insert(text),
            Key::Close | Key::Save | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text (a URL is one line). `true` when it changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        self.url.insert(&line)
    }

    /// Where the clone will land, once the URL names a repository.
    pub fn target(&self) -> Option<String> {
        let name = mkrew_core::git::repo_dir_name(&self.url)?;
        Some(Path::new(&self.parent).join(name).display().to_string())
    }

    /// The `project.clone` to send; `None` while one is in flight or without a usable URL
    /// (then `error` says why).
    pub fn begin(&mut self) -> Option<ProjectCloneParams> {
        if self.cloning.is_some() {
            return None;
        }
        if self.target().is_none() {
            self.error = Some("Type the repository URL, e.g. https://github.com/owner/repo".into());
            return None;
        }
        let clone_id = Ulid::new();
        self.cloning = Some(clone_id);
        self.error = None;
        Some(ProjectCloneParams {
            clone_id,
            url: self.url.trim().to_owned(),
            parent: self.parent.clone(),
        })
    }

    /// The request was refused, or the clone failed; the dialog stays open.
    pub fn failed(&mut self, error: String) {
        self.cloning = None;
        self.error = Some(error);
    }

    /// Whether `event` ends this dialog's clone.
    pub fn is_mine(&self, event: &ProjectClonedEvent) -> bool {
        self.cloning == Some(event.clone_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_needs_a_repository_url_and_runs_one_at_a_time() {
        let mut dialog = CloneDialog::new(r"C:\work".into());
        assert_eq!(dialog.begin(), None);
        assert!(dialog.error.is_some());
        assert!(dialog.insert(" git@github.com:o/repo.git\r\n"));
        assert_eq!(
            dialog.target(),
            Some(Path::new(r"C:\work").join("repo").display().to_string())
        );
        let params = dialog.begin().unwrap();
        assert_eq!(params.url, "git@github.com:o/repo.git");
        assert_eq!(params.parent, r"C:\work");
        assert_eq!(dialog.error, None);
        assert_eq!(dialog.begin(), None, "one clone at a time");
        let done = |clone_id| ProjectClonedEvent {
            clone_id,
            project: None,
            error: Some("not found".into()),
        };
        assert!(dialog.is_mine(&done(params.clone_id)));
        assert!(!dialog.is_mine(&done(Ulid::new())));
        dialog.failed("not found".into());
        assert_eq!(dialog.error.as_deref(), Some("not found"));
        assert!(dialog.begin().is_some(), "a failed clone can be retried");
    }
}
