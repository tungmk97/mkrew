//! The Git view's commit box and Fetch / Push (SPEC §12B): a multi-line message committed with
//! Ctrl+Enter (staged changes only, no amend), and the remote action in flight with its outcome.
//! Pure and tested headlessly; `main.rs` sends the requests and renders them. The box has its
//! own focus outside `keymap::GIT_CONTEXT`, so typing `s` / `u` / `b` never acts on the list.

use mkrew_protocol::{GitCommitParams, GitRemoteParams, Request};
use ulid::Ulid;

use crate::task_dialog::DialogKey;

/// What a key press in the commit box does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Keep typing (the draft may have changed).
    Edited,
    /// Esc: back to the changed-file list; the draft stays.
    Leave,
    /// Ctrl+Enter: commit the draft.
    Commit,
}

/// The commit message draft of one workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitBox {
    pub text: String,
    /// A `git.commit` is in flight.
    pub busy: bool,
    /// Why the last commit failed, or why the draft cannot be committed.
    pub error: Option<String>,
}

impl CommitBox {
    /// Applies a key; Enter is a new line. Paste is the caller's (see [`CommitBox::insert`]).
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Leave,
            DialogKey::Save => return Outcome::Commit,
            DialogKey::Enter => self.insert("\n"),
            DialogKey::Backspace => {
                if !self.busy {
                    self.text.pop();
                    self.error = None;
                }
            }
            DialogKey::Insert(text) => self.insert(&text),
            DialogKey::SwitchField | DialogKey::Paste => {}
        }
        Outcome::Edited
    }

    /// Types or pastes `text` (CRLF becomes LF); ignored while committing.
    pub fn insert(&mut self, text: &str) {
        if self.busy {
            return;
        }
        self.text.push_str(&text.replace("\r\n", "\n"));
        self.error = None;
    }

    /// The `git.commit` of the draft (blank lines and spaces around it trimmed), and marks it
    /// in flight; `None`
    /// while one is in flight or when the draft cannot be committed ([`Self::error`] says why).
    pub fn begin(&mut self, project_id: Ulid, staged: usize) -> Option<GitCommitParams> {
        if self.busy {
            return None;
        }
        let message = self.text.trim();
        let invalid = if staged == 0 {
            Some("Stage changes to commit them.")
        } else if message.is_empty() {
            Some("Write a commit message.")
        } else {
            None
        };
        if let Some(invalid) = invalid {
            self.error = Some(invalid.to_owned());
            return None;
        }
        self.busy = true;
        self.error = None;
        Some(GitCommitParams {
            project_id,
            worktree_task_id: None,
            message: message.to_owned(),
        })
    }

    /// The `git.commit` answer: the draft empties on success, stays (with the error) otherwise.
    pub fn committed(&mut self, result: Result<(), String>) {
        self.busy = false;
        match result {
            Ok(()) => self.text.clear(),
            Err(error) => self.error = Some(format!("Commit failed: {error}")),
        }
    }
}

/// A remote action of the Git view header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remote {
    /// `git.fetch`: refreshes the base and ahead/behind.
    Fetch,
    /// `git.push`: the current branch to its upstream, never forced.
    Push,
}

impl Remote {
    pub fn label(self) -> &'static str {
        match self {
            Remote::Fetch => "Fetch",
            Remote::Push => "Push",
        }
    }

    /// The label while it runs.
    pub fn running_label(self) -> &'static str {
        match self {
            Remote::Fetch => "Fetching…",
            Remote::Push => "Pushing…",
        }
    }

    pub fn request(self, project_id: Ulid) -> Request {
        let params = GitRemoteParams {
            project_id,
            worktree_task_id: None,
        };
        match self {
            Remote::Fetch => Request::GitFetch(params),
            Remote::Push => Request::GitPush(params),
        }
    }
}

/// The Fetch / Push of one workspace: at most one runs at a time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Remotes {
    pub running: Option<Remote>,
    /// Why the last fetch or push failed.
    pub error: Option<String>,
}

impl Remotes {
    /// Marks `remote` in flight; `false` while another one runs.
    pub fn begin(&mut self, remote: Remote) -> bool {
        if self.running.is_some() {
            return false;
        }
        self.running = Some(remote);
        self.error = None;
        true
    }

