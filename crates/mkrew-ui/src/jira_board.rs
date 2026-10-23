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
    /// Only the issues assigned to the user (`Mine`); default `All`.
    pub mine: bool,
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
}
