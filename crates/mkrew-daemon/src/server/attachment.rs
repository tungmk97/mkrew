//! `attachment.save` (SPEC §20B.5).

use mkrew_protocol::{AttachmentSaveParams, AttachmentSaveResult, ErrorCode, Outcome};

use super::{Daemon, ok, store_outcome};
use crate::attachment;

impl Daemon {
    /// Stores a pasted image for a known session and returns its absolute path.
    pub(super) async fn attachment_save(&self, params: AttachmentSaveParams) -> Outcome {
        if let Err(message) = attachment::check(&params.ext, params.data.len()) {
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        if let Err(e) = self.hub.store().get_session(params.session_id) {
            return store_outcome::<()>(Err(e));
        }
        let dir = self.paths.attachments_dir();
        let saved = tokio::task::spawn_blocking(move || {
            attachment::save(&dir, params.session_id, &params.ext, &params.data)
        })
        .await;
        match saved {
            Ok(Ok(path)) => ok(&AttachmentSaveResult {
                path: path.to_string_lossy().into_owned(),
            }),
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, format!("save attachment: {e}")),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("save attachment failed: {e}")),
        }
    }
}