    /// The answer of the running action.
    pub fn finished(&mut self, remote: Remote, result: Result<(), String>) {
        self.running = None;
        self.error = result
            .err()
            .map(|error| format!("{} failed: {error}", remote.label()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_text(draft: &mut CommitBox, text: &str) {
        for c in text.chars() {
            draft.key(DialogKey::Insert(c.to_string()));
        }
    }

    #[test]
    fn enter_breaks_lines_and_ctrl_enter_commits_the_trimmed_message() {
        let project_id = Ulid::new();
        let mut draft = CommitBox::default();
        type_text(&mut draft, "Fix loginn");
        assert_eq!(draft.key(DialogKey::Backspace), Outcome::Edited);
        assert_eq!(draft.key(DialogKey::Enter), Outcome::Edited);
        assert_eq!(draft.key(DialogKey::Enter), Outcome::Edited);
        draft.insert("Body\r\nmore\n\n");
        assert_eq!(draft.key(DialogKey::SwitchField), Outcome::Edited);
        assert_eq!(draft.text, "Fix login\n\nBody\nmore\n\n");
        assert_eq!(draft.key(DialogKey::Save), Outcome::Commit);
        assert_eq!(draft.key(DialogKey::Cancel), Outcome::Leave);

        assert_eq!(
            draft.begin(project_id, 1),
            Some(GitCommitParams {
                project_id,
                worktree_task_id: None,
                message: "Fix login\n\nBody\nmore".into(),
            })
        );
        assert!(draft.busy);
        assert_eq!(draft.begin(project_id, 1), None, "one commit at a time");
        draft.insert("x");
        draft.key(DialogKey::Backspace);
        assert_eq!(
            draft.text, "Fix login\n\nBody\nmore\n\n",
            "frozen while committing"
        );

        draft.committed(Ok(()));
        assert!(!draft.busy);
        assert_eq!(draft.text, "");
    }

    #[test]
    fn nothing_staged_or_no_message_is_explained_instead_of_sent() {
        let mut draft = CommitBox::default();
        draft.insert("  \n ");
        assert_eq!(draft.begin(Ulid::new(), 2), None);
        assert_eq!(draft.error.as_deref(), Some("Write a commit message."));
        draft.insert("Ship it");
        assert_eq!(draft.error, None, "typing clears the hint");
        assert_eq!(draft.begin(Ulid::new(), 0), None);
        assert_eq!(
            draft.error.as_deref(),
            Some("Stage changes to commit them.")
        );
        assert!(!draft.busy);
        draft.key(DialogKey::Backspace);
        assert_eq!(draft.error, None, "editing clears it too");
    }

    #[test]
    fn a_failed_commit_keeps_the_draft_and_says_why() {
        let mut draft = CommitBox::default();
        draft.insert("Ship it");
        assert!(draft.begin(Ulid::new(), 1).is_some());
        draft.committed(Err("pre-commit hook failed".into()));
        assert!(!draft.busy);
        assert_eq!(draft.text, "Ship it");
        assert_eq!(
            draft.error.as_deref(),
            Some("Commit failed: pre-commit hook failed")
        );
    }

    #[test]
    fn one_remote_action_runs_at_a_time_and_failures_are_kept() {
        let project_id = Ulid::new();
        assert_eq!(
            Remote::Fetch.request(project_id),
            Request::GitFetch(GitRemoteParams {
                project_id,
                worktree_task_id: None,
            })
        );
        assert_eq!(
            Remote::Push.request(project_id),
            Request::GitPush(GitRemoteParams {
                project_id,
                worktree_task_id: None,
            })
        );

        let mut remotes = Remotes::default();
        assert!(remotes.begin(Remote::Push));
        assert!(!remotes.begin(Remote::Fetch));
        remotes.finished(Remote::Push, Err("rejected (non-fast-forward)".into()));
        assert_eq!(remotes.running, None);
        assert_eq!(
            remotes.error.as_deref(),
            Some("Push failed: rejected (non-fast-forward)")
        );
        assert!(remotes.begin(Remote::Fetch));
        assert_eq!(remotes.error, None, "a new attempt clears the old failure");
        remotes.finished(Remote::Fetch, Ok(()));
        assert_eq!(remotes, Remotes::default());
    }
}
