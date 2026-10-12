//! Phone access: while `[web] enabled = true` in `settings.toml`, the daemon runs `mk web` as a
//! child and reports it in `daemon.status` (`web`). The daemon itself opens no network socket:
//! the listener lives in the `mk web` process (SPEC §15).
//!
//! [`start`] is one task: it reads the switch, runs the child until it exits or the switch
//! goes off, and starts it again after [`backoff`]. [`SettingsHolder::changed`] wakes it. The
//! child dies with the daemon (`kill_on_drop`, and the task is dropped when the daemon ends).

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use mkrew_protocol::WebStatus;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::settings::SettingsHolder;
use crate::spawn;

/// Names the program to run instead of looking for `mk` (tests point it at the fake agent).
pub const ENV_MK_EXE: &str = "MKREW_MK_EXE";

/// A child that ran this long forgives the failures before it.
const STABLE: Duration = Duration::from_secs(60);
/// The longest wait between two starts.
const MAX_WAIT: Duration = Duration::from_secs(60);
/// How long to wait before looking for a missing `mk` again (a stat, not a spin).
const LOOK_AGAIN: Duration = Duration::from_secs(60);
/// How long to wait for the last stderr lines of a child that exited.
const DRAIN: Duration = Duration::from_secs(1);

/// What `daemon.status` reports; the supervisor task is its only writer.
#[derive(Default)]
pub struct Web {
    status: Mutex<WebStatus>,
}

impl Web {
    pub fn status(&self) -> WebStatus {
        self.lock().clone()
    }

    fn set(&self, enabled: bool, running: bool, error: Option<String>) {
        *self.lock() = WebStatus {
            enabled,
            running,
            error,
        };
    }

    fn lock(&self) -> MutexGuard<'_, WebStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Supervises `mk web` per `settings.current().web.enabled` until aborted; aborting (or
/// dropping the runtime) kills the child.
pub fn start(web: Arc<Web>, settings: Arc<SettingsHolder>) -> JoinHandle<()> {
    tokio::spawn(supervise(web, settings))
}

/// How a child run ended.
enum Ended {
    /// The switch went off; the child was killed.
    Disabled,
    /// There is no `mk` to run.
    NotFound,
    /// The child exited, or could not start, after running for `ran`.
    Exited { ran: Duration, error: String },
}

async fn supervise(web: Arc<Web>, settings: Arc<SettingsHolder>) {
    let mut failures = 0u32;
    loop {
        if !settings.current().web.enabled {
            web.set(false, false, None);
            failures = 0;
            settings.changed().await;
            continue;
        }
        match run_child(&web, &settings).await {
            // The next turn of the loop publishes "off".
            Ended::Disabled => {}
            Ended::NotFound => {
                web.set(true, false, Some(not_found_message()));
                // Look again once a minute (or at once when the settings change), so installing
                // `mk` later just works and the dialog's "Retrying…" stays true.
                sleep_while_enabled(&settings, LOOK_AGAIN).await;
            }
            Ended::Exited { ran, error } => {
                let (wait, next) = after_exit(failures, ran);
                failures = next;
                tracing::warn!(?ran, ?wait, "mk web stopped, starting it again");
                web.set(true, false, Some(error));
                sleep_while_enabled(&settings, wait).await;
            }
        }
    }
}

/// Starts `mk web` and returns when it is over. Publishes "running" once it started.
async fn run_child(web: &Web, settings: &SettingsHolder) -> Ended {
    let mut child = match spawn_mk() {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ended::NotFound,
        Err(e) => {
            return Ended::Exited {
                ran: Duration::ZERO,
                error: format!("cannot start mk web: {e}"),
            };
        }
    };
    let started = Instant::now();
    tracing::info!(pid = ?child.id(), "mk web started");
    web.set(true, true, None);
    // Read while it runs: a full stderr pipe would block `mk web`.
    let reader = child
        .stderr
        .take()
        .map(|pipe| tokio::spawn(last_line(BufReader::new(pipe))));
    loop {
        tokio::select! {
            status = child.wait() => {
                let last = match reader {
                    Some(reader) => finish(reader).await,
                    None => None,
                };
                let how = match status {
                    Ok(status) => status.to_string(),
                    Err(e) => e.to_string(),
                };
                return Ended::Exited { ran: started.elapsed(), error: exit_error(last, &how) };
            }
            () = settings.changed() => {
                // Any other setting changing leaves the child alone.
                if !settings.current().web.enabled {
                    if let Err(e) = child.kill().await {
                        tracing::warn!("cannot kill mk web: {e}");
                    }
                    if let Some(reader) = &reader {
                        reader.abort();
                    }
                    tracing::info!("mk web stopped: phone access turned off");
                    return Ended::Disabled;
                }
            }
        }
    }
}

/// Sleeps `wait`, or less when the switch goes off (the caller's loop then publishes "off").
async fn sleep_while_enabled(settings: &SettingsHolder, wait: Duration) {
    let deadline = Instant::now() + wait;
    loop {
        tokio::select! {
            () = tokio::time::sleep_until(deadline) => return,
            () = settings.changed() => {
                if !settings.current().web.enabled {
                    return;
                }
            }
        }
    }
}

/// `mk web`: no console window (the daemon has none), no input, output ignored, stderr piped.
fn spawn_mk() -> io::Result<Child> {
    let mut command = Command::new(mk_exe());
    command
        .arg("web")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // Without this the child would open a console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn()
}

/// `MKREW_MK_EXE` when set (tests), else `mk` next to the running daemon, else `PATH`.
fn mk_exe() -> PathBuf {
    match std::env::var_os(ENV_MK_EXE).filter(|v| !v.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => spawn::sibling_exe("mk").unwrap_or_else(|_| PathBuf::from("mk")),
    }
}

fn not_found_message() -> String {
    format!(
        "mk{} not found next to mkrew-daemon",
        std::env::consts::EXE_SUFFIX
    )
}

/// The wait before the next start, 1, 2, 4 … s up to a minute, for the `failures`-th failure in
/// a row (0 = the first).
fn backoff(failures: u32) -> Duration {
    // 1 << 6 = 64 s is already over the cap; the `min` also keeps the shift in range.
    Duration::from_secs(1u64 << failures.min(6)).min(MAX_WAIT)
}

/// The wait before the next start and the new failure count after a child ran for `ran`: a run
/// of [`STABLE`] or more starts the count over.
fn after_exit(failures: u32, ran: Duration) -> (Duration, u32) {
    let failures = if ran >= STABLE { 0 } else { failures };
    (backoff(failures), failures.saturating_add(1))
}

/// Reads `reader` to its end; the last line with text, trimmed. Bytes that are not UTF-8 are
/// replaced, never a reason to stop reading.
async fn last_line(mut reader: impl AsyncBufRead + Unpin) -> Option<String> {
    let mut last = None;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => return last,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim();
                if !line.is_empty() {
                    last = Some(line.to_owned());
                }
            }
        }
    }
}

