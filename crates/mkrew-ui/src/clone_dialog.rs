//! The "Add workspace" dialog (ADR 0076): open a local folder, or clone a Git repository
//! (GitHub, GitLab, any URL `git clone` takes) into a folder and add the clone. Confirming
//! sends `project.clone`; its end arrives as `project.cloned`. Pure and tested headlessly.
//!
//! In a machine window (spec §6.4) the folders are on the other computer, so the OS pickers
//! give way to text fields: the folder to add and the folder to clone into are typed, and the
//! daemon validates them.

use std::path::Path;

use mkrew_protocol::{ProjectCloneParams, ProjectClonedEvent};
use ulid::Ulid;

use crate::base_dialog::Key;
use crate::text_box::Input;

/// A text field of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The folder to add (machine window only).
    Folder,
    Url,
    /// The folder the clone goes in (machine window only: else the picker's).
    Parent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneDialog {
    /// The repository URL being typed.
    pub url: Input,
    /// The folder the clone goes in (the clone is a new folder inside it).
    pub parent: Input,
    /// The folder to add, typed (machine window).
    pub folder: Input,
    /// The machine of the window: its folders are typed, not picked. `None` = this computer.
    pub machine: Option<String>,
    /// The field the keyboard edits.
    pub field: Field,
    /// The `project.clone` in flight.
    pub cloning: Option<Ulid>,
    pub error: Option<String>,
}

impl CloneDialog {
    pub fn new(parent: String, machine: Option<String>) -> CloneDialog {
        CloneDialog {
            url: Input::default(),
            parent: parent.into(),
            folder: Input::default(),
            field: if machine.is_some() {
                Field::Folder
            } else {
                Field::Url
            },
            machine,
            cloning: None,
            error: None,
        }
    }

