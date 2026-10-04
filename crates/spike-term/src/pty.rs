//! PTY session: a child process on a pseudo-terminal with its output in a shared inbox
//! (SPEC §8).
//!
//! The reader thread appends every read to one byte buffer (so bursts coalesce, whatever the
//! read sizes) and signals a wake channel; a UI awaits that signal instead of polling (SPEC §10).

use std::io::{Read, Write};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::emulator::Emulator;

const READ_BUFFER_BYTES: usize = 64 * 1024;
/// The reader thread blocks (back-pressuring the child) while this much output is unconsumed.
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
/// Max output fed to the emulator per `drain` call (one UI frame).
const DRAIN_BUDGET_BYTES: usize = 1024 * 1024;

/// Output not yet consumed, shared by the reader/exit threads and the session.
#[derive(Default)]
struct Inbox {
    bytes: Vec<u8>,
    exited: Option<u32>,
    /// The session was dropped; the reader stops.
    closed: bool,
}

#[derive(Default)]
struct Shared {
    inbox: Mutex<Inbox>,
    /// Signalled on every inbox change (new output, exit, output consumed).
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Inbox> {
        // The inbox is plain data, still valid if a holder panicked.
        self.inbox.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct PtySession {
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    shared: Arc<Shared>,
    /// Capacity 1: a pending wake already covers any output that arrives after it.
    wake_tx: async_channel::Sender<()>,
    wake_rx: async_channel::Receiver<()>,
}

impl PtySession {
    /// Spawns `command` on a new PTY of `cols` x `rows`.
    pub fn spawn(mut command: CommandBuilder, cols: u16, rows: u16) -> Result<Self> {
        command.env("TERM", "xterm-256color");
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open pty")?;
        let mut child = pair.slave.spawn_command(command).context("spawn command")?;
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;

        let shared = Arc::new(Shared::default());
        let (wake_tx, wake_rx) = async_channel::bounded(1);
        // Blocking reads live on their own thread (SPEC §8).
        let (reader_shared, reader_wake) = (shared.clone(), wake_tx.clone());
        thread::spawn(move || {
            let mut buf = vec![0u8; READ_BUFFER_BYTES];
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let mut inbox = reader_shared.lock();
                while inbox.bytes.len() >= MAX_PENDING_BYTES && !inbox.closed {
                    inbox = reader_shared
                        .changed
                        .wait(inbox)
                        .unwrap_or_else(PoisonError::into_inner);
                }
                if inbox.closed {
                    break;
                }
                inbox.bytes.extend_from_slice(&buf[..n]);
                drop(inbox);
                reader_shared.changed.notify_all();
                // Full = a wake is already pending; closed = nobody listens. Both are fine.
                let _ = reader_wake.try_send(());
            }
        });
        // ConPTY never signals EOF on child exit, so exit is detected by waiting on the child.
        let (exit_shared, exit_wake) = (shared.clone(), wake_tx.clone());
        thread::spawn(move || {
            let code = child.wait().map(|status| status.exit_code()).unwrap_or(1);
            exit_shared.lock().exited = Some(code);
            exit_shared.changed.notify_all();
            let _ = exit_wake.try_send(());
        });

        Ok(Self {
            master: Some(pair.master),
            writer,
            killer,
            shared,
            wake_tx,
            wake_rx,
        })
    }

    /// Receives a message whenever output or the exit is waiting for [`Self::drain`].
    /// Wakes coalesce: one message may stand for many reads.
    pub fn wake(&self) -> async_channel::Receiver<()> {
        self.wake_rx.clone()
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes).context("write to pty")?;
        self.writer.flush().context("flush pty")
    }

    /// Tells the child its terminal is now `cols` x `rows` (SIGWINCH / ConPTY resize).
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let master = self.master.as_ref().context("pty already closed")?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("resize pty")
    }

    /// Feeds output into `emulator` (answering its replies) until no output arrives for
    /// `idle`, the channel closes, or `deadline` passes. Returns the exit code if the child exited.
    pub fn pump(
        &mut self,
        emulator: &mut Emulator,
        idle: Duration,
        deadline: Duration,
    ) -> Result<Option<u32>> {
        let end = Instant::now() + deadline;
        while Instant::now() < end {
            let inbox = self.shared.lock();
            let (mut inbox, _) = self
                .shared
                .changed
                .wait_timeout_while(inbox, idle, |inbox| inbox.bytes.is_empty())
                .unwrap_or_else(PoisonError::into_inner);
            if inbox.bytes.is_empty() {
                break;
            }
            let bytes = std::mem::take(&mut inbox.bytes);
            drop(inbox);
            self.shared.changed.notify_all();
            let reply = emulator.advance(&bytes);
            if !reply.is_empty() {
                self.write(&reply)?;
            }
        }
        Ok(self.shared.lock().exited)
    }

    /// Non-blocking: feeds all output queued so far into `emulator` (answering its replies).
    /// Lets a UI coalesce everything that arrived between two frames (SPEC §10).
    /// Takes at most `DRAIN_BUDGET_BYTES` so a flood cannot starve the UI thread; a remainder
    /// re-arms the wake. The exit is reported once all output queued before it is drained.
    pub fn drain(&mut self, emulator: &mut Emulator) -> Result<Drained> {
        let mut inbox = self.shared.lock();
        let bytes = if inbox.bytes.len() > DRAIN_BUDGET_BYTES {
            let rest = inbox.bytes.split_off(DRAIN_BUDGET_BYTES);
            std::mem::replace(&mut inbox.bytes, rest)
        } else {
            std::mem::take(&mut inbox.bytes)
        };
        let more = !inbox.bytes.is_empty();
        let exited = if more { None } else { inbox.exited };
        drop(inbox);
        if bytes.is_empty() {
            return Ok(Drained {
                exited,
                ..Drained::default()
            });
        }
        self.shared.changed.notify_all();
        if more {
            let _ = self.wake_tx.try_send(());
        }
        let reply = emulator.advance(&bytes);
        if !reply.is_empty() {
            self.write(&reply)?;
        }
        Ok(Drained {
            output: true,
            bytes: bytes.len(),
            exited,
        })
    }
}

