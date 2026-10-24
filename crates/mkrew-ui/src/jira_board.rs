//! The Board's Jira side of one workspace (SPEC §20I): which side shows, the issue list being
//! loaded, the keys field, and the link from an issue to its task. Pure; `shell/jira.rs`
//! renders it.

use mkrew_protocol::Task;
use ulid::Ulid;

use crate::jira::Issue;
use crate::text_box::Input;

/// Which side of the Board shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Tasks,
    Jira,
}

/// The issue list of a workspace; kept until Refresh or app restart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Load {
    #[default]
    NotLoaded,
    Loading,
    Loaded(Vec<Issue>),
    Failed(String),
}

#[derive(Debug, Default)]
pub struct JiraView {
    pub source: Source,
    pub load: Load,
    /// Bumped per search; an answer with an older serial is dropped.
    serial: u64,
    /// `Some` while the keys field shows (no keys yet, or Edit).
    pub keys_field: Option<Input>,
    /// Why the last keys Save failed (shown under the field).
    pub keys_error: Option<String>,
    pub saving_keys: bool,
    /// The issue whose draft is being fetched (its row shows the dots).
    pub opening: Option<String>,
    /// `(issue key, reason)` of the last failed draft fetch, shown under that row.
    pub row_error: Option<(String, String)>,
    /// The toolbar's "Filter issues" text (matches key or summary, ignoring case).
    pub filter: Input,
}

impl JiraView {
    /// A search was sent; returns its serial for [`JiraView::finish_load`].
    pub fn start_load(&mut self) -> u64 {
        self.serial += 1;
        self.load = Load::Loading;
        self.serial
    }

    /// Applies a search answer unless a newer search started since; returns whether it did.
    pub fn finish_load(&mut self, serial: u64, result: Result<Vec<Issue>, String>) -> bool {
        if serial != self.serial {
            return false;
        }
        self.load = match result {
            Ok(issues) => Load::Loaded(issues),
            Err(reason) => Load::Failed(reason),
        };
        true
    }

    /// Drops the list (and any search in flight), e.g. after the `All | Mine` filter changed.
    pub fn forget(&mut self) {
        self.serial += 1;
        self.load = Load::NotLoaded;
    }

    /// Something is in flight (the status tick keeps the dots moving).
    pub fn busy(&self) -> bool {
        self.load == Load::Loading || self.opening.is_some() || self.saving_keys
    }

    /// Shows the keys field, filled with the saved keys.
    pub fn edit_keys(&mut self, keys: &[String]) {
        self.keys_field = Some(Input::from(keys.join(", ")));
        self.keys_error = None;
    }

    /// The field's text split on commas; the daemon trims, upper-cases and checks them.
    pub fn typed_keys(&self) -> Vec<String> {
        let text = self
            .keys_field
            .as_ref()
            .map(Input::as_str)
            .unwrap_or_default();
        text.split(',')
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

/// The task of `project_id` whose title is `key` followed by a space (SPEC §20I).
pub fn linked_task<'a>(tasks: &'a [Task], project_id: Ulid, key: &str) -> Option<&'a Task> {
    tasks.iter().find(|task| {
        task.project_id == project_id
            && task
                .title
                .strip_prefix(key)
                .is_some_and(|rest| rest.starts_with(' '))
    })
}

/// Whether a Jira status is the In Progress one (its own icon, pill and place in the list).
pub fn is_in_progress(status: &str) -> bool {
    status.eq_ignore_ascii_case("in progress")
}

/// The leading `[xxx]` area tag of an issue title and the title without it: `[BE] Fix x` is
/// `(Some("BE"), "Fix x")`. No tag, an empty `[]` or an unclosed `[` leave the title as is.
pub fn split_area(title: &str) -> (Option<&str>, &str) {
    let Some(rest) = title.strip_prefix('[') else {
        return (None, title);
    };
    match rest.split_once(']') {
        Some((area, after)) if !area.trim().is_empty() => (Some(area.trim()), after.trim_start()),
        _ => (None, title),
    }
}

/// One status group of the list (`In Progress`, `To Do`, ...), items in list order.
#[derive(Debug, PartialEq, Eq)]
pub struct Section<'a> {
    pub status: &'a str,
    pub issues: Vec<&'a Issue>,
}

/// Whether `issue` matches the filter text (key or summary, ignoring case); empty matches all.
pub fn matches_filter(issue: &Issue, filter: &str) -> bool {
    let needle = filter.trim().to_lowercase();
    needle.is_empty()
        || issue.key.to_lowercase().contains(&needle)
        || issue.summary.to_lowercase().contains(&needle)
}

