//! Background IPC client for the UI (SPEC §7).
//!
//! Each [`Connection`] owns one OS thread running a single-threaded tokio runtime. The GPUI
//! side never blocks: [`Connection::request`] queues the frame and returns a future, events and
//! session output arrive on [`Connection::incoming`]. Both are runtime-agnostic, so GPUI awaits
//! them on its own executor.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use interprocess::local_socket::tokio::{Stream, prelude::*};
use mkrew_protocol::{
    ClientKind, ErrorCode, Event, Frame, FrameDecoder, HelloParams, HelloResult, Message, Outcome,
    PROTOCOL_VERSION, Request, ResponseError, encode,
};
use serde::de::DeserializeOwned;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::sync::{Semaphore, mpsc, oneshot};
use ulid::Ulid;

/// Frames and output bytes buffered for the UI. When either is full the IPC thread stops
/// reading, so a stalled UI makes the daemon drop queued output and send a `session.snapshot`
/// instead (SPEC §7). One output frame can carry a whole 4 MB attachment queue. At most
/// `INCOMING_BYTES` of output plus `INCOMING_FRAMES` events wait here (and the OS pipe buffer).
const INCOMING_FRAMES: usize = 256;
const INCOMING_BYTES: usize = 16 * 1024 * 1024;
const READ_BUFFER: usize = 64 * 1024;
const HELLO_ID: u64 = 1;
/// Last stderr lines of a program kept for the error report.
const STDERR_LINES: usize = 20;
/// How long a program that failed `hello` gets to exit and flush its stderr.
const PROGRAM_EXIT_WAIT: Duration = Duration::from_secs(2);
/// No console window for `ssh` (`CREATE_NO_WINDOW`).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A push message from the daemon.
#[derive(Debug, Clone, PartialEq)]
// Events are the common case; boxing them would add an allocation per push.
#[allow(clippy::large_enum_variant)]
pub enum Incoming {
    Event(Event),
    /// Raw PTY bytes of an attached session.
    Output {
        session_id: Ulid,
        data: Vec<u8>,
    },
}

/// Receiving end of [`Connection::incoming`]: a channel bounded by frames and by output bytes.
#[derive(Clone)]
pub struct Inbox {
    frames: async_channel::Receiver<Incoming>,
    budget: Arc<Budget>,
}

/// Sending end of an [`Inbox`], used by the IPC thread.
struct Outbox {
    frames: async_channel::Sender<Incoming>,
    budget: Arc<Budget>,
}

/// Output bytes in the channel, as semaphore permits.
struct Budget {
    permits: Semaphore,
    limit: usize,
}

impl Budget {
    /// Clamped to the whole budget, so one oversized frame still fits once the rest drained.
    fn cost(&self, incoming: &Incoming) -> usize {
        match incoming {
            Incoming::Output { data, .. } => data.len().min(self.limit),
            Incoming::Event(_) => 0,
        }
    }

    fn release(&self, incoming: &Incoming) {
        self.permits.add_permits(self.cost(incoming));
    }
}

fn inbox(frames: usize, bytes: usize) -> (Outbox, Inbox) {
    let (tx, rx) = async_channel::bounded(frames);
    let budget = Arc::new(Budget {
        permits: Semaphore::new(bytes),
        limit: bytes,
    });
    let outbox = Outbox {
        frames: tx,
        budget: Arc::clone(&budget),
    };
    (outbox, Inbox { frames: rx, budget })
}

impl Inbox {
    pub async fn recv(&self) -> Result<Incoming, async_channel::RecvError> {
        let incoming = self.frames.recv().await?;
        self.budget.release(&incoming);
        Ok(incoming)
    }

    pub fn try_recv(&self) -> Result<Incoming, async_channel::TryRecvError> {
        let incoming = self.frames.try_recv()?;
        self.budget.release(&incoming);
        Ok(incoming)
    }
}