/// Result of [`PtySession::drain`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Some output was fed to the emulator.
    pub output: bool,
    /// Output bytes fed to the emulator.
    pub bytes: usize,
    pub exited: Option<u32>,
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.killer.kill();
        // Release a reader thread blocked on a full inbox.
        self.shared.lock().closed = true;
        self.shared.changed.notify_all();
        // Closing the pseudo console can block on Windows; keep it off the caller's thread.
        if let Some(master) = self.master.take() {
            thread::spawn(move || drop(master));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_command(text: &str) -> CommandBuilder {
        let mut cmd = if cfg!(windows) {
            let mut cmd = CommandBuilder::new("cmd.exe");
            cmd.args(["/C", "echo", text]);
            cmd
        } else {
            let mut cmd = CommandBuilder::new("sh");
            cmd.args(["-c", &format!("echo {text}")]);
            cmd
        };
        cmd.cwd(std::env::temp_dir());
        cmd
    }

    #[test]
    fn child_output_reaches_the_grid() {
        let mut session = PtySession::spawn(echo_command("mkrew-pty-ok"), 40, 5).unwrap();
        let mut emu = Emulator::new(40, 5);
        let mut text = String::new();
        let end = Instant::now() + Duration::from_secs(10);
        while !text.contains("mkrew-pty-ok") && Instant::now() < end {
            let exited = session
                .pump(&mut emu, Duration::from_millis(200), Duration::from_secs(1))
                .unwrap();
            text = emu.grid().to_text();
            if exited.is_some() {
                // Drain output that arrives after the exit event, then stop.
                session
                    .pump(&mut emu, Duration::from_millis(200), Duration::from_secs(1))
                    .unwrap();
                text = emu.grid().to_text();
                break;
            }
        }
        assert!(text.contains("mkrew-pty-ok"), "grid was:\n{text}");
    }

    #[test]
    fn resize_reaches_the_pty() {
        // An interactive shell stays alive (waiting on stdin) until the session is dropped.
        let mut cmd = CommandBuilder::new(if cfg!(windows) { "cmd.exe" } else { "sh" });
        cmd.cwd(std::env::temp_dir());
        let session = PtySession::spawn(cmd, 40, 5).unwrap();
        session.resize(100, 30).unwrap();
        session.resize(20, 3).unwrap();
    }

    #[test]
    fn drain_collects_output_and_exit_without_blocking() {
        let mut session = PtySession::spawn(echo_command("mkrew-drain-ok"), 40, 5).unwrap();
        let mut emu = Emulator::new(40, 5);
        let mut saw_output = false;
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            let drained = session.drain(&mut emu).unwrap();
            saw_output |= drained.output;
            assert_eq!(drained.output, drained.bytes > 0);
            if drained.exited.is_some() && emu.grid().to_text().contains("mkrew-drain-ok") {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_output);
        let text = emu.grid().to_text();
        assert!(text.contains("mkrew-drain-ok"), "grid was:\n{text}");
    }

    #[test]
    fn wake_signals_output_and_exit_without_polling() {
        // On a helper thread so a lost wake fails the test instead of hanging it.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let mut session = PtySession::spawn(echo_command("mkrew-wake-ok"), 40, 5).unwrap();
            let wake = session.wake();
            let mut emu = Emulator::new(40, 5);
            // Each drain is preceded by a blocking wait on the wake; no sleeps.
            while session.drain(&mut emu).unwrap().exited.is_none() {
                wake.recv_blocking().unwrap();
            }
            // ConPTY can deliver output after the exit.
            session
                .pump(&mut emu, Duration::from_millis(200), Duration::from_secs(1))
                .unwrap();
            let _ = done_tx.send(emu.grid().to_text());
        });
        let text = done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(text.contains("mkrew-wake-ok"), "grid was:\n{text}");
    }
}
