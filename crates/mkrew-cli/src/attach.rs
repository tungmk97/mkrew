//! `mk attach <session-id>` (SPEC §9): paint the daemon's screen snapshot, then stream raw
//! output frames to stdout and forward stdin as `session.input`. `Ctrl+]` detaches; the agent
//! keeps running in the daemon.
//!
//! Two connections: one streams output/events (read by its own thread), the other carries
//! `session.input` / `session.resize` requests from the main thread. When stdin and stdout are
//! not a terminal (tests, pipes) raw mode and resizing are skipped and stdin EOF detaches.
//! Detaching = closing the streaming connection (the daemon drops its attachments); the
//! user's screen keeps the agent's last output, like `tmux detach`.

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use mkrew_core::Paths;
use mkrew_protocol::paint::{RESET, paint};
use mkrew_protocol::{
    Event, Frame, Message, Request, SessionAttachResult, SessionInputParams, SessionRef,
    SessionResizeParams,
};
use ulid::Ulid;

use crate::client::Client;

/// `Ctrl+]` (GS), as in telnet.
const DETACH: u8 = 0x1d;
/// How often the terminal size is polled (no portable resize signal on Windows).
const RESIZE_POLL: Duration = Duration::from_millis(250);
/// After a failed input request: how long to wait for the `session.exited` that explains it.
const EXIT_GRACE: Duration = Duration::from_secs(1);

enum Msg {
    Stdin(Vec<u8>),
    StdinClosed,
    Exited(Option<i32>),
    Disconnected(anyhow::Error),
}

/// `(cols, rows)` of the controlling terminal, if stdout is one.
pub fn terminal_size() -> Option<(u16, u16)> {
    if !io::stdout().is_terminal() {
        return None;
    }
    crossterm::terminal::size()
        .ok()
        .filter(|&(c, r)| c > 0 && r > 0)
}

pub fn attach(paths: &Paths, session_id: Ulid) -> anyhow::Result<ExitCode> {
    let tty = io::stdin().is_terminal() && io::stdout().is_terminal();
    let mut control = crate::project::connect(paths)?;
    let mut size = if tty { terminal_size() } else { None };
    if let Some((cols, rows)) = size {
        resize(&mut control, session_id, cols, rows)?;
    }
    let mut stream = crate::project::connect(paths)?;
    let attached: SessionAttachResult =
        stream.call(Request::SessionAttach(SessionRef { session_id }))?;

    let raw = if tty { Some(RawMode::enable()?) } else { None };
    let detached = Arc::new(AtomicBool::new(false));
    write_stdout(&detached, paint(&attached.snapshot).as_bytes());

    let (tx, rx) = mpsc::channel();
    spawn_output(stream, session_id, Arc::clone(&detached), tx.clone());
    spawn_stdin(tty, tx);

    let end = loop {
        let msg = match rx.recv_timeout(RESIZE_POLL) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => {
                let now = if tty { terminal_size() } else { None };
                if let Some((cols, rows)) = now.filter(|&now| Some(now) != size) {
                    size = now;
                    if let Err(e) = resize(&mut control, session_id, cols, rows) {
                        break exited_or(&rx, e);
                    }
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break End::Detached,
        };
        match msg {
            Msg::Stdin(bytes) => {
                let (data, detach) = match bytes.iter().position(|&b| b == DETACH) {
                    Some(at) => (&bytes[..at], true),
                    None => (&bytes[..], false),
                };
                if !data.is_empty() {
                    let input = Request::SessionInput(SessionInputParams {
                        session_id,
                        data: data.to_vec(),
                    });
                    if let Err(e) = control.call::<()>(input) {
                        break exited_or(&rx, e);
                    }
                }
                if detach {
                    break End::Detached;
                }
            }
            Msg::StdinClosed => break End::Detached,
            Msg::Exited(code) => break End::Exited(code),
            Msg::Disconnected(e) => break End::Failed(e),
        }
    };

    // Stop the output thread from painting over the reset, then restore the terminal.
    {
        let mut stdout = io::stdout().lock();
        detached.store(true, Ordering::SeqCst);
        let _ = stdout.write_all(RESET.as_bytes());
        let _ = stdout.flush();
    }
    drop(raw);
    match end {
        End::Detached => {
            println!("\n[detached from session {session_id}; it keeps running]");
            Ok(ExitCode::SUCCESS)
        }
        End::Exited(code) => {
            let code = code.map_or_else(|| "unknown".to_owned(), |c| c.to_string());
            println!("\n[session {session_id} exited with code {code}]");
            Ok(ExitCode::SUCCESS)
        }
        End::Failed(e) => Err(e),
    }
}

enum End {
    Detached,
    Exited(Option<i32>),
    Failed(anyhow::Error),
}

/// Input failed: usually because the session just exited, which the output thread reports.
fn exited_or(rx: &mpsc::Receiver<Msg>, error: anyhow::Error) -> End {
    while let Ok(msg) = rx.recv_timeout(EXIT_GRACE) {
        if let Msg::Exited(code) = msg {
            return End::Exited(code);
        }
    }
    End::Failed(error)
}

fn resize(control: &mut Client, session_id: Ulid, cols: u16, rows: u16) -> anyhow::Result<()> {
    control.call::<()>(Request::SessionResize(SessionResizeParams {
        session_id,
        cols,
        rows,
    }))
}

fn write_stdout(detached: &AtomicBool, bytes: &[u8]) {
    let mut stdout = io::stdout().lock();
    if detached.load(Ordering::SeqCst) {
        return;
    }
    let _ = stdout.write_all(bytes);
    let _ = stdout.flush();
}

/// Copies this session's output frames to stdout and repaints on `session.snapshot`.
fn spawn_output(
    mut stream: Client,
    session_id: Ulid,
    detached: Arc<AtomicBool>,
    tx: mpsc::Sender<Msg>,
) {
    std::thread::spawn(move || {
        loop {
            let frame = match stream.next_frame() {
                Ok(frame) => frame,
                Err(e) => {
                    let _ = tx.send(Msg::Disconnected(e));
                    return;
                }
            };
            match frame {
                Frame::Output {
                    session_id: id,
                    data,
                } if id == session_id => {
                    write_stdout(&detached, &data);
                }
                Frame::Message(Message::Event(Event::SessionSnapshot(event)))
                    if event.session_id == session_id =>
                {
                    // The history is already in the terminal's scrollback from the first attach.
                    let mut snapshot = event.snapshot;
                    snapshot.history.clear();
                    write_stdout(&detached, paint(&snapshot).as_bytes());
                }
                Frame::Message(Message::Event(Event::SessionExited(event)))
                    if event.session_id == session_id =>
                {
                    let _ = tx.send(Msg::Exited(event.exit_code));
                    return;
                }
                _ => {}
            }
        }
    });
}

fn spawn_stdin(tty: bool, tx: mpsc::Sender<Msg>) {
    std::thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut buf = [0u8; 16 * 1024];
        loop {
            let msg = match stdin.read(&mut buf) {
                // std's Windows console reader turns a lone Ctrl+Z into a 0-byte read; a
                // console never reaches EOF otherwise, so keep reading (with a pause, so a
                // console that went away cannot spin a core).
                Ok(0) if tty && cfg!(windows) => {
                    std::thread::sleep(Duration::from_millis(20));
                    continue;
                }
                Ok(0) | Err(_) => Msg::StdinClosed,
                Ok(n) => Msg::Stdin(buf[..n].to_vec()),
            };
            let closed = matches!(msg, Msg::StdinClosed);
            if tx.send(msg).is_err() || closed {
                return;
            }
        }
    });
}

