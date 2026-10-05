//! Attached clients (SPEC §7): each attachment is a bounded byte queue filled by the session
//! task and drained by a per-client flusher task into binary output frames.
//!
//! The session task never waits on a client: when the queue overflows it drops the queued
//! bytes, and the flusher asks the session for a fresh screen snapshot instead.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use mkrew_protocol::{Event, Frame, Message, ScreenSnapshot, SessionSnapshotEvent, encode};
use tokio::sync::{Notify, mpsc, oneshot};
use ulid::Ulid;

use crate::server::Outgoing;
use crate::session::Command;

/// Output queued per attached client before it is dropped for a snapshot.
pub const CLIENT_QUEUE_BYTES: usize = 4 * 1024 * 1024;
/// Output frames go out at most this often (SPEC §7: batched ≤ every 16 ms).
pub const BATCH_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Default)]
pub struct Attachment {
    pending: Mutex<Pending>,
    wake: Notify,
    /// Set when the client detaches or disconnects; the session task then forgets it.
    closed: AtomicBool,
}

#[derive(Debug, Default)]
struct Pending {
    bytes: Vec<u8>,
    snapshot: Option<ScreenSnapshot>,
    overflow: Overflow,
}

#[derive(Debug, Default, PartialEq)]
enum Overflow {
    #[default]
    No,
    /// Bytes were dropped; the flusher has not asked for a snapshot yet.
    Dropped,
    /// Snapshot requested; output is dropped until it arrives.
    Requested,
}

/// What the flusher does next.
#[derive(Debug, PartialEq)]
enum Batch {
    Resnapshot,
    Ready {
        snapshot: Option<ScreenSnapshot>,
        bytes: Vec<u8>,
    },
}

impl Attachment {
    /// Session task: queues output, or drops everything queued once the limit is hit.
    pub fn push(&self, bytes: &[u8]) {
        {
            let mut pending = self.lock();
            if pending.overflow != Overflow::No {
                return;
            }
            if pending.bytes.len() + bytes.len() > CLIENT_QUEUE_BYTES {
                pending.bytes = Vec::new();
                pending.overflow = Overflow::Dropped;
            } else {
                pending.bytes.extend_from_slice(bytes);
            }
        }
        self.wake.notify_one();
    }

    /// Session task: the screen as of now replaces everything queued before it.
    pub fn resnapshot(&self, snapshot: ScreenSnapshot) {
        {
            let mut pending = self.lock();
            pending.bytes.clear();
            pending.overflow = Overflow::No;
            pending.snapshot = Some(snapshot);
        }
        self.wake.notify_one();
    }

