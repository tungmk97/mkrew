//! `comment.*` handlers (SPEC §7, §12): review threads on Markdown files. Every change goes
//! out as `comment.updated`; status changes always land on the thread root.

use mkrew_protocol::{
    CommentCreateParams, CommentId, CommentListParams, CommentReplyParams, CommentStatus,
    DocComment, ErrorCode, Event, Outcome,
};
use mkrew_store::{Store, StoreError};

use super::{Daemon, store_outcome};

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

    /// Announces every changed comment and replies with the first one.
    fn comments_updated(&self, changed: Result<Vec<DocComment>, StoreError>) -> Outcome {
        let first = changed.map(|changed| {
            let first = changed.first().cloned();
            for comment in changed {
                let _ = self
                    .hub
                    .events
                    .send(Event::CommentUpdated(Box::new(comment)));
            }
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