impl Outbox {
    /// Waits for room; fails once every [`Inbox`] is gone.
    async fn send(&self, incoming: Incoming) -> anyhow::Result<()> {
        let cost = u32::try_from(self.budget.cost(&incoming)).context("output frame too large")?;
        if cost > 0 {
            // Never closed; the permits come back as the inbox hands the frame out. Cancelling
            // before the send below loses them, which only happens when the connection ends.
            self.budget.permits.acquire_many(cost).await?.forget();
        }
        self.frames
            .send(incoming)
            .await
            .map_err(|_| anyhow!("the UI dropped the incoming channel"))
    }
}

/// A greeted connection to the daemon. Dropping it closes the connection.
pub struct Connection {
    requests: mpsc::UnboundedSender<Pending>,
    incoming: Inbox,
    next_id: AtomicU64,
    hello: HelloResult,
}

struct Pending {
    id: u64,
    bytes: Vec<u8>,
    reply: oneshot::Sender<Outcome>,
}

type Opened = io::Result<anyhow::Result<Connection>>;

/// Where the daemon is reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Socket(String),
    /// A program whose stdin/stdout carry the frames (`ssh … mk proxy`).
    Program {
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
}

/// Why opening a program endpoint failed before `hello` was answered.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The program could not be started.
    #[error("cannot run {program}: {source}")]
    Spawn { program: String, source: io::Error },
    /// It ended (or closed its stdout) before answering `hello`; `stderr` = its last lines.
    #[error("the connection ended before it was ready")]
    Ended { stderr: String },
    /// The daemon answered `hello` with `version_mismatch`.
    #[error("{message}")]
    Version { message: String },
}

impl Connection {
    /// Connects and performs `hello` (blocking). `Err` from the outer `io::Result` means the
    /// endpoint could not be reached (e.g. no daemon is listening); the inner error is a
    /// failed handshake (e.g. version mismatch, or an [`OpenError`] for a program).
    pub fn open(endpoint: &Endpoint) -> Opened {
        let endpoint = endpoint.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("mkrew-ipc".into())
            .spawn(move || run(endpoint, ready_tx))?;
        ready_rx
            .recv()
            .map_err(|_| io::Error::other("the IPC thread stopped during connect"))?
    }

    /// What the daemon answered to `hello`.
    pub fn hello(&self) -> &HelloResult {
        &self.hello
    }

    /// Events and session output, in arrival order. The UI must keep draining it: while it is
    /// full no responses are read either. Closed once the daemon connection is gone.
    pub fn incoming(&self) -> Inbox {
        self.incoming.clone()
    }

    /// Queues `request` now (requests go out in call order) and resolves with its outcome.
    pub fn request(
        &self,
        request: Request,
    ) -> impl Future<Output = anyhow::Result<Outcome>> + Send + 'static {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, response) = oneshot::channel();
        let queued = encode(&Frame::Message(Message::Request { id, request }))
            .context("encode the request")
            .and_then(|bytes| {
                self.requests
                    .send(Pending { id, bytes, reply })
                    .map_err(|_| anyhow!("the daemon connection is closed"))
            });
        async move {
            queued?;
            response
                .await
                .map_err(|_| anyhow!("the daemon connection closed before replying"))
        }
    }

    /// Like [`Connection::request`], decoding a successful result as `T`.
    pub fn call<T: DeserializeOwned>(
        &self,
        request: Request,
    ) -> impl Future<Output = anyhow::Result<T>> + Send + 'static {
        let outcome = self.request(request);
        async move { into_result(outcome.await?) }
    }
}

fn into_result<T: DeserializeOwned>(outcome: Outcome) -> anyhow::Result<T> {
    outcome.into_result().map_err(|e| match e {
        ResponseError::Daemon(body) => anyhow!("{}", body.message),
        ResponseError::Decode(e) => anyhow!("invalid response from the daemon: {e}"),
    })
}

