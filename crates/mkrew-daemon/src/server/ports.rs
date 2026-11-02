//! `port.list` / `port.stop` (spec 2026-11-01, ADR 0091).

use std::collections::HashMap;
use std::path::PathBuf;

use mkrew_core::run;
use mkrew_protocol::{ErrorCode, Outcome, PortRow, PortStopParams, SessionState};

use super::{Daemon, ok, store_outcome};
use crate::ports::{self, Folder, Known, Owner};
use crate::session::Command;

impl Daemon {
    pub(super) async fn port_list(&self) -> Outcome {
        match self.port_rows().await {
            Ok(rows) => ok(&rows),
            Err(outcome) => outcome,
        }
    }

    /// Lists again, then acts only when `pid` still holds `port`.
    pub(super) async fn port_stop(&self, params: PortStopParams) -> Outcome {
        let rows = match self.port_rows().await {
            Ok(rows) => rows,
            Err(outcome) => return outcome,
        };
        let Some(row) = rows
            .into_iter()
            .find(|r| r.port == params.port && r.pid == params.pid)
        else {
            let message = format!(
                "port :{} is no longer held by pid {}",
                params.port, params.pid
            );
            return Outcome::error(ErrorCode::NotFound, message);
        };
        if let (Some(session_id), Some(name)) = (row.session_id, row.run_command.as_deref()) {
            tracing::info!(%session_id, name, port = row.port, "stopping a run command from the port list");
            self.hub.command(session_id, Command::Kill { force: false });
            return ok(&());
        }
        if self.hub.live_pids().iter().any(|(_, pid)| *pid == row.pid) {
            return Outcome::error(
                ErrorCode::InvalidState,
                "that is the pane's own process; close the pane instead",
            );
        }
        tracing::info!(
            pid = row.pid,
            port = row.port,
            process = row.process,
            "stopping a port's process"
        );
        let (pid, daemon_pid) = (params.pid, std::process::id());
        match tokio::task::spawn_blocking(move || ports::kill_tree(pid, daemon_pid)).await {
            Ok(Ok(())) => ok(&()),
            Ok(Err(reason)) => Outcome::error(ErrorCode::InvalidState, reason),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("port stop: {e}")),
        }
    }

    async fn port_rows(&self) -> Result<Vec<PortRow>, Outcome> {
        let known = self.port_known()?;
        match tokio::task::spawn_blocking(move || ports::scan(known)).await {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(e)) => Err(Outcome::error(ErrorCode::Internal, format!("{e:#}"))),
            Err(e) => Err(Outcome::error(
                ErrorCode::Internal,
                format!("port scan: {e}"),
            )),
        }
    }

    /// Live sessions by root pid, workspace and worktree folders, workspace order. The live
    /// map is read and released before the store is locked: no nested locks.
    fn port_known(&self) -> Result<Known, Outcome> {
        let pids = self.hub.live_pids();
        let read = {
            let store = self.store();
            store.list_projects().and_then(|projects| {
                let tasks = store.list_tasks(None, None)?;
                let sessions = store.list_sessions(None)?;
                Ok((projects, tasks, sessions))
            })
        };
        let (projects, tasks, sessions) = read.map_err(|e| store_outcome::<()>(Err(e)))?;
        let projects: Vec<_> = projects.into_iter().filter(|p| !p.archived).collect();
        let worktree = |task_id: Option<_>| {
            tasks
                .iter()
                .find(|t| Some(t.id) == task_id && t.worktree_path.is_some())
        };
        let owners = pids
            .into_iter()
            .filter_map(|(session_id, pid)| {
                let session = sessions
                    .iter()
                    .find(|s| s.id == session_id && s.state != SessionState::Exited)?;
                let task = worktree(session.task_id);
                Some((
                    pid,
                    Owner {
                        session_id,
                        project_id: session.project_id,
                        worktree_task_id: task.map(|t| t.id),
                        branch: task
                            .and_then(|t| t.branch.as_deref())
                            .map(ports::short_branch),
                        session_title: session.title.clone(),
                        run_command: run::command_name(session).map(str::to_owned),
                    },
                ))
            })
            .collect();
        let mut folders: Vec<Folder> = projects
            .iter()
            .map(|p| Folder {
                path: PathBuf::from(&p.path),
                project_id: p.id,
                worktree_task_id: None,
                branch: None,
            })
            .collect();
        folders.extend(tasks.iter().filter_map(|t| {
            Some(Folder {
                path: PathBuf::from(t.worktree_path.as_ref()?),
                project_id: t.project_id,
                worktree_task_id: Some(t.id),
                branch: t.branch.as_deref().map(ports::short_branch),
            })
        }));
        let order: HashMap<_, _> = projects
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id, i))
            .collect();
        Ok(Known {
            owners,
            folders,
            order,
            daemon_pid: std::process::id(),
        })
    }
}
