//! Background IPC client for the UI (SPEC §7).
//!
//! Each [`Connection`] owns one OS thread running a single-threaded tokio runtime. The GPUI
//! side never blocks: [`Connection::request`] queues the frame and returns a future, events and
//! session output arrive on [`Connection::incoming`]. Both are runtime-agnostic, so GPUI awaits
//! them on its own executor.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, anyhow, bail};
use interprocess::local_socket::tokio::{RecvHalf, SendHalf, Stream, prelude::*};
use mkrew_protocol::{
    ClientKind, Event, Frame, FrameDecoder, HelloParams, HelloResult, Message, Outcome,
    PROTOCOL_VERSION, Request, ResponseError, encode,
};
use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use ulid::Ulid;

/// Frames buffered for the UI. When full the IPC thread stops reading, so a stalled UI makes
/// the daemon drop queued output and send a `session.snapshot` instead (SPEC §7).
const INCOMING_FRAMES: usize = 256;
const READ_BUFFER: usize = 64 * 1024;
const HELLO_ID: u64 = 1;

/// A push message from the daemon.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Event(Event),
    /// Raw PTY bytes of an attached session.
    Output {
        session_id: Ulid,
        data: Vec<u8>,
    },
}

/// A greeted connection to the daemon. Dropping it closes the connection.
pub struct Connection {
    requests: mpsc::UnboundedSender<Pending>,
    incoming: async_channel::Receiver<Incoming>,
    next_id: AtomicU64,
    hello: HelloResult,
}

struct Pending {
    id: u64,
    bytes: Vec<u8>,
    reply: oneshot::Sender<Outcome>,
}

type Opened = io::Result<anyhow::Result<Connection>>;

impl Connection {
    /// Connects and performs `hello` (blocking). `Err` from the outer `io::Result` means the
    /// endpoint could not be reached (e.g. no daemon is listening); the inner error is a
    /// failed handshake (e.g. version mismatch).
    pub fn open(socket: &str) -> Opened {
        let socket = socket.to_owned();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("mkrew-ipc".into())
            .spawn(move || run(&socket, ready_tx))?;
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
    pub fn incoming(&self) -> async_channel::Receiver<Incoming> {
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
fn run(socket: &str, ready: std::sync::mpsc::Sender<Opened>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    runtime.block_on(async move {
        let stream = match connect(socket).await {
            Ok(stream) => stream,
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        let (mut rx, mut tx) = stream.split();
        let mut decoder = FrameDecoder::new();
        let hello = greet(&mut rx, &mut tx, &mut decoder).await;
        let hello = match hello {
            Ok(hello) => hello,
            Err(e) => {
                let _ = ready.send(Ok(Err(e)));
                return;
            }
        };
        let (requests, mut outbox) = mpsc::unbounded_channel();
        let (incoming_tx, incoming) = async_channel::bounded(INCOMING_FRAMES);
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
    });
}

async fn connect(socket: &str) -> io::Result<Stream> {
    Stream::connect(mkrew_daemon::ipc::name(socket)?).await
}

async fn greet(
    rx: &mut RecvHalf,
    tx: &mut SendHalf,
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

async fn read_loop(
    mut rx: RecvHalf,
    mut decoder: FrameDecoder,
    pending: &RefCell<HashMap<u64, oneshot::Sender<Outcome>>>,
    incoming: async_channel::Sender<Incoming>,
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
            // Fails only once the `Connection` (which holds a receiver) is gone.
            incoming.send(push).await?;
        }
        let n = rx.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        decoder.push(&buf[..n]);
    }
}

async fn write_loop(
    mut tx: SendHalf,
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
