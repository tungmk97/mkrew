//! The git state of the shown workspace (SPEC §10, ADR 0029): the `Git N` badge and the branch
//! name of the chat's branch chip. One `RepoSummary` per repository of the workspace folder,
//! reduced from its `git.status` (Working mode). Pure; `main.rs` loads it.

use std::collections::HashSet;

use mkrew_protocol::GitStatus;

/// One repository of a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepoSummary {
    /// `""` = the workspace folder itself, else the child folder name (ADR 0029).
    pub repo: String,
    /// `None` = detached HEAD / unknown.
    pub branch: Option<String>,
    /// Full HEAD hash; `None` = no commit yet.
    pub head: Option<String>,
    /// Distinct paths in staged ∪ unstaged.
    pub modified: usize,
    pub untracked: usize,
}

impl RepoSummary {
    pub fn of(repo: &str, status: &GitStatus) -> Self {
        let modified = status
            .staged
            .iter()
            .chain(&status.unstaged)
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>()
            .len();
        Self {
            repo: repo.to_owned(),
            branch: status.branch.clone(),
            head: status.head.clone(),
            modified,
            untracked: status.untracked.len(),
        }
    }

    /// Changed files of this repository.
    pub fn changed(&self) -> usize {
        self.modified + self.untracked
    }

    /// The workspace name for the folder itself, else the child folder.
    pub fn label<'a>(&'a self, workspace: &'a str) -> &'a str {
        if self.repo.is_empty() {
            workspace
        } else {
            &self.repo
        }
    }

    /// `Clean`, `1 change`, `N changes`.
    pub fn state(&self) -> String {
        match self.changed() {
            0 => "Clean".to_owned(),
            1 => "1 change".to_owned(),
            n => format!("{n} changes"),
        }
    }

    /// The checked-out branch, `detached` on a detached HEAD.
    pub fn branch_name(&self) -> &str {
        self.branch.as_deref().unwrap_or("detached")
    }
}

/// Every repository of the shown workspace, in `git.repos` order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitSummary {
    pub repos: Vec<RepoSummary>,
}

impl GitSummary {
    /// Changed files over all repositories: the `Git N` badge.
    pub fn changed(&self) -> usize {
        self.repos.iter().map(RepoSummary::changed).sum()
    }

    /// `master` for one repository, `2 repos` for several; `None` when there is none.
    pub fn branch_label(&self) -> Option<String> {
        match self.repos.as_slice() {
            [] => None,
            [one] => Some(one.branch_name().to_owned()),
            many => Some(format!("{} repos", many.len())),
        }
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{ChangeKind, ChangedFile, GitStatus};

    use super::*;

    fn file(path: &str, kind: ChangeKind) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            old_path: None,
            kind,
            added: Some(1),
            removed: Some(0),
        }
    }

    fn status(branch: Option<&str>, head: Option<&str>) -> GitStatus {
        GitStatus {
            branch: branch.map(str::to_owned),
            head: head.map(str::to_owned),
            ..GitStatus::default()
        }
    }

    fn summary(repos: &[(&str, GitStatus)]) -> GitSummary {
        GitSummary {
            repos: repos
                .iter()
                .map(|(name, s)| RepoSummary::of(name, s))
                .collect(),
        }
    }

    #[test]
    fn one_repo_label_is_its_branch() {
        let s = summary(&[("", status(Some("master"), Some("1a2b3c4d5e6f")))]);
        assert_eq!(s.branch_label().as_deref(), Some("master"));
    }

    #[test]
    fn many_repos_label_counts_them() {
        let s = summary(&[
            ("fe", status(Some("main"), Some("1a2b3c4d5e6f"))),
            ("be", status(Some("feature/x"), Some("9f8e7d6c5b4a"))),
        ]);
        assert_eq!(s.branch_label().as_deref(), Some("2 repos"));
    }

    /// The workspace folder has no `.git` (git.repos has no `""`), but two children do: the
    /// chip and the badge must still show.
    #[test]
    fn a_non_git_root_with_child_repos_is_never_blank() {
        let mut dirty = status(Some("main"), Some("aaaaaaaa"));
        dirty.unstaged = vec![file("a.rs", ChangeKind::Modified)];
        dirty.untracked = vec![file("n.txt", ChangeKind::Untracked)];
        let s = summary(&[("fe", dirty), ("be", status(Some("dev"), Some("bbbbbbbb")))]);
        assert_eq!(s.branch_label().as_deref(), Some("2 repos"));
        assert_eq!(s.changed(), 2);
        // One child only: its own branch, not "1 repos".
        let one = summary(&[("be", status(Some("dev"), Some("bbbbbbbb")))]);
        assert_eq!(one.branch_label().as_deref(), Some("dev"));
    }

    #[test]
    fn the_badge_sums_every_repo() {
        let mut a = status(Some("main"), Some("aaaaaaaa"));
        a.staged = vec![
            file("a.rs", ChangeKind::Modified),
            file("b.rs", ChangeKind::Added),
        ];
        let mut b = status(Some("main"), Some("bbbbbbbb"));
        b.unstaged = vec![file("c.rs", ChangeKind::Deleted)];
        b.untracked = vec![file("d.txt", ChangeKind::Untracked)];
        let s = summary(&[("fe", a), ("be", b)]);
        assert_eq!(s.repos[0].changed(), 2);
        assert_eq!(s.repos[1].changed(), 2);
        assert_eq!(s.changed(), 4);
    }

    /// A file staged and then edited again is in both lists: one changed file.
    #[test]
    fn staged_and_unstaged_changes_of_one_path_count_once() {
        let mut st = status(Some("main"), Some("aaaaaaaa"));
        st.staged = vec![file("a.rs", ChangeKind::Modified)];
        st.unstaged = vec![
            file("a.rs", ChangeKind::Modified),
            file("b.rs", ChangeKind::Deleted),
        ];
        st.untracked = vec![file("new.txt", ChangeKind::Untracked)];
        let repo = RepoSummary::of("", &st);
        assert_eq!((repo.modified, repo.untracked, repo.changed()), (2, 1, 3));
    }

    #[test]
    fn a_detached_head_still_has_a_label() {
        let s = summary(&[("", status(None, Some("1a2b3c4d5e6f")))]);
        assert_eq!(s.branch_label().as_deref(), Some("detached"));
    }

    #[test]
    fn no_repository_means_no_chip_and_no_badge() {
        let s = GitSummary::default();
        assert_eq!((s.branch_label(), s.changed()), (None, 0));
    }
}
