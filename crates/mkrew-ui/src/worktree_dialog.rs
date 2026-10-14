//! "Merge back…" and "Remove worktree…" of a worktree task's Review / Done card (SPEC §20E.1):
//! the confirmation listing the branch's commits (or the worktree's uncommitted files, with a
//! separate confirmed discard step), then the `task.merge` / `task.remove_worktree` result.
//! Pure and tested headlessly; `main.rs` renders it.

use std::collections::{BTreeSet, HashSet};

use mkrew_protocol::{
    ChangeKind, Commit, GitDiscardParams, GitLogParams, GitStatus, GitStatusParams,
    RemoveWorktreeResult, Request, Session, SessionState, Task, TaskConfirmParams, TaskMergeResult,
};
use ulid::Ulid;

use crate::fanout::Pick;

/// What the dialog does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Merge,
    Remove,
}

/// What a merge needs to know, read when the dialog opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePlan {
    /// The branch's commits the user's checkout lacks, newest first.
    pub commits: Vec<Commit>,
    /// The checkout's branch; `None` = detached HEAD.
    pub into: Option<String>,
    /// Uncommitted files of the worktree.
    pub worktree_dirty: Vec<String>,
    /// Uncommitted files of the user's checkout.
    pub checkout_dirty: Vec<String>,
}

impl MergePlan {
    /// Why the merge cannot run now (the daemon refuses the same cases, except "no commits").
    pub fn blocker(&self) -> Option<String> {
        if !self.worktree_dirty.is_empty() {
            return Some(format!(
                "The worktree has {} uncommitted; commit them in its Git view first.",
                files(self.worktree_dirty.len())
            ));
        }
        if !self.checkout_dirty.is_empty() {
            return Some(format!(
                "Your checkout has {} uncommitted; commit or stash them first.",
                files(self.checkout_dirty.len())
            ));
        }
        if self.into.is_none() {
            return Some("Your checkout is on a detached HEAD; switch to a branch first.".into());
        }
        if self.commits.is_empty() {
            return Some("The branch has no commits your checkout lacks.".into());
        }
        None
    }
}

/// What a removal needs to know, read when the dialog opens and after a discard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovePlan {
    /// Every uncommitted file of the worktree (staged, unstaged, untracked), sorted.
    pub uncommitted: Vec<String>,
    /// The ones `git.discard` can throw away: unstaged and untracked (staged changes stay).
    pub discardable: Vec<String>,
}

/// The dialog's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Loading,
    Failed(String),
    Merge(MergePlan),
    Remove(RemovePlan),
    /// Remove: "Discard N files?" asked; the next confirm discards them.
    ConfirmDiscard(RemovePlan),
    /// The daemon's answer.
    Done(Done),
}

/// How it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// Merged into this branch.
    Merged {
        into: String,
    },
    /// `git merge --abort` ran: the checkout is as before.
    Conflicts(Vec<String>),
    Removed {
        branch_kept: bool,
    },
}

/// The open dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeDialog {
    pub kind: Kind,
    pub task_id: Ulid,
    pub project_id: Ulid,
    pub title: String,
    pub branch: String,
    pub step: Step,
    /// A request changing something is in flight.
    pub busy: bool,
    /// Why the last request failed.
    pub error: Option<String>,
    /// The merge is a fan-out "Pick" (SPEC §20E.2).
    pub pick: Option<Pick>,
}

impl WorktreeDialog {
    /// The dialog for a task with a worktree and its branch; `None` without them.
    pub fn new(kind: Kind, task: &Task) -> Option<Self> {
        task.worktree_path.as_ref()?;
        Some(Self {
            kind,
            task_id: task.id,
            project_id: task.project_id,
            title: task.title.clone(),
            branch: task.branch.clone()?,
            step: Step::Loading,
            busy: false,
            error: None,
            pick: None,
        })
    }

    /// "Pick" in the Compare view: the Merge back dialog of the picked child.
    pub fn for_pick(task: &Task, pick: Pick) -> Option<Self> {
        let mut dialog = Self::new(Kind::Merge, task)?;
        dialog.pick = Some(pick);
        Some(dialog)
    }

    /// The Pick's original, once the pick merged: it moves to Review.
    pub fn picked_original(&self) -> Option<Ulid> {
        match (&self.step, &self.pick) {
            (Step::Done(Done::Merged { .. }), Some(pick)) => Some(pick.original_id),
            _ => None,
        }
    }

