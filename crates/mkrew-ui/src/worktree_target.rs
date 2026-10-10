//! The target of the Files and Git views (SPEC §20E.1): the workspace folder ("Project") or
//! one of its tasks' worktrees, picked in the views' "Project | <task worktree>…" chooser, and
//! the `⎇ mkrew/<branch>` line of a worktree task's card. Pure and tested headlessly.

use std::path::{Path, PathBuf};

use mkrew_protocol::Task;
use ulid::Ulid;

/// One chip of the chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// `None` = the project folder.
    pub worktree_task_id: Option<Ulid>,
    pub label: String,
    /// The worktree's branch line (for the tooltip).
    pub branch: Option<String>,
}

/// The targets of `project_id`: none while none of its tasks has a worktree (the chooser is
/// hidden), else "Project" then every task with a worktree, oldest first.
pub fn choices(project_id: Ulid, tasks: &[Task]) -> Vec<Choice> {
    let mut worktrees: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.project_id == project_id && task.worktree_path.is_some())
        .collect();
    if worktrees.is_empty() {
        return vec![];
    }
    worktrees.sort_by_key(|task| (task.created_at, task.id));
    let project = Choice {
        worktree_task_id: None,
        label: "Project".to_owned(),
        branch: None,
    };
    std::iter::once(project)
        .chain(worktrees.into_iter().map(|task| Choice {
            worktree_task_id: Some(task.id),
            label: task.title.clone(),
            branch: branch_line(task),
        }))
        .collect()
}

/// Whether `target` can still be shown: the project always, a task while it has a worktree.
pub fn exists(project_id: Ulid, target: Option<Ulid>, tasks: &[Task]) -> bool {
    target.is_none_or(|task_id| {
        tasks.iter().any(|task| {
            task.id == task_id && task.project_id == project_id && task.worktree_path.is_some()
        })
    })
}

/// The card's `⎇ mkrew/<branch>` line, once the task's worktree exists.
pub fn branch_line(task: &Task) -> Option<String> {
    let branch = task
        .branch
        .as_deref()
        .filter(|_| task.worktree_path.is_some())?;
    Some(format!("⎇ {branch}"))
}

/// The worktree folder matching the project folder `project_path` (for "Open in Zed"): the
/// worktree root plus the project's path inside its repository, the repository being the
/// nearest folder up holding `.git` (a folder, or a file in a worktree). Outside a repository,
/// the worktree root itself. Keep in step with the daemon's `task_dir` (`server/target.rs`),
/// which resolves the folder the views read.
pub fn project_dir(project_path: &Path, worktree_root: &Path) -> PathBuf {
    let prefix = project_path
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .and_then(|repo| project_path.strip_prefix(repo).ok());
    match prefix {
        Some(prefix) => worktree_root.join(prefix),
        None => worktree_root.to_path_buf(),
    }
}

/// `rel` (`/`-separated, relative to `base`) below `base` with native separators. An empty
/// `rel` is `base` itself, without a trailing separator (the OS file manager needs that).
pub fn join_relative(base: PathBuf, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|part| !part.is_empty())
        .fold(base, |full, part| full.join(part))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::TaskStatus;

    fn task(project_id: Ulid, n: u128, title: &str, worktree: bool) -> Task {
        Task {
            id: Ulid(n),
            project_id,
            title: title.into(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Review,
            position: 0.0,
            checkpoint: None,
            error_flag: false,
            worktree,
            worktree_path: worktree.then(|| format!("/data/wt/{n}")),
            branch: worktree.then(|| format!("mkrew/{}-{n}", title.to_lowercase())),
            fanout_of: None,
            created_at: 100 - n as i64,
            updated_at: 0,
        }
    }

    #[test]
    fn the_chooser_lists_the_project_then_the_worktrees_oldest_first() {
        let project = Ulid(1);
        let mut other = task(Ulid(2), 5, "Elsewhere", true);
        other.project_id = Ulid(2);
        let mut pending = task(project, 6, "Pending", false);
        pending.worktree = true;
        assert!(choices(project, &[pending.clone(), other.clone()]).is_empty());

        let tasks = [
            task(project, 3, "Fix", true),
            pending,
            task(project, 4, "Docs", true),
            other,
        ];
        let choices = choices(project, &tasks);
        let line = choices
            .iter()
            .map(|choice| match &choice.branch {
                Some(branch) => format!("{} ({branch})", choice.label),
                None => choice.label.clone(),
            })
            .collect::<Vec<_>>()
            .join(" | ");
        insta::assert_snapshot!(line);
        assert_eq!(choices[1].worktree_task_id, Some(Ulid(4)));
    }

    /// The Files tree's root is `""`: Explorer must get the folder itself, not `proj\`.
    #[test]
    fn a_relative_path_joins_below_its_folder_and_empty_means_the_folder() {
        let base = PathBuf::from("proj");
        assert_eq!(
            join_relative(base.clone(), "src/a.rs"),
            Path::new("proj").join("src").join("a.rs")
        );
        assert_eq!(join_relative(base.clone(), "").as_os_str(), "proj");
        assert_eq!(
            join_relative(base, "src//a.rs/"),
            Path::new("proj").join("src").join("a.rs")
        );
    }

    #[test]
    fn a_target_exists_while_its_task_has_a_worktree() {
        let project = Ulid(1);
        let tasks = [task(project, 3, "Fix", true), task(project, 4, "No", false)];
        assert!(exists(project, None, &[]));
        assert!(exists(project, Some(Ulid(3)), &tasks));
        assert!(!exists(project, Some(Ulid(4)), &tasks), "worktree removed");
        assert!(!exists(project, Some(Ulid(9)), &tasks), "task deleted");
        assert!(
            !exists(Ulid(2), Some(Ulid(3)), &tasks),
            "another workspace's"
        );
    }

    #[test]
    fn the_card_names_the_branch_once_the_worktree_exists() {
        let mut fix = task(Ulid(1), 3, "Fix", true);
        assert_eq!(branch_line(&fix).as_deref(), Some("⎇ mkrew/fix-3"));
        fix.worktree_path = None;
        assert_eq!(branch_line(&fix), None);
    }

    #[test]
    fn the_worktree_folder_keeps_the_projects_place_in_its_repo() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("app/web")).unwrap();
        let worktree = Path::new("/wt/t1");
        assert_eq!(project_dir(&repo, worktree), worktree);
        assert_eq!(
            project_dir(&repo.join("app/web"), worktree),
            worktree.join("app").join("web")
        );
        let plain = temp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(project_dir(&plain, worktree), worktree);
    }
}
