//! PTY session: a child process on a pseudo-terminal with its output on a channel (SPEC §8).

use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::emulator::Emulator;

/// Max chunks buffered between the reader thread and the consumer.
const CHANNEL_CAPACITY: usize = 64;
const READ_BUFFER_BYTES: usize = 64 * 1024;
/// Max output fed to the emulator per `drain` call (one UI frame).
const DRAIN_BUDGET_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum PtyEvent {
    Output(Vec<u8>),
    Exited(u32),
}

pub struct PtySession {
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    events: Receiver<PtyEvent>,
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

        let (tx, events) = sync_channel(CHANNEL_CAPACITY);
        let output_tx: SyncSender<PtyEvent> = tx.clone();
        // Blocking reads live on their own thread (SPEC §8).
        thread::spawn(move || {
            let mut buf = vec![0u8; READ_BUFFER_BYTES];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if output_tx.send(PtyEvent::Output(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        // ConPTY never signals EOF on child exit, so exit is detected by waiting on the child.
        thread::spawn(move || {
            let code = child.wait().map(|status| status.exit_code()).unwrap_or(1);
            let _ = tx.send(PtyEvent::Exited(code));
        });

        Ok(Self {
            master: Some(pair.master),
            writer,
            killer,
            events,
        })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes).context("write to pty")?;
        self.writer.flush().context("flush pty")
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
        let mut exit = None;
        while Instant::now() < end {
            match self.events.recv_timeout(idle) {
                Ok(PtyEvent::Output(bytes)) => {
                    let reply = emulator.advance(&bytes);
                    if !reply.is_empty() {
                        self.write(&reply)?;
                    }
                }
                Ok(PtyEvent::Exited(code)) => exit = Some(code),
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(exit)
    }

    /// Non-blocking: feeds all output queued so far into `emulator` (answering its replies).
    /// Lets a UI coalesce everything that arrived between two frames (SPEC §10).
    /// Stops after about `DRAIN_BUDGET_BYTES` so a flood cannot starve the UI thread.
    pub fn drain(&mut self, emulator: &mut Emulator) -> Result<Drained> {
        let mut drained = Drained::default();
        let mut bytes_fed = 0;
        while bytes_fed < DRAIN_BUDGET_BYTES
            && let Ok(event) = self.events.try_recv()
        {
            match event {
                PtyEvent::Output(bytes) => {
                    drained.output = true;
                    bytes_fed += bytes.len();
                    let reply = emulator.advance(&bytes);
                    if !reply.is_empty() {
                        self.write(&reply)?;
                    }
                }
                PtyEvent::Exited(code) => drained.exited = Some(code),
            }
        }
        Ok(drained)
    }
}

/// Result of [`PtySession::drain`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drained {
    /// Some output was fed to the emulator.
    pub output: bool,
    pub exited: Option<u32>,
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.killer.kill();
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
    fn drain_collects_output_and_exit_without_blocking() {
        let mut session = PtySession::spawn(echo_command("mkrew-drain-ok"), 40, 5).unwrap();
        let mut emu = Emulator::new(40, 5);
        let mut saw_output = false;
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            let drained = session.drain(&mut emu).unwrap();
            saw_output |= drained.output;
            if drained.exited.is_some() && emu.grid().to_text().contains("mkrew-drain-ok") {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_output);
        let text = emu.grid().to_text();
        assert!(text.contains("mkrew-drain-ok"), "grid was:\n{text}");
    }
}