    /// The fields in Tab order.
    fn fields(&self) -> &'static [Field] {
        if self.machine.is_some() {
            &[Field::Folder, Field::Url, Field::Parent]
        } else {
            &[Field::Url]
        }
    }

    /// Tab (`forward`) / Shift+Tab has no field left in that direction: the keyboard leaves
    /// the text fields for the buttons ([`crate::dialog_focus::nav`]).
    pub fn at_edge(&self, forward: bool) -> bool {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.field).unwrap_or(0);
        if forward {
            at + 1 >= fields.len()
        } else {
            at == 0
        }
    }

    /// Tab / Shift+Tab between the fields (not at an edge).
    pub fn step(&mut self, forward: bool) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.field).unwrap_or(0);
        let to = if forward { at + 1 } else { at.wrapping_sub(1) };
        self.field = fields.get(to).copied().unwrap_or(self.field);
    }

    /// The keyboard came back to the text fields from the buttons: Tab lands on the first,
    /// Shift+Tab on the last.
    pub fn enter(&mut self, forward: bool) {
        let fields = self.fields();
        let first_or_last = if forward {
            fields.first()
        } else {
            fields.last()
        };
        self.field = first_or_last.copied().unwrap_or(self.field);
    }

    /// The input of `field`.
    pub fn input(&self, field: Field) -> &Input {
        match field {
            Field::Folder => &self.folder,
            Field::Url => &self.url,
            Field::Parent => &self.parent,
        }
    }

    fn input_mut(&mut self, field: Field) -> &mut Input {
        match field {
            Field::Folder => &mut self.folder,
            Field::Url => &mut self.url,
            Field::Parent => &mut self.parent,
        }
    }

    /// A click in `field`: it takes the keyboard and the caret goes under the pointer.
    pub fn click(&mut self, field: Field, at: usize, extend: bool, clicks: usize) {
        self.field = field;
        self.input_mut(field).click(at, extend, clicks);
    }

    /// Edits the focused field; `true` when it changed.
    pub fn key(&mut self, key: &Key) -> bool {
        match key {
            Key::Edit(key) => {
                let field = self.field;
                self.input_mut(field).edit(key)
            }
            Key::Insert(text) => self.insert(text),
            Key::Close | Key::Save | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text (a URL or a path is one line). `true` when it changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        let field = self.field;
        self.input_mut(field).insert(&line)
    }

    /// The name of the folder the clone creates, once the URL names a repository.
    fn repo_dir(&self) -> Option<String> {
        mkrew_core::git::repo_dir_name(&self.url)
    }

    /// Where the clone will land, once the URL names a repository (and, in a machine window,
    /// the folder to clone into is typed).
    pub fn target(&self) -> Option<String> {
        let name = self.repo_dir()?;
        if self.machine.is_none() {
            return Some(
                Path::new(self.parent.as_str())
                    .join(name)
                    .display()
                    .to_string(),
            );
        }
        // The other computer's path: its own separator, not this computer's.
        let parent = self.parent.trim();
        let parent_dir = parent.trim_end_matches(['/', '\\']);
        if parent.is_empty() {
            return None;
        }
        let separator = if parent.contains('\\') && !parent.contains('/') {
            '\\'
        } else {
            '/'
        };
        Some(format!("{parent_dir}{separator}{name}"))
    }

    /// The folder to add (`project.add`) typed in a machine window; `None` (then `error` says
    /// why) while it is empty.
    pub fn folder_path(&mut self) -> Option<String> {
        let machine = self.machine.as_deref()?;
        let path = self.folder.trim().to_owned();
        if path.is_empty() {
            self.error = Some(format!("Type the folder on {machine}."));
            return None;
        }
        self.error = None;
        Some(path)
    }

    /// The `project.clone` to send; `None` while one is in flight or without a usable URL
    /// (then `error` says why).
    pub fn begin(&mut self) -> Option<ProjectCloneParams> {
        if self.cloning.is_some() {
            return None;
        }
        if self.repo_dir().is_none() {
            self.error = Some("Type the repository URL, e.g. https://github.com/owner/repo".into());
            return None;
        }
        if let Some(machine) = &self.machine
            && self.parent.trim().is_empty()
        {
            self.error = Some(format!("Type the folder to clone into on {machine}."));
            return None;
        }
        let clone_id = Ulid::new();
        self.cloning = Some(clone_id);
        self.error = None;
        Some(ProjectCloneParams {
            clone_id,
            url: self.url.trim().to_owned(),
            parent: if self.machine.is_some() {
                self.parent.trim().to_owned()
            } else {
                self.parent.as_str().to_owned()
            },
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
        let mut dialog = CloneDialog::new(r"C:\work".into(), None);
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

    #[test]
    fn this_computers_dialog_has_the_url_field_only() {
        let mut dialog = CloneDialog::new(r"C:\work".into(), None);
        assert_eq!(dialog.field, Field::Url);
        assert!(
            dialog.at_edge(true) && dialog.at_edge(false),
            "Tab leaves at once"
        );
        assert_eq!(dialog.folder_path(), None);
        assert_eq!(dialog.error, None, "nothing to type but the URL");
    }

    #[test]
    fn a_machine_dialog_types_the_folder_the_url_and_the_clone_folder() {
        let mut dialog = CloneDialog::new("/home/me".into(), Some("Home PC".into()));
        assert_eq!(
            dialog.field,
            Field::Folder,
            "the first field has the keyboard"
        );

        // Add: the typed folder, trimmed; empty is refused by name.
        assert_eq!(dialog.folder_path(), None);
        assert_eq!(dialog.error.as_deref(), Some("Type the folder on Home PC."));
        assert!(dialog.insert(" /srv/app\r\n"));
        assert_eq!(dialog.folder_path().as_deref(), Some("/srv/app"));
        assert_eq!(dialog.error, None);
        assert_eq!(dialog.url.as_str(), "", "typing went to the folder only");

        // Tab walks the fields and leaves at both ends.
        assert!(!dialog.at_edge(true) && dialog.at_edge(false));
        dialog.step(true);
        assert_eq!(dialog.field, Field::Url);
        assert!(dialog.insert("git@github.com:o/repo.git"));
        dialog.step(true);
        assert_eq!(dialog.field, Field::Parent);
        assert!(dialog.at_edge(true) && !dialog.at_edge(false));
        dialog.enter(true);
        assert_eq!(dialog.field, Field::Folder, "Tab from the last button");
        dialog.enter(false);
        assert_eq!(
            dialog.field,
            Field::Parent,
            "Shift+Tab from the first button"
        );

        // Clone: the other computer's separator, and the typed folder goes out as typed.
        assert_eq!(dialog.target().as_deref(), Some("/home/me/repo"));
        let params = dialog.begin().unwrap();
        assert_eq!(
            (params.parent.as_str(), params.url.as_str()),
            ("/home/me", "git@github.com:o/repo.git")
        );
    }

    #[test]
    fn a_machine_dialog_clone_needs_its_folder_and_keeps_the_others_separator() {
        let mut dialog = CloneDialog::new(String::new(), Some("Home PC".into()));
        dialog.field = Field::Url;
        dialog.insert("https://github.com/o/repo");
        assert_eq!(dialog.target(), None, "no folder typed yet");
        assert_eq!(dialog.begin(), None);
        assert_eq!(
            dialog.error.as_deref(),
            Some("Type the folder to clone into on Home PC.")
        );
        dialog.field = Field::Parent;
        dialog.insert(r"D:\work\");
        assert_eq!(dialog.target().as_deref(), Some(r"D:\work\repo"));
        dialog.parent = "/".into();
        assert_eq!(dialog.target().as_deref(), Some("/repo"));
        assert!(dialog.begin().is_some());
    }
}
