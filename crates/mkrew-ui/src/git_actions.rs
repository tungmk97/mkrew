//! The Git view's commit box and Fetch / Push (SPEC §12B): a multi-line message committed with
//! Ctrl+Enter (staged changes only, no amend), and the remote action in flight with its outcome.
//! Pure and tested headlessly; `main.rs` sends the requests and renders them. The box has its
//! own focus outside `keymap::GIT_CONTEXT`, so typing `s` / `u` / `b` never acts on the list.

use mkrew_protocol::{GitCommitParams, GitRemoteParams, Request};
use ulid::Ulid;

use crate::task_dialog::DialogKey;
use crate::text_box::TextBox;

/// The commit box counts its summary line against this many chars (git's usual limit).
pub const SUMMARY_LIMIT: usize = 72;

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
    pub text: TextBox,
    /// A `git.commit` is in flight.
    pub busy: bool,
    /// Why the last commit failed, or why the draft cannot be committed.
    pub error: Option<String>,
    /// The `ai.generate` writing this draft (SPEC §12B ✦), by id; typing waits for it.
    pub generating: Option<Ulid>,
    /// Ctrl+Enter found nothing staged but changes to stage: the next commit stages them all
    /// first. Any edit withdraws the offer.
    pub stage_all_offered: bool,
}

/// What a commit sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Begin {
    /// `git.commit` of the staged changes.
    Commit(GitCommitParams),
    /// `git.stage` of every changed file, then `git.commit`.
    StageAllThenCommit(GitCommitParams),
}

/// The commit box's note while it offers to stage everything.
pub const STAGE_ALL_OFFER: &str =
    "Nothing staged. Ctrl+Enter again stages every change and commits.";