/// Raw mode for the lifetime of the guard. On Windows the console must also emit VT
/// sequences for keys (arrows, function keys) and interpret VT output.
struct RawMode {
    #[cfg(windows)]
    console: windows_console::Saved,
}

impl RawMode {
    fn enable() -> anyhow::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self {
            #[cfg(windows)]
            console: windows_console::enable_vt(),
        })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        #[cfg(windows)]
        windows_console::restore(&self.console);
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_console {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        GetConsoleMode, SetConsoleMode,
    };

    /// Console modes before [`enable_vt`]; `None` where the handle is not a console.
    pub struct Saved {
        input: Option<CONSOLE_MODE>,
        output: Option<CONSOLE_MODE>,
    }

    fn get(handle: windows_sys::Win32::Foundation::HANDLE) -> Option<CONSOLE_MODE> {
        let mut mode: CONSOLE_MODE = 0;
        // SAFETY: `handle` is one of mk's live std handles and `mode` a valid out pointer.
        (unsafe { GetConsoleMode(handle, &mut mode) } != 0).then_some(mode)
    }

    fn set(handle: windows_sys::Win32::Foundation::HANDLE, mode: CONSOLE_MODE) {
        // SAFETY: `handle` is one of mk's live std handles; failure keeps the old mode.
        unsafe { SetConsoleMode(handle, mode) };
    }

    pub fn enable_vt() -> Saved {
        let stdin = std::io::stdin().as_raw_handle();
        let stdout = std::io::stdout().as_raw_handle();
        let saved = Saved {
            input: get(stdin),
            output: get(stdout),
        };
        if let Some(mode) = saved.input {
            set(stdin, mode | ENABLE_VIRTUAL_TERMINAL_INPUT);
        }
        if let Some(mode) = saved.output {
            set(stdout, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
        saved
    }

    /// Restores the input/output modes saved by [`enable_vt`].
    pub fn restore(saved: &Saved) {
        if let Some(mode) = saved.input {
            set(std::io::stdin().as_raw_handle(), mode);
        }
        if let Some(mode) = saved.output {
            set(std::io::stdout().as_raw_handle(), mode);
        }
    }
}