    pub fn heading(&self) -> &'static str {
        match (&self.step, self.kind) {
            (Step::Done(Done::Merged { .. }), _) => "Merged",
            (Step::Done(Done::Conflicts(_)), _) => "Merge aborted",
            (Step::Done(Done::Removed { .. }), _) => "Worktree removed",
            (_, Kind::Merge) => "Merge back?",
            (_, Kind::Remove) => "Remove worktree?",
        }
    }

    /// `git.status` of the worktree (both kinds) or of the user's checkout (merge only).
    pub fn status_request(&self, worktree: bool) -> Request {
        Request::GitStatus(GitStatusParams {
            project_id: self.project_id,
            worktree_task_id: worktree.then_some(self.task_id),
            repo: None,
            mode: Default::default(),
            since: None,
        })
    }

    /// `git.log` of the worktree or of the user's checkout (merge only).
    pub fn log_request(&self, worktree: bool) -> Request {
        Request::GitLog(GitLogParams {
            project_id: self.project_id,
            worktree_task_id: worktree.then_some(self.task_id),
            repo: None,
            limit: GitLogParams::DEFAULT_LIMIT,
            path: None,
        })
    }

    /// The confirmed request of the current step, or why there is none. `sessions` = every
    /// known session: the daemon refuses removing the worktree of a live task.
    pub fn confirm_request(&self, sessions: &[Session]) -> Result<Request, String> {
        if self.busy {
            return Err("Working…".into());
        }
        let confirm = TaskConfirmParams {
            task_id: self.task_id,
            confirm: true,
        };
        match &self.step {
            Step::Loading | Step::Failed(_) | Step::Done(_) => Err(String::new()),
            Step::Merge(plan) => match plan.blocker() {
                Some(why) => Err(why),
                None => Ok(Request::TaskMerge(confirm)),
            },
            Step::Remove(plan) => {
                if sessions
                    .iter()
                    .any(|s| s.task_id == Some(self.task_id) && s.state != SessionState::Exited)
                {
                    return Err("Close the task's terminal first.".into());
                }
                if !plan.uncommitted.is_empty() {
                    return Err(format!(
                        "The worktree has {} uncommitted.",
                        files(plan.uncommitted.len())
                    ));
                }
                Ok(Request::TaskRemoveWorktree(confirm))
            }
            Step::ConfirmDiscard(plan) => Ok(Request::GitDiscard(GitDiscardParams {
                project_id: self.project_id,
                worktree_task_id: Some(self.task_id),
                repo: None,
                paths: plan.discardable.clone(),
                confirm: true,
            })),
        }
    }

    /// Remove: "Discard…" was clicked; the next confirm discards. `false` = nothing to discard.
    pub fn ask_discard(&mut self) -> bool {
        match &self.step {
            Step::Remove(plan) if !plan.discardable.is_empty() => {
                self.step = Step::ConfirmDiscard(plan.clone());
                self.error = None;
                true
            }
            _ => false,
        }
    }

    /// Back from the discard question to the file list.
    pub fn keep_files(&mut self) {
        if let Step::ConfirmDiscard(plan) = &self.step {
            self.step = Step::Remove(plan.clone());
        }
    }

    pub fn merged(&mut self, result: TaskMergeResult) {
        self.busy = false;
        let into = match &self.step {
            Step::Merge(plan) => plan.into.clone().unwrap_or_default(),
            _ => String::new(),
        };
        self.step = Step::Done(if result.merged {
            Done::Merged { into }
        } else {
            Done::Conflicts(result.conflicts)
        });
    }

    pub fn removed(&mut self, result: RemoveWorktreeResult) {
        self.busy = false;
        self.step = Step::Done(Done::Removed {
            branch_kept: result.branch_kept,
        });
    }

    /// The request failed: shown under the body; the step is read again by the caller.
    pub fn failed(&mut self, error: &str) {
        self.busy = false;
        self.error = Some(error.to_owned());
    }

    /// The intro line of the current step.
    pub fn intro(&self) -> String {
        let branch = &self.branch;
        match &self.step {
            Step::Loading | Step::Failed(_) => match self.kind {
                Kind::Merge => format!("Merges {branch} into your checkout."),
                Kind::Remove => format!("Deletes the worktree folder of “{}”.", self.title),
            },
            Step::Merge(plan) => {
                let into = plan.into.as_deref().unwrap_or("your checkout");
                format!(
                    "Runs git merge --no-ff {branch} on {into}. On a conflict the merge is \
                     aborted and your checkout stays as it is."
                )
            }
            Step::Remove(plan) if plan.uncommitted.is_empty() => format!(
                "Deletes the worktree folder of “{}”, then the branch {branch} if it is merged \
                 (an unmerged branch is kept).",
                self.title
            ),
            Step::Remove(plan) if plan.discardable.len() < plan.uncommitted.len() => format!(
                "The worktree has uncommitted changes, so it cannot be removed. Commit them, \
                 or unstage the staged ones in its Git view and discard the rest ({}).",
                files(plan.uncommitted.len())
            ),
            Step::Remove(plan) => format!(
                "The worktree has uncommitted changes ({}), so it cannot be removed. Commit \
                 them in its Git view, or discard them.",
                files(plan.uncommitted.len())
            ),
            Step::ConfirmDiscard(plan) => format!(
                "Discard the {} below? Changed files go back to their committed version, new \
                 ones are deleted. This cannot be undone.",
                files(plan.discardable.len())
            ),
            Step::Done(Done::Merged { into }) => format!("Merged {branch} into {into}."),
            Step::Done(Done::Conflicts(_)) => format!(
                "{branch} conflicts with your checkout in the files below. The merge was \
                 aborted: your checkout is unchanged."
            ),
            Step::Done(Done::Removed { branch_kept: true }) => format!(
                "Worktree removed. The branch {branch} is not merged, so it was kept \
                 (git branch -d)."
            ),
            Step::Done(Done::Removed { branch_kept: false }) => {
                format!("Worktree removed and the branch {branch} deleted.")
            }
        }
    }
}

