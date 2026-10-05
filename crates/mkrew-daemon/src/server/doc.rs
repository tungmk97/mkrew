//! `doc.list` / `doc.read` (SPEC §7, §12): the Markdown files to review and their content.
//! Both keep the project watched so `doc.changed` follows external edits (HUMAN_CHECKS T6.4).

use std::path::PathBuf;
use std::sync::Arc;

use mkrew_core::review::ReviewGlobs;
use mkrew_protocol::{DocContent, DocListParams, DocReadParams, ErrorCode, Outcome, Project};

use super::{Daemon, now_ms, ok, store_outcome};
use crate::docs::{self, ReadError};

impl Daemon {
    pub(super) async fn doc_list(&self, params: DocListParams) -> Outcome {
        let project = match self.watched_project(params.project_id) {
            Ok(project) => project,
            Err(outcome) => return outcome,
        };
        let globs = match ReviewGlobs::for_project(project.review_globs.as_deref()) {
            Ok(globs) => globs,
            Err(e) => {
                let message = format!("invalid review_globs: {e}");
                return Outcome::error(ErrorCode::InvalidState, message);
            }
        };
        let since = now_ms() - docs::RECENT_MS;
        let (changed, comments) = {
            let store = self.store();
            let changed = store.project_session_events(project.id, "files_changed", since);
            (changed, store.list_comments(project.id, None))
        };
        let (changed, comments) = match (changed, comments) {
            (Ok(changed), Ok(comments)) => (changed, comments),
            (Err(e), _) | (_, Err(e)) => return store_outcome::<()>(Err(e)),
        };
        let root = PathBuf::from(&project.path);
        let listed = tokio::task::spawn_blocking(move || {
            let listed = docs::markdown_files(&root, &globs);
            docs::doc_files(listed, &changed, &comments, |p| root.join(p).is_file())
        })
        .await;
        match listed {
            Ok(files) => ok(&files),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("doc.list failed: {e}")),
        }
    }

    pub(super) async fn doc_read(&self, params: DocReadParams) -> Outcome {
        let Some((file_path, relative)) = docs::relative_path(&params.file_path) else {
            let message = format!(
                "file_path {:?} must be relative to the project root",
                params.file_path
            );
            return Outcome::error(ErrorCode::InvalidRequest, message);
        };
        let project = match self.watched_project(params.project_id) {
            Ok(project) => project,
            Err(outcome) => return outcome,
        };
        let path = PathBuf::from(&project.path).join(relative);
        let (hub, file) = (Arc::clone(&self.hub), file_path.clone());
        let read = tokio::task::spawn_blocking(move || {
            let content = docs::read(&path)?;
            if docs::is_markdown(&file) {
                hub.doc_seen(project.id, &file, Some(&content));
            }
            Ok::<_, ReadError>(content)
        });
        match read.await {
            Ok(Ok(content)) => ok(&DocContent {
                hash: docs::hash(content.as_bytes()),
                file_path,
                content,
            }),
            Ok(Err(e)) => read_error(project.id, &file_path, e),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("doc.read failed: {e}")),
        }
    }

    /// The project, now watched for doc changes.
    fn watched_project(&self, project_id: ulid::Ulid) -> Result<Project, Outcome> {
        let project = self
            .store()
            .get_project(project_id)
            .map_err(|e| store_outcome::<()>(Err(e)))?;
        self.hub
            .watch_docs(project.id, &PathBuf::from(&project.path));
        Ok(project)
    }
}

/// The error answer for a project file that could not be read.
pub(super) fn read_error(project_id: ulid::Ulid, file_path: &str, e: ReadError) -> Outcome {
    match e {
        ReadError::NotFound => {
            let message = format!("no file {file_path} in project {project_id}");
            Outcome::error(ErrorCode::NotFound, message)
        }
        ReadError::TooLarge(_) | ReadError::NotUtf8 => {
            Outcome::error(ErrorCode::InvalidState, format!("{file_path}: {e}"))
        }
        ReadError::Io(e) => Outcome::error(ErrorCode::Internal, format!("read {file_path}: {e}")),
    }
}
