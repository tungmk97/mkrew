//! The Git view's History section (SPEC §12B): the last 50 commits (`git.log`), the selected
//! commit and its changed files (`git.show`). The files' diffs go through the Git view's own
//! diff viewer ([`crate::git_view::GitView`] with [`crate::git_view::Group::Commit`]).
//! Pure and tested headlessly.

use mkrew_protocol::{ChangedFile, Commit, CommitDetail};

use crate::git_graph::{self, GraphRow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Commits {
    Loading,
    Loaded(Vec<Commit>),
    Failed(String),
}

/// The selected commit's changed files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    /// No commit selected.
    None,
    Loading,
    Loaded(Vec<ChangedFile>),
    Failed(String),
}

#[derive(Debug)]
pub struct History {
    pub commits: Commits,
    /// The lane graph of the listed commits, one row each ([`git_graph::layout`]).
    pub graph: Vec<GraphRow>,
    /// The hash of the selected (expanded) commit.
    pub selected: Option<String>,
    pub detail: Detail,
}

impl Default for History {
    fn default() -> Self {
        History {
            commits: Commits::Loading,
            graph: Vec::new(),
            selected: None,
            detail: Detail::None,
        }
    }
}

impl History {
    /// The `git.log` answer (the view coalesces the requests, so leaving and re-entering the
    /// History never sends two): returns the commit to show. The selection stays on its commit
    /// if it is still listed, else moves to the first one. A failed refresh of a loaded list
    /// keeps it (the error is returned for the view to note).
    pub fn logged(&mut self, loaded: Result<Vec<Commit>, String>) -> Logged {
        let commits = match loaded {
            Ok(commits) => commits,
            Err(error) if matches!(self.commits, Commits::Loaded(_)) => {
                return Logged {
                    show: None,
                    error: Some(error),
                };
            }
            Err(error) => {
                self.commits = Commits::Failed(error);
                self.graph.clear();
                self.selected = None;
                self.detail = Detail::None;
                return Logged::default();
            }
        };
        let kept = self
            .selected
            .as_ref()
            .filter(|hash| commits.iter().any(|commit| commit.hash == **hash))
            .cloned();
        let next = kept.or_else(|| commits.first().map(|commit| commit.hash.clone()));
        self.graph = git_graph::layout(&commits);
        self.commits = Commits::Loaded(commits);
        let show = match next {
            Some(hash) => self.select(hash),
            None => {
                self.selected = None;
                self.detail = Detail::None;
                None
            }
        };
        Logged { show, error: None }
    }

    /// The listed commits (none while loading or failed).
    pub fn commits(&self) -> &[Commit] {
        match &self.commits {
            Commits::Loaded(commits) => commits,
            Commits::Loading | Commits::Failed(_) => &[],
        }
    }

    /// Selects commit `hash`. Returns it when its files have to be read: another commit, or
    /// the same one whose files failed (a retry).
    pub fn select(&mut self, hash: String) -> Option<String> {
        if self.selected.as_ref() == Some(&hash) && !matches!(self.detail, Detail::Failed(_)) {
            return None;
        }
        self.selected = Some(hash.clone());
        self.detail = Detail::Loading;
        Some(hash)
    }

    /// The commit `by` places after (or before) the selected one, stopping at the ends (where
    /// nothing happens); returns it when it has to be read.
    pub fn step(&mut self, by: isize) -> Option<String> {
        let commits = self.commits();
        let at = self
            .selected
            .as_ref()
            .and_then(|hash| commits.iter().position(|commit| commit.hash == *hash));
        let to = match at {
            Some(at) => at
                .saturating_add_signed(by)
                .min(commits.len().checked_sub(1)?),
            None => 0,
        };
        if Some(to) == at {
            return None;
        }
        let hash = commits.get(to)?.hash.clone();
        self.select(hash)
    }

    /// The `git.show` answer for commit `hash`; `true` = it is the selected commit's (shown).
    pub fn shown(&mut self, hash: &str, loaded: Result<CommitDetail, String>) -> bool {
        if self.selected.as_deref() != Some(hash) {
            return false;
        }
        self.detail = match loaded {
            Ok(detail) => Detail::Loaded(detail.files),
            Err(error) => Detail::Failed(error),
        };
        true
    }

    /// The selected commit's changed files (none until loaded).
    pub fn files(&self) -> &[ChangedFile] {
        match &self.detail {
            Detail::Loaded(files) => files,
            Detail::None | Detail::Loading | Detail::Failed(_) => &[],
        }
    }
}

/// What a `git.log` answer asks of the view.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Logged {
    /// The newly selected commit, whose files are to be read.
    pub show: Option<String>,
    /// A refresh failed; the shown list stays.
    pub error: Option<String>,
}