/// `n file(s)`.
fn files(n: usize) -> String {
    if n == 1 {
        "1 file".into()
    } else {
        format!("{n} files")
    }
}

/// The merge's view of both sides.
pub fn merge_plan(
    worktree_log: Vec<Commit>,
    checkout_log: &[Commit],
    worktree: &GitStatus,
    checkout: &GitStatus,
) -> MergePlan {
    MergePlan {
        commits: branch_commits(worktree_log, checkout_log),
        into: checkout.branch.clone(),
        worktree_dirty: uncommitted(worktree),
        // Untracked files of the checkout are allowed (git refuses a merge overwriting one).
        checkout_dirty: checkout
            .staged
            .iter()
            .chain(&checkout.unstaged)
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

/// The removal's view of the worktree.
pub fn remove_plan(worktree: &GitStatus) -> RemovePlan {
    let discardable = worktree
        .unstaged
        .iter()
        .filter(|file| file.kind != ChangeKind::Conflicted)
        .chain(&worktree.untracked)
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    RemovePlan {
        uncommitted: uncommitted(worktree),
        discardable: discardable.into_iter().collect(),
    }
}

/// The worktree's commits missing from the checkout's log (both newest first, cut at
/// [`GitLogParams::DEFAULT_LIMIT`], so a checkout far ahead of the fork can list a few shared
/// ones too).
pub fn branch_commits(worktree_log: Vec<Commit>, checkout_log: &[Commit]) -> Vec<Commit> {
    let known: HashSet<&str> = checkout_log.iter().map(|c| c.hash.as_str()).collect();
    worktree_log
        .into_iter()
        .filter(|commit| !known.contains(commit.hash.as_str()))
        .collect()
}

/// Every uncommitted path of a working-mode status, sorted, once each.
pub fn uncommitted(status: &GitStatus) -> Vec<String> {
    status
        .staged
        .iter()
        .chain(&status.unstaged)
        .chain(&status.untracked)
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{ChangedFile, SessionKind, StateSource, TaskStatus};

    use super::*;

    fn task(worktree: bool) -> Task {
        Task {
            id: Ulid(7),
            project_id: Ulid(1),
            title: "Fix login".into(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Review,
            position: 1.0,
            checkpoint: Some("abc".into()),
            error_flag: false,
            worktree: true,
            worktree_path: worktree.then(|| "/data/wt/7".into()),
            branch: worktree.then(|| "mkrew/fix-login-7".into()),
            fanout_of: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn commit(hash: &str, summary: &str) -> Commit {
        Commit {
            hash: hash.into(),
            parents: vec![],
            author_name: "A".into(),
            author_email: "a@x".into(),
            authored_at: 0,
            summary: summary.into(),
            body: String::new(),
        }
    }

    fn file(path: &str, kind: ChangeKind) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            old_path: None,
            kind,
            added: None,
            removed: None,
        }
    }

    fn clean(branch: Option<&str>) -> GitStatus {
        GitStatus {
            branch: branch.map(Into::into),
            ..Default::default()
        }
    }

    fn session(task_id: Ulid, state: SessionState) -> Session {
        Session {
            id: Ulid(9),
            task_id: Some(task_id),
            project_id: Ulid(1),
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
        }
    }

    /// The dialog's text, as the user reads it.
    fn shown(dialog: &WorktreeDialog, sessions: &[Session]) -> String {
        let mut lines = vec![dialog.heading().to_owned(), dialog.intro()];
        match &dialog.step {
            Step::Merge(plan) => lines.extend(
                plan.commits
                    .iter()
                    .map(|c| format!("  {} {}", &c.hash[..7], c.summary)),
            ),
            Step::Remove(plan) => lines.extend(plan.uncommitted.iter().map(|p| format!("  {p}"))),
            Step::ConfirmDiscard(plan) => {
                lines.extend(plan.discardable.iter().map(|p| format!("  {p}")))
            }
            Step::Done(Done::Conflicts(paths)) => {
                lines.extend(paths.iter().map(|p| format!("  {p}")))
            }
            _ => {}
        }
        match dialog.confirm_request(sessions) {
            Ok(request) => {
                let method = serde_json::to_value(&request).unwrap()["method"].clone();
                lines.push(format!("[confirm: {method}]"));
            }
            Err(why) if !why.is_empty() => lines.push(format!("[disabled: {why}]")),
            Err(_) => {}
        }
        lines.join("\n")
    }

    #[test]
    fn only_a_task_with_a_worktree_gets_the_dialog() {
        assert!(WorktreeDialog::new(Kind::Merge, &task(false)).is_none());
        let dialog = WorktreeDialog::new(Kind::Remove, &task(true)).unwrap();
        assert_eq!(dialog.branch, "mkrew/fix-login-7");
        assert_eq!(dialog.step, Step::Loading);
    }

    #[test]
    fn the_commit_list_is_what_the_checkout_lacks() {
        let shared = commit("0000000aaaa", "init");
        let worktree = vec![
            commit("2222222bbbb", "Second"),
            commit("1111111cccc", "First"),
            shared.clone(),
        ];
        let listed = branch_commits(worktree, &[commit("3333333dddd", "main"), shared]);
        let summaries: Vec<_> = listed.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(summaries, ["Second", "First"]);
    }

    #[test]
    fn merge_confirmation_lists_the_commits_then_shows_the_result() {
        let mut dialog = WorktreeDialog::new(Kind::Merge, &task(true)).unwrap();
        let worktree = vec![
            commit("2222222bbbb", "Fix the login form"),
            commit("1111111cccc", "Add a test"),
            commit("0000000aaaa", "init"),
        ];
        dialog.step = Step::Merge(merge_plan(
            worktree,
            &[commit("0000000aaaa", "init")],
            &clean(Some("mkrew/fix-login-7")),
            &clean(Some("main")),
        ));
        let before = shown(&dialog, &[]);
        dialog.busy = true;
        dialog.merged(TaskMergeResult {
            merged: true,
            conflicts: vec![],
        });
        insta::assert_snapshot!(format!("{before}\n---\n{}", shown(&dialog, &[])));
    }

    #[test]
    fn a_conflict_reports_the_files_and_an_unchanged_checkout() {
        let mut dialog = WorktreeDialog::new(Kind::Merge, &task(true)).unwrap();
        dialog.merged(TaskMergeResult {
            merged: false,
            conflicts: vec!["src/login.rs".into(), "README.md".into()],
        });
        insta::assert_snapshot!(shown(&dialog, &[]));
    }

    #[test]
    fn merge_is_refused_until_both_sides_are_clean_on_a_branch() {
        let mut dialog = WorktreeDialog::new(Kind::Merge, &task(true)).unwrap();
        let commits = || vec![commit("2222222bbbb", "Fix")];
        let mut dirty = clean(Some("main"));
        dirty.untracked = vec![file("notes.txt", ChangeKind::Untracked)];
        let mut changed = clean(Some("main"));
        changed.staged = vec![file("a.rs", ChangeKind::Modified)];
        let cases = [
            (
                dirty.clone(),
                clean(Some("main")),
                "The worktree has 1 file",
            ),
            (clean(None), changed, "Your checkout has 1 file"),
            (clean(None), clean(None), "detached HEAD"),
        ];
        for (worktree, checkout, why) in cases {
            dialog.step = Step::Merge(merge_plan(commits(), &[], &worktree, &checkout));
            let refused = dialog.confirm_request(&[]).unwrap_err();
            assert!(refused.contains(why), "{refused}");
        }
        dialog.step = Step::Merge(merge_plan(vec![], &[], &clean(None), &clean(Some("main"))));
        assert!(dialog.confirm_request(&[]).is_err());
        // An untracked file in the checkout does not block (git checks overwrites itself).
        dialog.step = Step::Merge(merge_plan(commits(), &[], &clean(None), &dirty));
        assert!(dialog.confirm_request(&[]).is_ok());
        dialog.busy = true;
        assert!(dialog.confirm_request(&[]).is_err());
    }

    #[test]
    fn removal_shows_uncommitted_files_and_discards_them_in_a_separate_step() {
        let mut dialog = WorktreeDialog::new(Kind::Remove, &task(true)).unwrap();
        let mut status = clean(Some("mkrew/fix-login-7"));
        status.unstaged = vec![file("src/login.rs", ChangeKind::Modified)];
        status.untracked = vec![file("scratch.txt", ChangeKind::Untracked)];
        dialog.step = Step::Remove(remove_plan(&status));
        let listed = shown(&dialog, &[]);
        assert!(dialog.ask_discard());
        let asked = shown(&dialog, &[]);
        let Ok(Request::GitDiscard(discard)) = dialog.confirm_request(&[]) else {
            panic!("the second step discards");
        };
        assert_eq!(discard.worktree_task_id, Some(dialog.task_id));
        assert!(discard.confirm);
        dialog.keep_files();
        assert!(matches!(dialog.step, Step::Remove(_)));

        dialog.step = Step::Remove(remove_plan(&clean(None)));
        let empty = shown(&dialog, &[]);
        let live = shown(&dialog, &[session(dialog.task_id, SessionState::Idle)]);
        dialog.removed(RemoveWorktreeResult { branch_kept: true });
        insta::assert_snapshot!([listed, asked, empty, live, shown(&dialog, &[])].join("\n---\n"));
    }

    #[test]
    fn staged_changes_are_not_discardable() {
        let mut status = clean(None);
        status.staged = vec![file("a.rs", ChangeKind::Modified)];
        status.unstaged = vec![
            file("a.rs", ChangeKind::Modified),
            file("c.rs", ChangeKind::Conflicted),
        ];
        let plan = remove_plan(&status);
        assert_eq!(plan.uncommitted, ["a.rs", "c.rs"]);
        assert_eq!(plan.discardable, ["a.rs"]);
        let mut dialog = WorktreeDialog::new(Kind::Remove, &task(true)).unwrap();
        dialog.step = Step::Remove(RemovePlan {
            uncommitted: vec!["a.rs".into()],
            discardable: vec![],
        });
        assert!(!dialog.ask_discard());
        assert!(dialog.intro().contains("unstage"));
    }

    #[test]
    fn a_pick_moves_its_original_only_once_merged() {
        let pick = Pick {
            original_id: Ulid(2),
            original_title: "Fix login".into(),
            others: vec![],
            removing: false,
        };
        let mut dialog = WorktreeDialog::for_pick(&task(true), pick).unwrap();
        assert_eq!(dialog.kind, Kind::Merge);
        assert_eq!(dialog.picked_original(), None);
        dialog.merged(TaskMergeResult {
            merged: false,
            conflicts: vec!["a.rs".into()],
        });
        assert_eq!(dialog.picked_original(), None, "a conflict moves nothing");
        dialog.merged(TaskMergeResult {
            merged: true,
            conflicts: vec![],
        });
        assert_eq!(dialog.picked_original(), Some(Ulid(2)));
        let plain = WorktreeDialog::new(Kind::Merge, &task(true)).unwrap();
        assert_eq!(plain.pick, None);
    }

    #[test]
    fn a_removal_deleting_the_branch_says_so() {
        let mut dialog = WorktreeDialog::new(Kind::Remove, &task(true)).unwrap();
        dialog.removed(RemoveWorktreeResult { branch_kept: false });
        assert_eq!(
            dialog.intro(),
            "Worktree removed and the branch mkrew/fix-login-7 deleted."
        );
    }
}