/// Body of the IPC thread: connect, greet, report to [`Connection::open`], then serve.
fn run(endpoint: Endpoint, ready: std::sync::mpsc::Sender<Opened>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    runtime.block_on(async move {
        match endpoint {
            Endpoint::Socket(socket) => {
                let stream = match connect(&socket).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                let (rx, tx) = stream.split();
                serve(rx, tx, None, ready).await;
            }
            Endpoint::Program { program, args, env } => {
                let mut command = tokio::process::Command::new(&program);
                command
                    .args(&args)
                    .envs(env)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true);
                #[cfg(windows)]
                command.creation_flags(CREATE_NO_WINDOW);
                let mut child = match command.spawn() {
                    Ok(child) => child,
                    Err(source) => {
                        let _ = ready.send(Ok(Err(OpenError::Spawn { program, source }.into())));
                        return;
                    }
                };
                // All three are `Some`: they were piped above.
                let (Some(rx), Some(tx), Some(stderr)) =
                    (child.stdout.take(), child.stdin.take(), child.stderr.take())
                else {
                    let _ = ready.send(Err(io::Error::other("the program has no pipes")));
                    return;
                };
                let tail = StderrTail::spawn(stderr);
                serve(rx, tx, Some((child, tail)), ready).await;
            }
        }
    });
}

/// Longest stderr line kept, in bytes; the rest of a longer line is read and dropped.
const STDERR_LINE_MAX: usize = 4096;

/// Reads `reader` to EOF (or an I/O error) keeping the last [`STDERR_LINES`] lines. Bytes that
/// are not UTF-8 (localized or OEM-codepage ssh messages) are replaced, never fatal, and a line
/// without a newline cannot grow past [`STDERR_LINE_MAX`] in memory.
async fn keep_lines<R: AsyncBufRead + Unpin>(
    mut reader: R,
    kept: &Mutex<VecDeque<String>>,
) -> io::Result<()> {
    let mut line: Vec<u8> = Vec::new();
    loop {
        let chunk = reader.fill_buf().await?;
        let eof = chunk.is_empty();
        let end = chunk.iter().position(|&b| b == b'\n');
        let take = end.unwrap_or(chunk.len());
        let room = STDERR_LINE_MAX.saturating_sub(line.len());
        line.extend_from_slice(&chunk[..take.min(room)]);
        reader.consume(end.map_or(take, |i| i + 1));
        if end.is_some() || eof {
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            if !text.is_empty() {
                let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
                if kept.len() == STDERR_LINES {
                    kept.pop_front();
                }
                kept.push_back(text.to_owned());
            }
            line.clear();
        }
        if eof {
            return Ok(());
        }
    }
}

