//! `task.fanout` (SPEC §20E.2): one task run by several agents, each in its own worktree.

use std::path::PathBuf;
use std::sync::Arc;

use mkrew_protocol::{
    ErrorCode, Outcome, SessionState, Task, TaskFanoutParams, TaskRunParams, TaskStatus,
};

use super::{Daemon, store_outcome};

impl Daemon {
    /// Creates one child task per profile (`fanout_of` = the task, title ` · <agent>`,
    /// worktree on), runs them all and moves the task to Running. Refused outside a git repo,
    /// for a running task, a child, or a task that already fanned out.
    pub(super) async fn task_fanout(&self, params: TaskFanoutParams) -> Outcome {
        let profiles = &params.agent_profiles;
        let range = TaskFanoutParams::MIN_PROFILES..=TaskFanoutParams::MAX_PROFILES;
        if !range.contains(&profiles.len()) {
            let message = format!(
                "pick {} to {} agent profiles, not {}",
                range.start(),
                range.end(),
                profiles.len()
            );
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        if params.cols == 0 || params.rows == 0 {
            return Outcome::error(ErrorCode::InvalidRequest, "cols and rows must be > 0");
        }
        if let Some(twice) = profiles
            .iter()
            .enumerate()
            .find_map(|(i, id)| profiles[..i].contains(id).then_some(id))
        {
            let message = format!("agent profile {twice} is picked twice");
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let mut names = Vec::with_capacity(profiles.len());
        for id in profiles {
            match self.load_profile(id) {
                Ok(profile) if profile.kind == mkrew_core::ProfileKind::Agent => {
                    names.push(profile.name);
                }
                Ok(_) => {
                    let message = format!("{id} is a shell profile, not an agent");
                    return Outcome::error(ErrorCode::InvalidRequest, message);
                }
                Err(outcome) => return outcome,
            }
        }
        let found = {
            let store = self.store();
            store.get_task(params.task_id).and_then(|task| {
                let project = store.get_project(task.project_id)?;
                let live = store
                    .list_sessions(Some(project.id))?
                    .iter()
                    .any(|s| s.task_id == Some(task.id) && s.state != SessionState::Exited);
                Ok((task, project, live))
            })
        };
        let (task, project, live) = match found {
            Ok(found) => found,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let refused = if live {
            Some("the task is running")
        } else if task.fanout_of.is_some() {
            Some("the task is itself a fan-out child")
        } else {
            None
        };
        if let Some(why) = refused {
            return Outcome::error(ErrorCode::InvalidState, format!("cannot fan out: {why}"));
        }
        let git = Arc::clone(&self.git);
        let dir = PathBuf::from(&project.path);
        let repo = tokio::task::spawn_blocking(move || git.repo(&dir)).await;
        if !matches!(repo, Ok(Ok(Some(_)))) {
            let message = "cannot fan out: the project is not in a git repository";
            return Outcome::error(ErrorCode::InvalidState, message);
        }

        let created: Result<Vec<Task>, _> = {
            // One lock for check and create: the children appear together and a concurrent
            // fan-out of the same task finds them. A store error midway can
            // leave a child without `fanout_of` (a plain task the user can delete).
            let store = self.store();
            match store.list_fanout_children(task.id) {
                Ok(existing) if !existing.is_empty() => {
                    let message = "cannot fan out: the task already fanned out";
                    return Outcome::error(ErrorCode::InvalidState, message);
                }
                Ok(_) => {}
                Err(e) => return store_outcome::<()>(Err(e)),
            }
            profiles
                .iter()
                .zip(&names)
                .map(|(id, name)| {
                    let title = format!("{} · {name}", task.title);
                    let child = store.create_task(
                        project.id,
                        &title,
                        &task.prompt,
                        Some(id),
                        None,
                        true,
                    )?;
                    store.set_task_fanout_of(child.id, task.id)
                })
                .collect()
        };
        let children = match created {
            Ok(children) => children,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        for child in &children {
            let _ = self.task_updated(Ok(child.clone()));
        }
        let mut failed = Vec::new();
        for child in &children {
            let run = TaskRunParams {
                task_id: child.id,
                cols: params.cols,
                rows: params.rows,
            };
            if let Outcome::Error(e) = self.task_run(run).await {
                tracing::warn!(task_id = %child.id, "fan-out child did not start: {}", e.message);
                failed.push((e.code, format!("{}: {}", child.title, e.message)));
            }
        }
        if failed.len() < children.len() {
            let moved = self
                .store()
                .set_task_status(task.id, TaskStatus::Running, false);
            let _ = self.task_updated(moved);
        }
        // The first failure's code stands for all of them.
        if let Some((code, _)) = failed.first() {
            let reasons: Vec<_> = failed.iter().map(|(_, reason)| reason.as_str()).collect();
            let message = format!(
                "{} of {} fan-out tasks did not start: {}",
                failed.len(),
                children.len(),
                reasons.join("; ")
            );
            return Outcome::error(*code, message);
        }
        let ids: Vec<_> = children.iter().map(|c| c.id).collect();
        let fresh: Result<Vec<Task>, _> = {
            let store = self.store();
            ids.iter().map(|id| store.get_task(*id)).collect()
        };
        store_outcome(fresh)
    }
}
