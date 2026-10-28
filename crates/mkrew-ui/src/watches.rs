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
    /// view (`git`) while that view is on screen. Callers pass `None` for a view that is
    /// hidden or that shows the project folder.
    pub fn of(workspace: Option<Ulid>, files: Option<Ulid>, git: Option<Ulid>) -> Self {
        let mut needs = BTreeSet::new();
        if let Some(project) = workspace {
            needs.insert((project, None));
            needs.extend(
                [files, git]
                    .into_iter()
                    .flatten()
                    .map(|t| (project, Some(t))),
            );
        }
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
        let before = Needs::of(Some(a), None, Some(task));
        let after = Needs::of(Some(b), None, None);
        assert_eq!(
            before.released(&after),
            [release(a, None), release(a, Some(task))]
        );
    }

    /// Hiding the panel (or leaving the Diff view) gives back only the worktree: the shown
    /// project keeps its watcher for the title bar. Picking another worktree gives back the
    /// old one, and a view on the project folder releases nothing.
    #[test]
    fn hiding_a_worktree_view_keeps_the_shown_project() {
        let (a, t1, t2) = (Ulid(1), Ulid(3), Ulid(4));
        let shown = Needs::of(Some(a), Some(t1), None);
        let hidden = Needs::of(Some(a), None, None);
        assert_eq!(shown.released(&hidden), [release(a, Some(t1))]);
        assert!(
            hidden.released(&shown).is_empty(),
            "showing it again takes, not gives"
        );
        assert_eq!(
            shown.released(&Needs::of(Some(a), Some(t2), None)),
            [release(a, Some(t1))]
        );
        // Files and Git on the same worktree count once.
        assert_eq!(
            Needs::of(Some(a), Some(t1), Some(t1)),
            Needs::of(Some(a), None, Some(t1))
        );
    }
}