/// The last lines a program wrote to stderr. Drained continuously: a full pipe would block
/// `ssh`.
struct StderrTail {
    lines: Arc<Mutex<VecDeque<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl StderrTail {
    fn spawn(stderr: tokio::process::ChildStderr) -> Self {
        let lines = Arc::new(Mutex::new(VecDeque::new()));
        let kept = Arc::clone(&lines);
        let task = tokio::spawn(async move {
            let _ = keep_lines(BufReader::new(stderr), &kept).await;
        });
        Self { lines, task }
    }

    fn text(&self) -> String {
        let lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        lines
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Greets, reports to [`Connection::open`], then serves until either end closes. A program
/// lives in here, so dropping the `Connection` ends `write_loop` and kills it.
async fn serve<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut rx: R,
    mut tx: W,
    mut program: Option<(tokio::process::Child, StderrTail)>,
    ready: std::sync::mpsc::Sender<Opened>,
) {
    let mut decoder = FrameDecoder::new();
    let hello = match greet(&mut rx, &mut tx, &mut decoder).await {
        Ok(hello) => hello,
        Err(mut error) => {
            if let Some((child, tail)) = program.as_mut()
                && !error.is::<OpenError>()
            {
                // Let it finish its last words (ssh's reason) before reporting.
                let _ = tokio::time::timeout(PROGRAM_EXIT_WAIT, async {
                    let _ = child.wait().await;
                    let _ = (&mut tail.task).await;
                })
                .await;
                error = OpenError::Ended {
                    stderr: tail.text(),
                }
                .into();
            }
            let _ = ready.send(Ok(Err(error)));
            return;
        }
    };
    let (requests, mut outbox) = mpsc::unbounded_channel();
    let (incoming_tx, incoming) = inbox(INCOMING_FRAMES, INCOMING_BYTES);
    let connection = Connection {
        requests,
        incoming,
        next_id: AtomicU64::new(HELLO_ID + 1),
        hello,
    };
    if ready.send(Ok(Ok(connection))).is_err() {
        return;
    }
    let pending = RefCell::new(HashMap::new());
    // Each loop runs until its end closes; neither is cancelled and then resumed (racing
    // single pipe reads in `select!` loses data on Windows). Whichever ends first ends the
    // connection: the daemon went away, or the UI dropped the `Connection`.
    tokio::select! {
        _ = read_loop(rx, decoder, &pending, incoming_tx) => {}
        _ = write_loop(tx, &mut outbox, &pending) => {}
    }
    // Dropping `pending` fails every request still waiting for a reply.
}

async fn connect(socket: &str) -> io::Result<Stream> {
    Stream::connect(mkrew_daemon::ipc::name(socket)?).await
}

async fn greet<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    rx: &mut R,
    tx: &mut W,
    decoder: &mut FrameDecoder,
) -> anyhow::Result<HelloResult> {
    let hello = Request::Hello(HelloParams {
        client: ClientKind::Ui,
        protocol_version: PROTOCOL_VERSION,
    });
    let frame = Frame::Message(Message::Request {
        id: HELLO_ID,
        request: hello,
    });
    tx.write_all(&encode(&frame)?).await?;
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        // Frames after the reply (events) stay in `decoder` for the read loop.
        if let Some(frame) = decoder.next_frame()? {
            match frame {
                Frame::Message(Message::Response {
                    id: HELLO_ID,
                    outcome,
                }) => {
                    if let Outcome::Error(body) = &outcome
                        && body.code == ErrorCode::VersionMismatch
                    {
                        return Err(OpenError::Version {
                            message: body.message.clone(),
                        }
                        .into());
                    }
                    return into_result(outcome);
                }
                _ => bail!("the daemon sent {frame:?} before greeting"),
            }
        }
        let n = rx.read(&mut buf).await?;
        if n == 0 {
            bail!("the daemon closed the connection during hello");
        }
        decoder.push(&buf[..n]);
    }
}

