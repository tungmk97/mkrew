//! The files of a Git view marked as viewed (SPEC §12B, ADR 0065), as the daemon last
//! confirmed them. Pure, so it is unit-tested; `GitView` owns one.

use std::collections::HashSet;

use mkrew_protocol::{ChangedFile, ViewedFile};

use crate::git_view::{Group, Target};

/// Viewed paths of the listed changes. The Staged group has its own set (a path can be staged
/// and unstaged with different diffs); every other group shares the other one. A History
/// commit's files (`Group::Commit`) are never viewed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewedSet {
    staged: HashSet<String>,
    other: HashSet<String>,
}

impl ViewedSet {
    /// The set a `git.viewed` answer describes.
    pub fn new(files: Vec<ViewedFile>) -> ViewedSet {
        let mut set = ViewedSet::default();
        for file in files {
            set.side_mut(file.staged).insert(file.path);
        }
        set
    }

    fn side_mut(&mut self, staged: bool) -> &mut HashSet<String> {
        if staged {
            &mut self.staged
        } else {
            &mut self.other
        }
    }

    /// Whether `path` of `group` is marked viewed.
    pub fn contains(&self, group: Group, path: &str) -> bool {
        match group {
            Group::Commit => false,
            Group::Staged => self.staged.contains(path),
            _ => self.other.contains(path),
        }
    }

    /// Marks or clears `path` of `group` (a no-op for a commit's files).
    pub fn set(&mut self, group: Group, path: &str, on: bool) {
        if group == Group::Commit {
            return;
        }
        let side = self.side_mut(group == Group::Staged);
        if on {
            side.insert(path.to_owned());
        } else {
            side.remove(path);
        }
    }

    /// `(viewed, listed)` over `files`: the header's `Viewed N/M`.
    pub fn count(&self, files: &[(Target, ChangedFile)]) -> (usize, usize) {
        let viewed = files
            .iter()
            .filter(|(target, _)| self.contains(target.group, &target.path))
            .count();
        (viewed, files.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::ChangeKind;

    fn viewed(path: &str, staged: bool) -> ViewedFile {
        ViewedFile {
            path: path.into(),
            staged,
        }
    }

    fn listed(group: Group, path: &str) -> (Target, ChangedFile) {
        let target = Target {
            group,
            path: path.into(),
            commit: None,
        };
        let file = ChangedFile {
            path: path.into(),
            old_path: None,
            kind: ChangeKind::Modified,
            added: Some(1),
            removed: Some(0),
        };
        (target, file)
    }

    #[test]
    fn the_staged_group_is_told_apart_from_every_other_group() {
        let set = ViewedSet::new(vec![viewed("a.rs", true), viewed("b.rs", false)]);
        assert!(set.contains(Group::Staged, "a.rs"));
        assert!(!set.contains(Group::Unstaged, "a.rs"));
        for group in [Group::Unstaged, Group::Untracked, Group::Base, Group::Since] {
            assert!(set.contains(group, "b.rs"), "{group:?}");
        }
        assert!(!set.contains(Group::Staged, "b.rs"));
    }

    #[test]
    fn a_commits_files_are_never_viewed() {
        let mut set = ViewedSet::new(vec![viewed("a.rs", false)]);
        assert!(!set.contains(Group::Commit, "a.rs"));
        set.set(Group::Commit, "c.rs", true);
        assert!(!set.contains(Group::Commit, "c.rs"));
        assert!(!set.contains(Group::Unstaged, "c.rs"));
    }

    #[test]
    fn set_marks_and_clears_one_group_only() {
        let mut set = ViewedSet::default();
        set.set(Group::Staged, "a.rs", true);
        assert!(set.contains(Group::Staged, "a.rs"));
        assert!(!set.contains(Group::Unstaged, "a.rs"));
        set.set(Group::Staged, "a.rs", false);
        assert_eq!(set, ViewedSet::default());
    }

    #[test]
    fn the_counter_counts_viewed_over_every_listed_file() {
        let set = ViewedSet::new(vec![viewed("a.rs", true)]);
        let files = [
            listed(Group::Staged, "a.rs"),
            listed(Group::Unstaged, "a.rs"),
            listed(Group::Untracked, "n.txt"),
        ];
        // Only the staged copy of a.rs is viewed.
        assert_eq!(set.count(&files), (1, 3));
        assert_eq!(set.count(&[]), (0, 0));
    }
}