/// The issues that match `filter`, grouped by status: `In Progress` first, `To Do` second, any
/// other status after in first-seen order; order inside a section is the list's.
pub fn sections<'a>(issues: &'a [Issue], filter: &str) -> Vec<Section<'a>> {
    let mut out: Vec<Section<'a>> = Vec::new();
    for issue in issues.iter().filter(|issue| matches_filter(issue, filter)) {
        match out
            .iter_mut()
            .find(|section| section.status.eq_ignore_ascii_case(&issue.status))
        {
            Some(section) => section.issues.push(issue),
            None => out.push(Section {
                status: &issue.status,
                issues: vec![issue],
            }),
        }
    }
    // Stable: the other statuses keep their first-seen order.
    out.sort_by_key(|section| {
        if is_in_progress(section.status) {
            0
        } else if section.status.eq_ignore_ascii_case("to do") {
            1
        } else {
            2
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::TaskStatus;

    use super::*;

    fn task(project_id: Ulid, title: &str) -> Task {
        Task {
            id: Ulid::new(),
            project_id,
            title: title.to_owned(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Todo,
            position: 1.0,
            checkpoint: None,
            error_flag: false,
            worktree: false,
            worktree_path: None,
            branch: None,
            fanout_of: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn issue(key: &str) -> Issue {
        Issue {
            key: key.into(),
            summary: "s".into(),
            status: "To Do".into(),
            kind: "Bug".into(),
        }
    }

    #[test]
    fn a_task_links_to_the_issue_its_title_starts_with() {
        let (p, other) = (Ulid::new(), Ulid::new());
        let tasks = vec![
            task(p, "PROJ-123 Fix login"),
            task(p, "PROJ-123x odd"),
            task(other, "PROJ-9 elsewhere"),
            task(p, "PROJ-1"),
        ];
        assert_eq!(
            linked_task(&tasks, p, "PROJ-123").map(|t| t.title.as_str()),
            Some("PROJ-123 Fix login")
        );
        assert!(
            linked_task(&tasks, p, "PROJ-12").is_none(),
            "prefix of a longer key"
        );
        assert!(
            linked_task(&tasks, p, "PROJ-9").is_none(),
            "other workspace"
        );
        assert!(
            linked_task(&tasks, p, "PROJ-1").is_none(),
            "title is only the key, no space"
        );
    }

    #[test]
    fn only_the_newest_load_lands() {
        let mut view = JiraView::default();
        assert_eq!(view.load, Load::NotLoaded);
        let first = view.start_load();
        let second = view.start_load();
        assert!(view.busy());
        assert!(
            !view.finish_load(first, Ok(vec![issue("OLD-1")])),
            "stale answer dropped"
        );
        assert_eq!(view.load, Load::Loading);
        assert!(view.finish_load(second, Err("Jira answered 500: x".into())));
        assert_eq!(view.load, Load::Failed("Jira answered 500: x".into()));
        let third = view.start_load();
        assert!(view.finish_load(third, Ok(vec![issue("PROJ-1")])));
        assert_eq!(view.load, Load::Loaded(vec![issue("PROJ-1")]));
        assert!(!view.busy());
        // A filter change mid-search: the old answer never lands.
        let fourth = view.start_load();
        view.forget();
        assert!(!view.finish_load(fourth, Ok(vec![issue("ALL-1")])));
        assert_eq!(view.load, Load::NotLoaded);
        assert!(!view.busy());
    }

    #[test]
    fn the_keys_field_starts_from_the_saved_keys_and_splits_on_commas() {
        let mut view = JiraView::default();
        view.edit_keys(&["PROJ".into(), "BILL".into()]);
        assert_eq!(
            view.keys_field.as_ref().map(Input::as_str),
            Some("PROJ, BILL")
        );
        view.keys_field = Some(Input::from(" proj ,, bill "));
        assert_eq!(view.typed_keys(), ["proj", "bill"]);
    }

    fn issue_in(key: &str, status: &str) -> Issue {
        Issue {
            status: status.into(),
            ..issue(key)
        }
    }

    #[test]
    fn the_area_tag_is_split_off_the_title() {
        assert_eq!(split_area("[BE] Fix x"), (Some("BE"), "Fix x"));
        assert_eq!(split_area("[FE]Fix x"), (Some("FE"), "Fix x"));
        assert_eq!(split_area("Fix x"), (None, "Fix x"));
        assert_eq!(split_area("Fix [BE] x"), (None, "Fix [BE] x"));
        assert_eq!(split_area("[] Fix x"), (None, "[] Fix x"));
        assert_eq!(split_area("[ ] Fix x"), (None, "[ ] Fix x"));
        assert_eq!(split_area("[BE Fix x"), (None, "[BE Fix x"));
        assert_eq!(split_area("[BE]"), (Some("BE"), ""));
    }

    #[test]
    fn sections_put_in_progress_first_then_to_do_then_others_first_seen() {
        let issues = vec![
            issue_in("A-1", "To Do"),
            issue_in("A-2", "Blocked"),
            issue_in("A-3", "In Progress"),
            issue_in("A-4", "In Review"),
            issue_in("A-5", "To Do"),
            issue_in("A-6", "in progress"),
        ];
        let sections = sections(&issues, "");
        let shown: Vec<(&str, Vec<&str>)> = sections
            .iter()
            .map(|s| (s.status, s.issues.iter().map(|i| i.key.as_str()).collect()))
            .collect();
        assert_eq!(
            shown,
            [
                ("In Progress", vec!["A-3", "A-6"]),
                ("To Do", vec!["A-1", "A-5"]),
                ("Blocked", vec!["A-2"]),
                ("In Review", vec!["A-4"]),
            ]
        );
    }

    #[test]
    fn the_filter_matches_key_or_summary_and_drops_empty_sections() {
        let mut a = issue_in("EIS-1", "To Do");
        a.summary = "Fix Login".into();
        let b = issue_in("EIS-2", "In Progress");
        let issues = vec![a, b];
        assert_eq!(sections(&issues, "login").len(), 1);
        assert_eq!(sections(&issues, " eis-2 ")[0].status, "In Progress");
        assert!(sections(&issues, "nope").is_empty());
        assert_eq!(sections(&issues, "").len(), 2);
    }
}
