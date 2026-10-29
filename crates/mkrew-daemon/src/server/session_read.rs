//! `session.log_tail` and `session.transcript` (spec 2026-10-29 §5): the daemon reads the
//! files so a window on another computer can show them.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use mkrew_protocol::{
    ErrorCode, Outcome, READ_WINDOW, SessionLogTail, SessionLogTailParams, SessionTranscript,
    SessionTranscriptParams,
};

use super::{Daemon, ok, store_outcome};
use crate::{session_log, usage};

impl Daemon {
    /// `session.log_tail`: the newest log bytes, up to [`READ_WINDOW`]. Answers for exited and
    /// hidden sessions like `session.history`.
    pub(super) async fn session_log_tail(&self, params: SessionLogTailParams) -> Outcome {
        let session = match self.store().get_session(params.session_id) {
            Ok(session) => session,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let files = session_log::files(&session_log::path(&self.paths.sessions_dir(), session.id));
        let limit = params.max_bytes.min(READ_WINDOW);
        match tokio::task::spawn_blocking(move || mkrew_core::log_tail::read_tail(&files, limit))
            .await
        {
            Ok(Ok(tail)) => ok(&SessionLogTail {
                data: tail.bytes,
                cut: tail.cut,
            }),
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, format!("read the log: {e}")),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("session.log_tail failed: {e}")),
        }
    }

    /// `session.transcript`: a window of the agent's transcript file. Not found (no profile,
    /// no `[usage]`, no native id, no file) is an answer with `found: false`, not an error.
    pub(super) async fn session_transcript(&self, params: SessionTranscriptParams) -> Outcome {
        let session = match self.store().get_session(params.session_id) {
            Ok(session) => session,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let format = self
            .load_profile(&session.agent_profile)
            .ok()
            .and_then(|profile| profile.usage)
            .map(|usage| usage.format);
        let native = session.native_session_id;
        let read = tokio::task::spawn_blocking(move || {
            let Some(format) = format else {
                return Ok(not_found(String::new()));
            };
            let name = format.name().to_owned();
            let Some(path) = native.and_then(|native| usage::transcript_path(&format, &native))
            else {
                return Ok(not_found(name));
            };
            let Ok(mut file) = File::open(&path) else {
                return Ok(not_found(name));
            };
            let len = file.metadata()?.len();
            let (start, end) = transcript_window(len, params.offset);
            // `end - start` is at most READ_WINDOW (4 MiB), so it fits in usize.
            let mut data = vec![0; usize::try_from(end - start).unwrap_or(0)];
            file.seek(SeekFrom::Start(start))?;
            file.read_exact(&mut data)?;
            Ok::<_, std::io::Error>(SessionTranscript {
                found: true,
                format: name,
                start,
                len,
                data,
            })
        })
        .await;
        match read {
            Ok(Ok(transcript)) => ok(&transcript),
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, format!("read the transcript: {e}")),
            Err(e) => Outcome::error(
                ErrorCode::Internal,
                format!("session.transcript failed: {e}"),
            ),
        }
    }
}

fn not_found(format: String) -> SessionTranscript {
    SessionTranscript {
        found: false,
        format,
        start: 0,
        len: 0,
        data: Vec::new(),
    }
}

/// The part of a transcript `session.transcript` returns: from `offset` when the file still
/// reaches it, else from the start; the last [`READ_WINDOW`] bytes when starting from 0.
fn transcript_window(len: u64, offset: u64) -> (u64, u64) {
    let start = if offset > len { 0 } else { offset };
    let start = if start == 0 {
        len.saturating_sub(READ_WINDOW)
    } else {
        start
    };
    let end = (start + READ_WINDOW).min(len);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_window_reads_the_tail_first_then_from_the_offset_and_restarts_after_a_shrink() {
        let w = READ_WINDOW;
        assert_eq!(transcript_window(10, 0), (0, 10));
        assert_eq!(
            transcript_window(w + 5, 0),
            (5, w + 5),
            "first look: the newest window"
        );
        assert_eq!(
            transcript_window(w + 5, w + 5),
            (w + 5, w + 5),
            "nothing new"
        );
        assert_eq!(
            transcript_window(3 * w, w),
            (w, 2 * w),
            "one window per reply"
        );
        assert_eq!(
            transcript_window(4, 9),
            (0, 4),
            "rewritten shorter: from the start"
        );
    }
}
