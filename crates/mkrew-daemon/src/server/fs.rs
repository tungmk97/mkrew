//! `fs.*` handlers (SPEC §7, §12A). Each keeps its target (project or task worktree, §20E.1)
//! watched: the Files view refreshes on `git.changed` and `fs.find`'s cached file list is
//! invalidated by the watcher.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mkrew_protocol::{ErrorCode, FsFindParams, FsListParams, FsReadParams, FsWriteParams, Outcome};
use ulid::Ulid;

use super::doc::read_error;
use super::target::Target;
use super::{Daemon, ok};
use crate::docs::ReadError;
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
        let target = match self
            .watched_target(params.project_id, params.worktree_task_id, None)
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let (project_id, folder) = (target.project.id, dir.clone());
        let listed = tokio::task::spawn_blocking(move || {
            let root = target.dir;
            // Not a repo (or git failing): no markers, the tree still lists.
            let status_of = |dir: &Path| {
                cache
                    .repo(&*git, dir)
                    .ok()
                    .and_then(|repo| git.status(dir, &repo).ok())
            };
            let mut markers = status_of(&root).map_or_else(HashMap::new, |s| files::markers(&s));
            // Child repositories (ADR 0029): only the one the listed folder is in, or all of
            // them when the root is listed (its folder dots). One `git status` each.
            let top = folder.split('/').next().unwrap_or_default();
            for repo in files::child_repos(&root)
                .into_iter()
                .filter(|repo| top.is_empty() || repo == top)
            {
                if let Some(status) = status_of(&root.join(&repo)) {
                    let nested = files::markers(&status).into_iter();
                    markers.extend(nested.map(|(path, marker)| (format!("{repo}/{path}"), marker)));
                }
            }
            files::list(&*git, &root, &folder, params.show_ignored, &markers)
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

    /// The checks `fs.read` and `fs.write` share: the project-relative path (outside `.git`),
    /// the watched target, and the file's place inside it.
    async fn file_target(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        path: &str,
    ) -> Result<(String, PathBuf, Target), Outcome> {
        let Some((path, relative)) = docs::relative_path(path) else {
            return Err(outside("path", path));
        };
        if in_git_dir(&path) {
            return Err(outside("path", &path));
        }
        let target = self
            .watched_target(project_id, worktree_task_id, None)
            .await?;
        let file = target.dir.join(relative);
        Ok((path, file, target))
    }

    pub(super) async fn fs_read(&self, params: FsReadParams) -> Outcome {
        let (path, file, target) = match self
            .file_target(params.project_id, params.worktree_task_id, &params.path)
            .await
        {
            Ok(found) => found,
            Err(outcome) => return outcome,
        };
        let shown = path.clone();
        let (git, root, want_text) = (Arc::clone(&self.git), target.dir.clone(), params.index_text);
        let read = tokio::task::spawn_blocking(move || {
            files::read(&file, path).map(|mut content| {
                if want_text && content.content.is_some() {
                    content.index_text = files::index_text(&*git, &root, &content.path);
                }
                content
            })
        })
        .await;
        match read {
            Ok(Ok(content)) => ok(&content),
            Ok(Err(e)) => read_error(target.project.id, &shown, e),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("fs.read failed: {e}")),
        }
    }

    /// `fs.write` (ADR 0028): the same path checks as `fs.read`; the file must exist.
    pub(super) async fn fs_write(&self, params: FsWriteParams) -> Outcome {
        let (path, file, target) = match self
            .file_target(params.project_id, params.worktree_task_id, &params.path)
            .await
        {
            Ok(found) => found,
            Err(outcome) => return outcome,
        };
        let written = tokio::task::spawn_blocking(move || {
            files::write(&file, &params.content, &params.expected_hash)
        })
        .await;
        match written {
            Ok(Ok(written)) => ok(&written),
            Ok(Err(ReadError::TooLarge(len))) => Outcome::error(
                ErrorCode::InvalidRequest,
                format!(
                    "{path}: {len} bytes; at most {} can be saved",
                    docs::MAX_BYTES
                ),
            ),
            Ok(Err(e)) => read_error(target.project.id, &path, e),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("fs.write failed: {e}")),
        }
    }

    pub(super) async fn fs_find(&self, params: FsFindParams) -> Outcome {
        let target = match self
            .watched_target(params.project_id, params.worktree_task_id, None)
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let hub = Arc::clone(&self.hub);
        let found = tokio::task::spawn_blocking(move || {
            let all = hub.file_index.files(target.key(), &target.dir);
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
