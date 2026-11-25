//! Which daemon file watchers this window still needs (SPEC §7 `watch.release`, ADR 0087).
//! The daemon keeps a project's watcher after Files / Git / Docs asked for it;
//! [`Needs::released`] names the ones to give back when the window stops showing what needed
//! them. Pure and tested headlessly; `shell/workspace.rs` sends the requests.

use std::collections::BTreeSet;

use mkrew_protocol::WatchReleaseParams;
use ulid::Ulid;

/// The watchers in use: `(project, None)` = the project folder, `(project, Some(task))` = a
/// task worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Needs(BTreeSet<(Ulid, Option<Ulid>)>);

impl Needs {
    /// What showing `workspace` uses: always its project watcher (the title bar summary
    /// follows `git.changed`), plus the worktree of the Files view (`files`) and of the Git
    /// view (`git`) while that view is on screen. `open` is `(project, worktree task)` of every
    /// open file pane of EVERY workspace with loaded tabs, in a shown tab or window or not: a
    /// hidden workspace keeps its worktree watchers until its last file there closes.
    /// Callers pass `None` for a view that is hidden or that shows the project folder.
    pub fn of(
        workspace: Option<Ulid>,
        files: Option<Ulid>,
        git: Option<Ulid>,
        open: impl IntoIterator<Item = (Ulid, Ulid)>,
    ) -> Self {
        let mut needs = BTreeSet::new();
        if let Some(project) = workspace {
            needs.insert((project, None));
            needs.extend(
                [files, git]
                    .into_iter()
                    .flatten()
                    .map(|task| (project, Some(task))),
            );
        }
        needs.extend(
            open.into_iter()
                .map(|(project, task)| (project, Some(task))),
        );
        Self(needs)
    }

    /// The watchers `self` used and `now` does not: one `watch.release` each.
    pub fn released(&self, now: &Needs) -> Vec<WatchReleaseParams> {
        self.0
            .difference(&now.0)
            .map(|&(project_id, worktree_task_id)| WatchReleaseParams {
                project_id,
                worktree_task_id,
            })
            .collect()
    }

    /// What `self` and `other` both keep watched (one window per workspace, ADR 0113).
    pub fn union(mut self, other: Needs) -> Needs {
        self.0.extend(other.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(project_id: Ulid, worktree_task_id: Option<Ulid>) -> WatchReleaseParams {
        WatchReleaseParams {
            project_id,
            worktree_task_id,
        }
    }

    /// Switching workspace gives back the old one's project watcher and its worktree watcher.
    #[test]
    fn switching_workspace_releases_the_old_project_and_its_worktree() {
        let (a, b, task) = (Ulid(1), Ulid(2), Ulid(3));
        let before = Needs::of(Some(a), None, Some(task), []);
        let after = Needs::of(Some(b), None, None, []);
        assert_eq!(
            before.released(&after),
            [release(a, None), release(a, Some(task))]
        );
    }

    /// Hiding the panel (or closing the Diff pane) gives back only the worktree: the shown
    /// project keeps its watcher for the title bar. Picking another worktree gives back the
    /// old one, and a view on the project folder releases nothing.
    #[test]
    fn hiding_a_worktree_view_keeps_the_shown_project() {
        let (a, t1, t2) = (Ulid(1), Ulid(3), Ulid(4));
        let shown = Needs::of(Some(a), Some(t1), None, []);
        let hidden = Needs::of(Some(a), None, None, []);
        assert_eq!(shown.released(&hidden), [release(a, Some(t1))]);
        assert!(
            hidden.released(&shown).is_empty(),
            "showing it again takes, not gives"
        );
        assert_eq!(
            shown.released(&Needs::of(Some(a), Some(t2), None, [])),
            [release(a, Some(t1))]
        );
        // Files and Git on the same worktree count once.
        assert_eq!(
            Needs::of(Some(a), Some(t1), Some(t1), []),
            Needs::of(Some(a), None, Some(t1), [])
        );
    }

    /// A file pane of a worktree keeps that worktree watched without the Files tree on it
    /// (its `git.changed` re-reads the file); closing the pane gives the watcher back.
    #[test]
    fn an_open_worktree_file_keeps_its_worktree_watched() {
        let (a, t1) = (Ulid(1), Ulid(3));
        let open = Needs::of(Some(a), None, None, [(a, t1)]);
        assert_eq!(open, Needs::of(Some(a), Some(t1), None, []));
        assert_eq!(
            open.released(&Needs::of(Some(a), None, None, [])),
            [release(a, Some(t1))]
        );
    }

    /// Switching workspace keeps the worktrees a hidden workspace still has files open on
    /// (its project watcher and its Files/Git view worktrees go); closing its last file there
    /// gives the worktree back.
    #[test]
    fn a_hidden_workspace_keeps_the_worktrees_of_its_open_files() {
        let (a, b, t1, t2) = (Ulid(1), Ulid(2), Ulid(3), Ulid(4));
        let open = [(a, t1), (b, t2)];
        let a_shown = Needs::of(Some(a), None, None, open);
        let b_shown = Needs::of(Some(b), None, None, open);
        assert_eq!(
            a_shown.released(&b_shown),
            [release(a, None)],
            "A's file on t1 stays watched while B is shown"
        );
        let closed = Needs::of(Some(b), None, None, [(b, t2)]);
        assert_eq!(b_shown.released(&closed), [release(a, Some(t1))]);
    }

    #[test]
    fn union_keeps_both_projects_and_their_worktrees() {
        let (a, b, task) = (Ulid::new(), Ulid::new(), Ulid::new());
        let both =
            Needs::of(Some(a), Some(task), None, []).union(Needs::of(Some(b), None, None, []));
        assert_eq!(both.released(&Needs::default()).len(), 3);
        assert!(
            Needs::of(Some(a), None, None, [])
                .released(&both)
                .is_empty()
        );
    }
}