    /// Output was dropped and no snapshot has replaced it yet.
    pub fn is_overflowed(&self) -> bool {
        self.lock().overflow != Overflow::No
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    fn take(&self) -> Batch {
        let mut pending = self.lock();
        if pending.overflow == Overflow::Dropped {
            pending.overflow = Overflow::Requested;
            return Batch::Resnapshot;
        }
        Batch::Ready {
            snapshot: pending.snapshot.take(),
            bytes: std::mem::take(&mut pending.bytes),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Pending> {
        // Plain buffers: still consistent if a holder panicked.
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Drains `attachment` into `out` until the client or the session goes away. At most one
/// output frame is in flight (queued or being written), so a slow client only grows the
/// bounded attachment queue.
pub async fn flush(
    session_id: Ulid,
    attachment: Arc<Attachment>,
    session: mpsc::Sender<Command>,
    out: mpsc::Sender<Outgoing>,
) {
    loop {
        attachment.wake.notified().await;
        match attachment.take() {
            Batch::Resnapshot => {
                tracing::debug!(id = %session_id, "client output queue overflowed; resnapshot");
                let command = Command::Resnapshot(Arc::clone(&attachment));
                // A session that exits answers every overflowed attachment with its last screen.
                let _ = session.send(command).await;
                continue;
            }
            Batch::Ready { snapshot, bytes } => {
                if let Some(snapshot) = snapshot {
                    let event = Event::SessionSnapshot(SessionSnapshotEvent {
                        session_id,
                        snapshot,
                    });
                    if !send(&out, Frame::Message(Message::Event(event)), None).await {
                        return;
                    }
                }
                if !bytes.is_empty() {
                    let (written, done) = oneshot::channel();
                    let frame = Frame::Output {
                        session_id,
                        data: bytes,
                    };
                    if !send(&out, frame, Some(written)).await {
                        return;
                    }
                    // Resolves (with an error) when the writer drops the frame after writing.
                    let _ = done.await;
                }
            }
        }
        tokio::time::sleep(BATCH_INTERVAL).await;
    }
}

/// `false` once the connection is gone.
async fn send(
    out: &mpsc::Sender<Outgoing>,
    frame: Frame,
    written: Option<oneshot::Sender<()>>,
) -> bool {
    match encode(&frame) {
        Ok(bytes) => out.send(Outgoing { bytes, written }).await.is_ok(),
        Err(e) => {
            tracing::error!("encode attach frame: {e}");
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::Emulator;
    use mkrew_protocol::FrameDecoder;

    fn ready(bytes: &[u8]) -> Batch {
        Batch::Ready {
            snapshot: None,
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn overflow_drops_output_until_the_snapshot() {
        let att = Attachment::default();
        att.push(b"ab");
        att.push(b"c");
        assert_eq!(att.take(), ready(b"abc"));
        assert_eq!(att.take(), ready(b""));

        let chunk = vec![b'x'; 64 * 1024];
        for _ in 0..(CLIENT_QUEUE_BYTES / chunk.len()) {
            att.push(&chunk);
        }
        assert_eq!(att.lock().bytes.len(), CLIENT_QUEUE_BYTES);
        att.push(b"y");
        assert_eq!(att.lock().bytes.capacity(), 0, "dropped bytes are freed");
        assert_eq!(att.take(), Batch::Resnapshot);
        // Asked once; output stays dropped until the snapshot arrives.
        att.push(b"lost");
        assert_eq!(att.take(), ready(b""));

        let snapshot = Emulator::new(4, 2).snapshot();
        att.resnapshot(snapshot.clone());
        att.push(b"after");
        assert_eq!(
            att.take(),
            Batch::Ready {
                snapshot: Some(snapshot),
                bytes: b"after".to_vec()
            }
        );
    }

    fn decode(item: &Outgoing) -> Frame {
        let mut decoder = FrameDecoder::new();
        decoder.push(&item.bytes);
        decoder.next_frame().unwrap().unwrap()
    }

    #[tokio::test]
    async fn slow_client_gets_a_snapshot_instead_of_unbounded_output() {
        let id = Ulid::new();
        let att = Arc::new(Attachment::default());
        let (session_tx, mut session_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(256);
        let flusher = tokio::spawn(flush(id, Arc::clone(&att), session_tx, out_tx));

        att.push(b"first");
        // The client never finishes writing this frame…
        let stuck = out_rx.recv().await.unwrap();
        let Frame::Output { session_id, data } = decode(&stuck) else {
            panic!("expected output");
        };
        assert_eq!((session_id, data.as_slice()), (id, &b"first"[..]));
        // …so nothing else is sent while the session keeps producing output.
        let chunk = vec![b'x'; 64 * 1024];
        for _ in 0..80 {
            att.push(&chunk);
        }
        tokio::time::sleep(BATCH_INTERVAL * 3).await;
        assert!(out_rx.try_recv().is_err());
        assert!(session_rx.try_recv().is_err());

        // Once the write completes, the flusher asks the session for a snapshot.
        drop(stuck);
        let Some(Command::Resnapshot(target)) = session_rx.recv().await else {
            panic!("expected a resnapshot request");
        };
        assert!(Arc::ptr_eq(&target, &att));
        let snapshot = Emulator::new(4, 2).snapshot();
        target.resnapshot(snapshot.clone());
        att.push(b"next");

        let Frame::Message(Message::Event(Event::SessionSnapshot(event))) =
            decode(&out_rx.recv().await.unwrap())
        else {
            panic!("expected a snapshot event");
        };
        assert_eq!((event.session_id, event.snapshot), (id, snapshot));
        let Frame::Output { data, .. } = decode(&out_rx.recv().await.unwrap()) else {
            panic!("expected output");
        };
        assert_eq!(data, b"next");
        flusher.abort();
    }
}