/// The stderr reader's result once the child is gone. Bounded: a grandchild that keeps the pipe
/// open must not hold up the restart.
async fn finish(mut reader: JoinHandle<Option<String>>) -> Option<String> {
    let last = tokio::time::timeout(DRAIN, &mut reader).await;
    reader.abort();
    last.ok()?.ok()?
}

/// What `daemon.status` shows for an exited child: its last stderr line, else its exit status.
fn exit_error(last_line: Option<String>, how: &str) -> String {
    last_line.unwrap_or_else(|| format!("mk web stopped ({how})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_a_minute_and_stays() {
        let secs: Vec<u64> = (0..9).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(backoff(u32::MAX).as_secs(), 60, "no shift overflow");
    }

    #[test]
    fn a_stable_run_forgives_earlier_failures() {
        let quick = Duration::from_secs(2);
        let mut failures = 0;
        let mut waits = Vec::new();
        for ran in [quick, quick, quick, STABLE, quick] {
            let (wait, next) = after_exit(failures, ran);
            waits.push(wait.as_secs());
            failures = next;
        }
        // 1, 2, 4; the run of a minute starts over at 1; the next quick exit waits 2.
        assert_eq!(waits, [1, 2, 4, 1, 2]);
    }

    #[test]
    fn just_under_a_minute_is_still_a_failure() {
        let (wait, next) = after_exit(3, STABLE - Duration::from_millis(1));
        assert_eq!((wait.as_secs(), next), (8, 4));
    }

    #[tokio::test]
    async fn last_line_is_the_last_one_with_text() {
        let text = b"starting\nError: port 8787 in use\n\n  \r\n";
        let last = last_line(BufReader::new(&text[..])).await;
        assert_eq!(last.as_deref(), Some("Error: port 8787 in use"));

        let no_newline = b"first\n  second \r\nthird";
        let last = last_line(BufReader::new(&no_newline[..])).await;
        assert_eq!(last.as_deref(), Some("third"));
        assert_eq!(last_line(BufReader::new(&b"\n \n"[..])).await, None);
    }

    #[tokio::test]
    async fn last_line_keeps_reading_past_invalid_utf8() {
        // Stopping at the bad line would leave the pipe unread and block `mk web` on a full one.
        let text = b"ok\nbad \xff byte\nfine\n";
        let last = last_line(BufReader::new(&text[..])).await;
        assert_eq!(last.as_deref(), Some("fine"));
    }

    #[test]
    fn exit_error_prefers_the_stderr_line() {
        assert_eq!(exit_error(Some("boom".into()), "exit code: 1"), "boom");
        assert_eq!(
            exit_error(None, "exit code: 3"),
            "mk web stopped (exit code: 3)"
        );
    }

    fn holder(dir: &tempfile::TempDir, enabled: bool) -> SettingsHolder {
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, format!("[web]\nenabled = {enabled}\n")).unwrap();
        SettingsHolder::load(path)
    }

    #[tokio::test(start_paused = true)]
    async fn a_missing_mk_is_looked_up_again_once_a_minute() {
        assert_eq!(LOOK_AGAIN, Duration::from_secs(60), "not a spin");
        let dir = tempfile::tempdir().unwrap();
        let holder = holder(&dir, true);
        let wait = sleep_while_enabled(&holder, LOOK_AGAIN);
        tokio::pin!(wait);
        let early = tokio::time::timeout(LOOK_AGAIN - Duration::from_secs(1), &mut wait).await;
        assert!(early.is_err(), "no look before the minute is over");
        let late = tokio::time::timeout(Duration::from_secs(2), &mut wait).await;
        assert!(late.is_ok(), "the minute ends the wait");
    }

    #[tokio::test(start_paused = true)]
    async fn turning_it_off_ends_the_wait_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let holder = holder(&dir, true);
        std::fs::write(dir.path().join("settings.toml"), "[web]\nenabled = false\n").unwrap();
        holder.reload();
        let done = tokio::time::timeout(
            Duration::from_secs(1),
            sleep_while_enabled(&holder, LOOK_AGAIN),
        )
        .await;
        assert!(done.is_ok());
    }
}
