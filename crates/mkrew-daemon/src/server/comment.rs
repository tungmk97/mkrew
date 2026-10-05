//! `comment.*` handlers (SPEC §7, §12): review threads on Markdown files. Every change goes
//! out as `comment.updated`; status changes always land on the thread root.

use std::path::PathBuf;

use mkrew_core::review;
use mkrew_protocol::{
    CommentCreateParams, CommentId, CommentListParams, CommentReplyParams, CommentSendParams,
    CommentStatus, DocComment, ErrorCode, Event, Outcome, Project,
};
use mkrew_store::{Store, StoreError};

use super::doc::read_error;
use super::{Daemon, ok, store_outcome};
use crate::docs;
use crate::session::Command;

impl Daemon {
    pub(super) fn comment_list(&self, params: CommentListParams) -> Outcome {
        let file = params.file_path.as_deref().map(normalize);
        store_outcome(
            self.store()
                .list_comments(params.project_id, file.as_deref()),
        )
    }

    pub(super) fn comment_create(&self, params: CommentCreateParams) -> Outcome {
        let file = normalize(&params.file_path);
        if file.is_empty() {
            return Outcome::error(ErrorCode::InvalidRequest, "file_path is empty");
        }
        if params.start_line == 0 || params.start_line > params.end_line {
            let message = format!(
                "invalid line range {}–{} (1-based, start ≤ end)",
                params.start_line, params.end_line
            );
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        if params.body.trim().is_empty() {
            return Outcome::error(ErrorCode::InvalidRequest, "comment body is empty");
        }
        let created = self.store().create_comment(
            params.project_id,
            &file,
            params.start_line,
            params.end_line,
            &params.quote,
            &params.body,
        );
        self.comments_updated(created.map(|c| vec![c]))
    }

    /// An agent reply (`mkrew_session_id` set, author = the session's profile) moves the
    /// thread to `addressed`; a user reply on an `addressed` thread moves it back to `open`.
    pub(super) fn comment_reply(&self, params: CommentReplyParams) -> Outcome {
        if params.body.trim().is_empty() {
            return Outcome::error(ErrorCode::InvalidRequest, "reply body is empty");
        }
        let changed = {
            let store = self.store();
            reply(&store, &params)
        };
        self.comments_updated(changed)
    }

    pub(super) fn comment_set_status(&self, id: CommentId, status: CommentStatus) -> Outcome {
        let changed = {
            let store = self.store();
            thread_root(&store, id).and_then(|root| store.set_comment_status(root.id, status))
        };
        self.comments_updated(changed.map(|c| vec![c]))
    }

    /// Types the review prompt for the threads into the session (queued while it works, like
    /// `session.send`), then marks them `sent` with the file content as the snapshot.
    pub(super) async fn comment_send_to_agent(&self, params: CommentSendParams) -> Outcome {
        let session_id = params.session_id;
        let sending = {
            let store = self.store();
            sending(&store, &params)
        };
        let Sending {
            project,
            file,
            comments,
        } = match sending {
            Ok(Ok(sending)) => sending,
            Ok(Err(message)) => return Outcome::error(ErrorCode::InvalidRequest, message),
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let Some(session) = self.hub.sender(session_id) else {
            return self.not_live(session_id);
        };
        let Some((_, relative)) = docs::relative_path(&file) else {
            let message = format!("comment file {file:?} is not inside the project");
            return Outcome::error(ErrorCode::InvalidState, message);
        };
        let path = PathBuf::from(&project.path).join(relative);
        let snapshot = match tokio::task::spawn_blocking(move || docs::read(&path)).await {
            Ok(Ok(content)) => content,
            Ok(Err(e)) => return read_error(project.id, &file, e),
            Err(e) => {
                let message = format!("comment.send_to_agent failed: {e}");
                return Outcome::error(ErrorCode::Internal, message);
            }
        };
        let mut text = review::prompt(&file, &comments);
        text.push('\r');
        if session.send(Command::Send(text)).await.is_err() {
            return self.not_live(session_id);
        }
        let sent = {
            let store = self.store();
            comments
                .iter()
                .filter(|c| c.parent_id.is_none())
                .map(|root| store.mark_comment_sent(root.id, session_id, &snapshot))
                .collect::<Result<Vec<_>, _>>()
        };
        match sent {
            Ok(sent) => {
                self.announce(sent.clone());
                ok(&sent)
            }
            // Queued before marking on purpose: a prompt without a record beats a record
            // without a prompt.
            Err(e) => {
                tracing::error!(%session_id, "prompt queued but marking comments sent failed: {e}");
                store_outcome::<()>(Err(e))
            }
        }
    }

    fn announce(&self, changed: Vec<DocComment>) {
        for comment in changed {
            let _ = self
                .hub
                .events
                .send(Event::CommentUpdated(Box::new(comment)));
        }
    }

    /// Announces every changed comment and replies with the first one.
    fn comments_updated(&self, changed: Result<Vec<DocComment>, StoreError>) -> Outcome {
        let first = changed.map(|changed| {
            let first = changed.first().cloned();
            self.announce(changed);
            first
        });
        match first {
            Ok(Some(comment)) => store_outcome(Ok(comment)),
            Ok(None) => Outcome::error(ErrorCode::Internal, "no comment changed"),
            Err(e) => store_outcome::<()>(Err(e)),
        }
    }
}

/// The reply, then the thread root if its status changed.
fn reply(store: &Store, params: &CommentReplyParams) -> Result<Vec<DocComment>, StoreError> {
    let root = thread_root(store, params.comment_id)?;
    let (author, status) = match params.mkrew_session_id {
        Some(session) => (
            store.get_session(session)?.agent_profile,
            CommentStatus::Addressed,
        ),
        None if root.status == CommentStatus::Addressed => {
            (DocComment::USER.to_owned(), CommentStatus::Open)
        }
        None => (DocComment::USER.to_owned(), root.status),
    };
    let reply = store.reply_comment(root.id, &author, &params.body)?;
    let mut changed = vec![reply];
    if status != root.status {
        changed.push(store.set_comment_status(root.id, status)?);
    }
    Ok(changed)
}

/// What `comment.send_to_agent` types: the threads' roots in request order, each followed by
/// its replies.
struct Sending {
    project: Project,
    file: String,
    comments: Vec<DocComment>,
}

/// The threads to send, or why the request is invalid (`Ok(Err)`): they must all belong to one
/// file of the session's project.
fn sending(
    store: &Store,
    params: &CommentSendParams,
) -> Result<Result<Sending, String>, StoreError> {
    let mut roots: Vec<DocComment> = Vec::new();
    for id in &params.comment_ids {
        let root = thread_root(store, *id)?;
        if !roots.iter().any(|r| r.id == root.id) {
            roots.push(root);
        }
    }
    let Some(first) = roots.first() else {
        return Ok(Err("comment_ids is empty".to_owned()));
    };
    let (project_id, file) = (first.project_id, first.file_path.clone());
    if roots
        .iter()
        .any(|r| r.project_id != project_id || r.file_path != file)
    {
        return Ok(Err("the comments must all be on one file".to_owned()));
    }
    let session = store.get_session(params.session_id)?;
    if session.project_id != project_id {
        let message = format!(
            "session {} belongs to another project than the comments",
            session.id
        );
        return Ok(Err(message));
    }
    let project = store.get_project(project_id)?;
    let all = store.list_comments(project_id, Some(&file))?;
    let mut comments = Vec::new();
    for root in roots {
        let id = root.id;
        comments.push(root);
        comments.extend(all.iter().filter(|c| c.parent_id == Some(id)).cloned());
    }
    Ok(Ok(Sending {
        project,
        file,
        comments,
    }))
}

/// The comment itself, or the thread it replies to.
fn thread_root(store: &Store, id: CommentId) -> Result<DocComment, StoreError> {
    let comment = store.get_comment(id)?;
    match comment.parent_id {
        Some(parent) => store.get_comment(parent),
        None => Ok(comment),
    }
}

/// Project-relative paths always use `/` (SPEC §5).
fn normalize(path: &str) -> String {
    path.trim().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_use_forward_slashes() {
        assert_eq!(normalize(r" docs\plan.md "), "docs/plan.md");
        assert_eq!(normalize("README.md"), "README.md");
    }
}