impl CommitBox {
    /// Applies a key; Enter is a new line. Paste is the caller's (see [`CommitBox::insert`]).
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => Outcome::Leave,
            DialogKey::Save => Outcome::Commit,
            _ if self.busy || self.generating.is_some() => Outcome::Edited,
            key => {
                self.text.key(key);
                self.error = None;
                self.stage_all_offered = false;
                Outcome::Edited
            }
        }
    }

    /// Types or pastes `text` (CRLF becomes LF); ignored while committing or generating.
    pub fn insert(&mut self, text: &str) {
        if self.busy || self.generating.is_some() {
            return;
        }
        self.text.insert(text);
        self.error = None;
        self.stage_all_offered = false;
    }

    /// What committing the draft (blank lines and spaces around it trimmed) sends, and marks it
    /// in flight; `None` while one is in flight or when the draft cannot be committed
    /// ([`Self::error`] says why). With nothing `staged` but `unstaged` files, the first try
    /// offers to stage them all ([`STAGE_ALL_OFFER`]) and the next one does.
    pub fn begin(&mut self, project_id: Ulid, staged: usize, unstaged: usize) -> Option<Begin> {
        if self.busy || self.generating.is_some() {
            return None;
        }
        let message = self.text.text().trim();
        let invalid = if message.is_empty() {
            Some("Write a commit message.")
        } else if staged == 0 && unstaged == 0 {
            Some("Stage changes to commit them.")
        } else if staged == 0 && !self.stage_all_offered {
            Some(STAGE_ALL_OFFER)
        } else {
            None
        };
        if let Some(invalid) = invalid {
            self.stage_all_offered = invalid == STAGE_ALL_OFFER;
            self.error = Some(invalid.to_owned());
            return None;
        }
        let params = GitCommitParams {
            project_id,
            worktree_task_id: None,
            repo: None,
            message: message.to_owned(),
        };
        self.busy = true;
        self.error = None;
        Some(if staged == 0 {
            self.stage_all_offered = false;
            Begin::StageAllThenCommit(params)
        } else {
            Begin::Commit(params)
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

    /// Starts generation `generation_id` of the message; `false` while committing or while
    /// another generation runs.
    pub fn begin_generate(&mut self, generation_id: Ulid) -> bool {
        if self.busy || self.generating.is_some() {
            return false;
        }
        self.generating = Some(generation_id);
        self.error = None;
        true
    }

    /// The answer of generation `generation_id`: the text replaces the draft, a failure keeps
    /// it and says why. `false` if this box did not ask for it.
    pub fn generated(&mut self, generation_id: Ulid, result: Result<String, String>) -> bool {
        if self.generating != Some(generation_id) {
            return false;
        }
        self.generating = None;
        match result {
            Ok(text) => {
                self.text.clear();
                self.text.insert(&text);
            }
            Err(error) => self.error = Some(format!("Cannot write a message: {error}")),
        }
        true
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

    /// The request on the project folder, task worktree `worktree_task_id` or child
    /// repository `repo`.
    pub fn request(
        self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        repo: Option<String>,
    ) -> Request {
        let params = GitRemoteParams {
            project_id,
            worktree_task_id,
            repo,
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
    fn a_generated_message_replaces_the_draft_and_typing_waits_for_it() {
        let mut commit = CommitBox::default();
        commit.insert("wip");
        let (first, second) = (Ulid::new(), Ulid::new());
        assert!(commit.begin_generate(first));
        assert!(!commit.begin_generate(second), "one generation at a time");
        commit.insert("x");
        commit.key(DialogKey::Backspace);
        assert_eq!(
            commit.text.text(),
            "wip",
            "typing is ignored while the agent writes"
        );
        assert!(
            commit.begin(Ulid::new(), 1, 0).is_none(),
            "no commit while the agent writes"
        );
        assert!(
            !commit.generated(second, Ok("other".into())),
            "not this box's generation"
        );
        assert!(commit.generated(first, Ok("feat: add x\r\n\r\nWhy.".into())));
        assert_eq!(commit.text.text(), "feat: add x\n\nWhy.");
        assert_eq!(commit.generating, None);
        assert_eq!(commit.error, None);
    }

    #[test]
    fn a_failed_generation_keeps_the_draft_and_says_why() {
        let mut commit = CommitBox::default();
        commit.insert("wip");
        let id = Ulid::new();
        assert!(commit.begin_generate(id));
        assert!(commit.generated(id, Err("the agent exited with 3".into())));
        assert_eq!(commit.text.text(), "wip");
        assert_eq!(
            commit.error.as_deref(),
            Some("Cannot write a message: the agent exited with 3")
        );
        assert!(commit.begin_generate(Ulid::new()), "the user can retry");
        let mut committing = CommitBox {
            busy: true,
            ..CommitBox::default()
        };
        assert!(
            !committing.begin_generate(Ulid::new()),
            "not while committing"
        );
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
        assert_eq!(draft.text.text(), "Fix login\n\nBody\nmore\n\n");
        assert_eq!(draft.key(DialogKey::Save), Outcome::Commit);
        assert_eq!(draft.key(DialogKey::Cancel), Outcome::Leave);

        assert_eq!(
            draft.begin(project_id, 1, 3),
            Some(Begin::Commit(GitCommitParams {
                project_id,
                worktree_task_id: None,
                repo: None,
                message: "Fix login\n\nBody\nmore".into(),
            }))
        );
        assert!(draft.busy);
        assert_eq!(draft.begin(project_id, 1, 0), None, "one commit at a time");
        draft.insert("x");
        draft.key(DialogKey::Backspace);
        assert_eq!(
            draft.text.text(),
            "Fix login\n\nBody\nmore\n\n",
            "frozen while committing"
        );

        draft.committed(Ok(()));
        assert!(!draft.busy);
        assert_eq!(draft.text.text(), "");
    }

    #[test]
    fn nothing_staged_or_no_message_is_explained_instead_of_sent() {
        let mut draft = CommitBox::default();
        draft.insert("  \n ");
        assert_eq!(draft.begin(Ulid::new(), 2, 0), None);
        assert_eq!(draft.error.as_deref(), Some("Write a commit message."));
        draft.insert("Ship it");
        assert_eq!(draft.error, None, "typing clears the hint");
        assert_eq!(draft.begin(Ulid::new(), 0, 0), None);
        assert_eq!(
            draft.error.as_deref(),
            Some("Stage changes to commit them.")
        );
        assert!(!draft.busy);
        draft.key(DialogKey::Backspace);
        assert_eq!(draft.error, None, "editing clears it too");
    }

    #[test]
    fn nothing_staged_offers_to_stage_everything_then_does_on_the_next_try() {
        let project_id = Ulid::new();
        let mut draft = CommitBox::default();
        draft.insert("Ship it");
        assert_eq!(draft.begin(project_id, 0, 3), None);
        assert_eq!(draft.error.as_deref(), Some(STAGE_ALL_OFFER));
        assert!(!draft.busy);
        // Editing withdraws the offer: the next try asks again.
        draft.insert("!");
        assert_eq!(draft.begin(project_id, 0, 3), None);
        assert!(draft.stage_all_offered);
        let params = GitCommitParams {
            project_id,
            worktree_task_id: None,
            repo: None,
            message: "Ship it!".into(),
        };
        assert_eq!(
            draft.begin(project_id, 0, 3),
            Some(Begin::StageAllThenCommit(params))
        );
        assert!(draft.busy && !draft.stage_all_offered && draft.error.is_none());
        draft.committed(Ok(()));
        // Staged changes commit at once, offer or not.
        draft.insert("Next");
        assert!(matches!(
            draft.begin(project_id, 2, 3),
            Some(Begin::Commit(_))
        ));
    }

    #[test]
    fn a_failed_commit_keeps_the_draft_and_says_why() {
        let mut draft = CommitBox::default();
        draft.insert("Ship it");
        assert!(draft.begin(Ulid::new(), 1, 0).is_some());
        draft.committed(Err("pre-commit hook failed".into()));
        assert!(!draft.busy);
        assert_eq!(draft.text.text(), "Ship it");
        assert_eq!(
            draft.error.as_deref(),
            Some("Commit failed: pre-commit hook failed")
        );
    }

    #[test]
    fn ctrl_a_then_backspace_typing_or_paste_replaces_the_whole_draft() {
        let mut draft = CommitBox::default();
        type_text(&mut draft, "wip\nmore");
        assert_eq!(draft.key(DialogKey::SelectAll), Outcome::Edited);
        draft.key(DialogKey::Backspace);
        assert_eq!(draft.text.text(), "", "Backspace removes everything");

        type_text(&mut draft, "old");
        draft.key(DialogKey::SelectAll);
        type_text(&mut draft, "fix: x");
        assert_eq!(draft.text.text(), "fix: x", "typing replaces the selection");
        draft.key(DialogKey::Backspace);
        assert_eq!(
            draft.text.text(),
            "fix: ",
            "the selection is gone after one edit"
        );

        draft.key(DialogKey::SelectAll);
        draft.insert("pasted");
        assert_eq!(draft.text.text(), "pasted", "paste replaces the selection");
    }

    #[test]
    fn select_all_does_not_change_commit_or_leave() {
        let mut draft = CommitBox::default();
        draft.insert("Ship it");
        draft.key(DialogKey::SelectAll);
        assert_eq!(draft.key(DialogKey::Cancel), Outcome::Leave);
        assert_eq!(draft.text.text(), "Ship it", "Esc keeps the text");
        assert_eq!(draft.key(DialogKey::Save), Outcome::Commit);
        assert_eq!(
            draft.begin(Ulid::new(), 1, 0).map(|begin| match begin {
                Begin::Commit(params) | Begin::StageAllThenCommit(params) => params.message,
            }),
            Some("Ship it".to_owned()),
            "Ctrl+Enter commits the whole selected text"
        );
        let mut busy = CommitBox {
            busy: true,
            ..CommitBox::default()
        };
        busy.key(DialogKey::SelectAll);
        assert!(!busy.text.all_selected(), "frozen while committing");
    }

    #[test]
    fn one_remote_action_runs_at_a_time_and_failures_are_kept() {
        let project_id = Ulid::new();
        assert_eq!(
            Remote::Fetch.request(project_id, None, None),
            Request::GitFetch(GitRemoteParams {
                project_id,
                worktree_task_id: None,
                repo: None,
            })
        );
        let worktree_task_id = Some(Ulid::new());
        assert_eq!(
            Remote::Push.request(project_id, worktree_task_id, None),
            Request::GitPush(GitRemoteParams {
                project_id,
                worktree_task_id,
                repo: None,
            })
        );
        assert_eq!(
            Remote::Push.request(project_id, None, Some("fe".into())),
            Request::GitPush(GitRemoteParams {
                project_id,
                worktree_task_id: None,
                repo: Some("fe".into()),
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
