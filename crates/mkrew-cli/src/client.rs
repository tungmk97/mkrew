//! Blocking IPC client (SPEC §7): one request at a time over the local socket.

use std::io::{self, Read, Write};

use anyhow::{Context, bail};
use interprocess::local_socket::{Stream, prelude::*};
use mkrew_protocol::{
    ClientKind, ErrorCode, Frame, FrameDecoder, HelloParams, HelloResult, Message, Outcome,
    PROTOCOL_VERSION, Request, ResponseError, encode,
};
use serde::de::DeserializeOwned;

/// The daemon answered a request with an error. It reads as the daemon's message; the code
/// stays in the `anyhow` chain for callers that act on it (`downcast_ref`).
#[derive(Debug)]
pub struct Refused {
    pub code: ErrorCode,
    pub message: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refused {}

/// Whether `error` is a daemon's `version_mismatch` answer to `hello`.
pub fn is_version_mismatch(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Refused>()
        .is_some_and(|refused| refused.code == ErrorCode::VersionMismatch)
}

pub struct Client {
    stream: Stream,
    decoder: FrameDecoder,
    next_id: u64,
}

impl Client {
    /// Connects and performs `hello`. `Err` from the outer `io::Result` means no daemon is
    /// listening; the inner error is a failed handshake (e.g. version mismatch).
    pub fn connect(socket: &str) -> io::Result<anyhow::Result<(Self, HelloResult)>> {
        let stream = Stream::connect(mkrew_daemon::ipc::name(socket)?)?;
        let mut client = Self {
            stream,
            decoder: FrameDecoder::new(),
            next_id: 1,
        };
        let hello = client.call(Request::Hello(HelloParams {
            client: ClientKind::Cli,
            protocol_version: PROTOCOL_VERSION,
        }));
        Ok(hello.map(|hello| (client, hello)))
    }

    /// Sends `request` and waits for its response.
    pub fn call<T: DeserializeOwned>(&mut self, request: Request) -> anyhow::Result<T> {
        let outcome = self.round_trip(request).context("talk to the daemon")?;
        outcome.into_result().map_err(|e| match e {
            ResponseError::Daemon(body) => anyhow::Error::new(Refused {
                code: body.code,
                message: body.message,
            }),
            ResponseError::Decode(e) => anyhow::anyhow!("invalid response from the daemon: {e}"),
        })
    }

    fn round_trip(&mut self, request: Request) -> anyhow::Result<Outcome> {
        let id = self.next_id;
        self.next_id += 1;
        let frame = Frame::Message(Message::Request { id, request });
        self.stream.write_all(&encode(&frame)?)?;
        loop {
            match self.next_frame()? {
                Frame::Message(Message::Response { id: got, outcome }) if got == id => {
                    return Ok(outcome);
                }
                // Events and output frames are only consumed by `mk attach` via `next_frame`.
                _ => {}
            }
        }
    }

    /// Blocks until the next frame arrives (responses, events or session output).
    pub fn next_frame(&mut self) -> anyhow::Result<Frame> {
        loop {
            if let Some(frame) = self.decoder.next_frame()? {
                return Ok(frame);
            }
            // Only zeroed when a read is needed, not for frames already decoded.
            let mut buf = [0u8; 64 * 1024];
            let n = self.stream.read(&mut buf)?;
            if n == 0 {
                bail!("the daemon closed the connection");
            }
            self.decoder.push(&buf[..n]);
        }
    }
}