async fn read_loop<R: AsyncRead + Unpin>(
    mut rx: R,
    mut decoder: FrameDecoder,
    pending: &RefCell<HashMap<u64, oneshot::Sender<Outcome>>>,
    incoming: Outbox,
) -> anyhow::Result<()> {
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        while let Some(frame) = decoder.next_frame()? {
            let push = match frame {
                Frame::Message(Message::Response { id, outcome }) => {
                    let reply = pending.borrow_mut().remove(&id);
                    if let Some(reply) = reply {
                        // The caller may have dropped its future; nothing to do then.
                        let _ = reply.send(outcome);
                    }
                    continue;
                }
                Frame::Message(Message::Event(event)) => Incoming::Event(event),
                Frame::Output { session_id, data } => Incoming::Output { session_id, data },
                Frame::Message(Message::Request { .. }) => bail!("the daemon sent a request"),
            };
            // Fails only once the `Connection` (which holds an inbox) is gone.
            incoming.send(push).await?;
        }
        let n = rx.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        decoder.push(&buf[..n]);
    }
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mut tx: W,
    outbox: &mut mpsc::UnboundedReceiver<Pending>,
    pending: &RefCell<HashMap<u64, oneshot::Sender<Outcome>>>,
) -> io::Result<()> {
    while let Some(Pending { id, bytes, reply }) = outbox.recv().await {
        // Registered before the write, so the reply always finds it.
        pending.borrow_mut().insert(id, reply);
        tx.write_all(&bytes).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::future::{block_on, poll_once};
    use mkrew_protocol::SessionExitedEvent;

    #[test]
    fn stderr_lines_survive_invalid_utf8_and_floods() {
        let kept = Mutex::new(VecDeque::new());
        let mut bytes = b"first\n".to_vec();
        bytes.extend_from_slice(&[0xff, 0xfe, b'x', b'\n']);
        bytes.extend_from_slice(b"  last words  ");
        block_on(keep_lines(&bytes[..], &kept)).unwrap();
        let lines: Vec<String> = kept.lock().unwrap().iter().cloned().collect();
        assert_eq!(lines, ["first", "\u{fffd}\u{fffd}x", "last words"]);

        let flood = vec![b'a'; STDERR_LINE_MAX * 10];
        block_on(keep_lines(&flood[..], &kept)).unwrap();
        assert_eq!(kept.lock().unwrap().back().unwrap().len(), STDERR_LINE_MAX);

        let many: Vec<u8> = (0..30)
            .flat_map(|n| format!("{n}\n").into_bytes())
            .collect();
        let kept = Mutex::new(VecDeque::new());
        block_on(keep_lines(&many[..], &kept)).unwrap();
        assert_eq!(kept.lock().unwrap().len(), STDERR_LINES);
        assert_eq!(kept.lock().unwrap().front().unwrap(), "10");
    }

    fn output(len: usize) -> Incoming {
        Incoming::Output {
            session_id: Ulid::nil(),
            data: vec![b'x'; len],
        }
    }

    fn exited() -> Incoming {
        Incoming::Event(Event::SessionExited(SessionExitedEvent {
            session_id: Ulid::nil(),
            exit_code: Some(0),
        }))
    }

    #[test]
    fn output_waits_until_the_inbox_hands_out_enough_bytes() {
        block_on(async {
            let (outbox, inbox) = inbox(8, 10);
            outbox.send(output(6)).await.unwrap();
            let mut second = Box::pin(outbox.send(output(6)));
            assert!(poll_once(&mut second).await.is_none(), "16 bytes > 10");
            assert_eq!(inbox.try_recv().unwrap(), output(6));
            poll_once(&mut second).await.unwrap().unwrap();
            assert_eq!(inbox.recv().await.unwrap(), output(6));
        });
    }

    #[test]
    fn events_are_bounded_by_frames_only() {
        block_on(async {
            let (outbox, inbox) = inbox(2, 10);
            outbox.send(output(10)).await.unwrap();
            outbox.send(exited()).await.unwrap();
            assert!(poll_once(Box::pin(outbox.send(exited()))).await.is_none());
            assert_eq!(inbox.recv().await.unwrap(), output(10));
            assert_eq!(inbox.recv().await.unwrap(), exited());
        });
    }

    #[test]
    fn an_oversized_frame_goes_through_once_the_rest_drained() {
        block_on(async {
            let (outbox, inbox) = inbox(8, 10);
            outbox.send(output(4)).await.unwrap();
            let mut big = Box::pin(outbox.send(output(25)));
            assert!(poll_once(&mut big).await.is_none());
            inbox.recv().await.unwrap();
            poll_once(&mut big).await.unwrap().unwrap();
            assert_eq!(inbox.recv().await.unwrap(), output(25));
            outbox.send(output(10)).await.unwrap();
        });
    }

    #[test]
    fn send_fails_once_the_inbox_is_gone() {
        block_on(async {
            let (outbox, inbox) = inbox(8, 10);
            drop(inbox);
            assert!(outbox.send(exited()).await.is_err());
        });
    }
}
