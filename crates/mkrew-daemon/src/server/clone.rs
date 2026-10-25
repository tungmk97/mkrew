//! `project.clone` (ADR 0076): `git clone` a remote into a new folder, then add it as a
//! workspace. The reply comes at once; the end is announced as `project.cloned`.

use std::path::Path;
use std::sync::Arc;

use mkrew_protocol::{ErrorCode, Event, Outcome, ProjectCloneParams, ProjectClonedEvent};

use super::{Daemon, ok};

impl Daemon {
    pub(super) fn project_clone(&self, params: ProjectCloneParams) -> Outcome {
        let url = params.url.trim().to_owned();
        let Some(name) = mkrew_core::git::repo_dir_name(&url) else {
            let message = format!("cannot tell the folder name from `{url}`");
            return Outcome::error(ErrorCode::InvalidRequest, message);
        };
        let parent = Path::new(&params.parent).to_path_buf();
        if !parent.is_absolute() || !parent.is_dir() {
            let message = format!("{} is not a folder", parent.display());
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let dest = parent.join(&name);
        if dest.exists() && !is_empty_dir(&dest) {
            let message = format!("{} already exists and is not empty", dest.display());
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        let hub = Arc::clone(&self.hub);
        let clone_id = params.clone_id;
        tokio::spawn(async move {
            let target = dest;
            let cloned =
                tokio::task::spawn_blocking(move || crate::git::clone(&parent, &url, &name))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));
            // A failed clone leaves nothing behind: git removes what it wrote.
            let added = cloned.and_then(|()| {
                let added = hub.store().add_project(&target, None);
                added.map_err(|e| format!("cloned, but cannot add the workspace: {e}"))
            });
            let event = match added {
                Ok(project) => {
                    tracing::info!(id = %project.id, path = %project.path, "project cloned");
                    let _ = hub.events.send(Event::ProjectUpdated(project.clone()));
                    ProjectClonedEvent {
                        clone_id,
                        project: Some(project),
                        error: None,
                    }
                }
                Err(error) => {
                    tracing::info!(%clone_id, "project.clone failed: {error}");
                    ProjectClonedEvent {
                        clone_id,
                        project: None,
                        error: Some(error),
                    }
                }
            };
            let _ = hub.events.send(Event::ProjectCloned(event));
        });
        ok(&())
    }
}

fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}