/// The first 7 characters of a commit hash.
pub fn short_hash(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

/// A commit's age at `now_ms` in one unit: `now`, `5m`, `3h`, `2d`, `4mo`, `1y`.
pub fn age(authored_at: i64, now_ms: i64) -> String {
    let minutes = (now_ms - authored_at).max(0) / 60_000;
    let (hours, days) = (minutes / 60, minutes / (60 * 24));
    match days {
        _ if minutes == 0 => "now".to_owned(),
        _ if hours == 0 => format!("{minutes}m"),
        0 => format!("{hours}h"),
        1..30 => format!("{days}d"),
        30..365 => format!("{}mo", days / 30),
        _ => format!("{}y", days / 365),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str) -> Commit {
        Commit {
            hash: hash.into(),
            parents: vec![],
            author_name: "Ann".into(),
            author_email: "ann@example.com".into(),
            authored_at: 0,
            summary: format!("commit {hash}"),
            body: String::new(),
        }
    }

    fn detail(hash: &str, paths: &[&str]) -> CommitDetail {
        CommitDetail {
            commit: commit(hash),
            files: paths
                .iter()
                .map(|path| ChangedFile {
                    path: (*path).into(),
                    old_path: None,
                    kind: mkrew_protocol::ChangeKind::Modified,
                    added: Some(1),
                    removed: Some(1),
                })
                .collect(),
        }
    }

    #[test]
    fn the_first_commit_is_selected_and_kept_across_refreshes() {
        let mut history = History::default();
        let logged = history.logged(Ok(vec![commit("b"), commit("a")]));
        assert_eq!(logged.show.as_deref(), Some("b"));
        assert_eq!(history.detail, Detail::Loading);
        assert!(history.shown("b", Ok(detail("b", &["x.rs"]))));
        assert_eq!(history.files().len(), 1);
        history.select("a".into());
        // A new commit on top: the selection stays on `a`, nothing to read.
        let logged = history.logged(Ok(vec![commit("c"), commit("b"), commit("a")]));
        assert_eq!(logged, Logged::default());
        assert_eq!(history.selected.as_deref(), Some("a"));
        // `a` is gone (amended elsewhere): the first commit.
        let logged = history.logged(Ok(vec![commit("c")]));
        assert_eq!(logged.show.as_deref(), Some("c"));
    }

    #[test]
    fn an_empty_or_failed_log_selects_nothing_and_a_failed_refresh_keeps_the_list() {
        let mut history = History::default();
        assert_eq!(history.logged(Ok(vec![])), Logged::default());
        assert_eq!(
            (history.selected.as_ref(), &history.detail),
            (None, &Detail::None)
        );
        history.logged(Ok(vec![commit("a")]));
        let logged = history.logged(Err("index.lock".into()));
        assert_eq!(logged.error.as_deref(), Some("index.lock"));
        assert_eq!(history.commits().len(), 1);
        let mut fresh = History::default();
        fresh.logged(Err("not a repo".into()));
        assert_eq!(fresh.commits, Commits::Failed("not a repo".into()));
    }

    #[test]
    fn answers_for_another_commit_are_dropped_and_a_failure_can_be_retried() {
        let mut history = History::default();
        history.logged(Ok(vec![commit("b"), commit("a")]));
        history.select("a".into());
        assert!(!history.shown("b", Ok(detail("b", &["x"]))));
        assert_eq!(history.detail, Detail::Loading);
        assert!(history.shown("a", Err("boom".into())));
        assert_eq!(history.select("a".into()).as_deref(), Some("a"));
        assert_eq!(history.select("a".into()), None);
    }

    #[test]
    fn stepping_stops_at_the_ends() {
        let mut history = History::default();
        assert_eq!(history.step(1), None);
        history.logged(Ok(vec![commit("c"), commit("b"), commit("a")]));
        assert_eq!(history.step(-1), None);
        assert_eq!(history.step(1).as_deref(), Some("b"));
        assert_eq!(history.step(5).as_deref(), Some("a"));
        assert_eq!(history.step(1), None);
        // At the end with its files failed: no silent retry.
        history.shown("a", Err("boom".into()));
        assert_eq!(history.step(1), None);
    }

    #[test]
    fn ages_use_one_unit() {
        let minute = 60_000;
        let day = 24 * 60 * minute;
        let now = 1_000 * day;
        let age_of = |ago: i64| age(now - ago, now);
        assert_eq!(age_of(30_000), "now");
        assert_eq!(age_of(5 * minute), "5m");
        assert_eq!(age_of(3 * 60 * minute + 5 * minute), "3h");
        assert_eq!(age_of(2 * day), "2d");
        assert_eq!(age_of(65 * day), "2mo");
        assert_eq!(age_of(400 * day), "1y");
        // Clock skew: a commit "from the future".
        assert_eq!(age(now + day, now), "now");
    }

    #[test]
    fn short_hashes_take_seven_characters() {
        assert_eq!(short_hash("0123456789abcdef"), "0123456");
        assert_eq!(short_hash("abc"), "abc");
    }
}
