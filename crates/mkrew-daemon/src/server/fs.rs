//! `fs.*` handlers (SPEC §7, §12A). Each keeps the project watched: the Files view refreshes
//! on `git.changed` and `fs.find`'s cached file list is invalidated by the watcher.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use mkrew_protocol::{ErrorCode, FsFindParams, FsListParams, FsReadParams, Outcome};

use super::doc::read_error;
use super::{Daemon, ok};
use crate::{docs, files};

impl Daemon {
    pub(super) async fn fs_list(&self, params: FsListParams) -> Outcome {
        let dir = if params.dir.trim().is_empty() {
            String::new()
        } else {
            match docs::relative_path(&params.dir) {
                Some((dir, _)) if !in_git_dir(&dir) => dir.trim_end_matches('/').to_owned(),
                _ => return outside("dir", &params.dir),
            }
        };
        let project = match self.watched_project(params.project_id) {
            Ok(project) => project,
            Err(outcome) => return outcome,
        };
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let (project_id, folder) = (project.id, dir.clone());
        let listed = tokio::task::spawn_blocking(move || {
            let root = PathBuf::from(project.path);
            // Not a repo (or git failing): no markers, the tree still lists.
            let status = cache
                .repo(&*git, &root)
                .ok()
                .and_then(|repo| git.status(&root, &repo).ok());
            let markers = status.map_or_else(HashMap::new, |s| files::markers(&s));
            files::list(&root, &folder, params.show_ignored, &markers)
        })
        .await;
        match listed {
            Ok(Ok(entries)) => ok(&entries),
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                let message = format!("no folder {dir:?} in project {project_id}");
                Outcome::error(ErrorCode::NotFound, message)
            }
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, format!("fs.list {dir:?}: {e}")),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("fs.list failed: {e}")),
        }
    }

    pub(super) async fn fs_read(&self, params: FsReadParams) -> Outcome {
        let Some((path, relative)) = docs::relative_path(&params.path) else {
            return outside("path", &params.path);
        };
        if in_git_dir(&path) {
            return outside("path", &params.path);
        }
        let project = match self.watched_project(params.project_id) {
            Ok(project) => project,
            Err(outcome) => return outcome,
        };
        let file = PathBuf::from(&project.path).join(relative);
        let shown = path.clone();
        let read = tokio::task::spawn_blocking(move || files::read(&file, path)).await;
        match read {
            Ok(Ok(content)) => ok(&content),
            Ok(Err(e)) => read_error(project.id, &shown, e),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("fs.read failed: {e}")),
        }
    }

    pub(super) async fn fs_find(&self, params: FsFindParams) -> Outcome {
        let project = match self.watched_project(params.project_id) {
            Ok(project) => project,
            Err(outcome) => return outcome,
        };
        let hub = Arc::clone(&self.hub);
        let found = tokio::task::spawn_blocking(move || {
            let all = hub
                .file_index
                .files(project.id, &PathBuf::from(&project.path));
            files::find(&all, &params.query, params.limit as usize)
        })
        .await;
        match found {
            Ok(matches) => ok(&matches),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("fs.find failed: {e}")),
        }
    }
}

fn outside(field: &str, value: &str) -> Outcome {
    let message =
        format!("{field} {value:?} must be relative to the project root and outside .git");
    Outcome::error(ErrorCode::InvalidRequest, message)
}

/// The Files view never shows git's own files (SPEC §12A tree hides `.git`).
fn in_git_dir(path: &str) -> bool {
    path.split('/')
        .any(|part| part.eq_ignore_ascii_case(".git"))
}
