//! Blocking IPC client (SPEC §7): one request at a time over the local socket.

use std::io::{self, Read, Write};

use anyhow::{Context, bail};
use interprocess::local_socket::{Stream, prelude::*};
use mkrew_protocol::{
    ClientKind, Frame, FrameDecoder, HelloParams, HelloResult, Message, Outcome, PROTOCOL_VERSION,
    Request, ResponseError, encode,
};
use serde::de::DeserializeOwned;

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
            ResponseError::Daemon(body) => anyhow::anyhow!("{}", body.message),
            ResponseError::Decode(e) => anyhow::anyhow!("invalid response from the daemon: {e}"),
        })
    }

    fn round_trip(&mut self, request: Request) -> anyhow::Result<Outcome> {
        let id = self.next_id;
        self.next_id += 1;
        let frame = Frame::Message(Message::Request { id, request });
        self.stream.write_all(&encode(&frame)?)?;
        let mut buf = [0u8; 4096];
        loop {
            while let Some(frame) = self.decoder.next_frame()? {
                match frame {
                    Frame::Message(Message::Response { id: got, outcome }) if got == id => {
                        return Ok(outcome);
                    }
                    // Nothing subscribes yet (M1 attach comes later): ignore events/output.
                    _ => {}
                }
            }
            let n = self.stream.read(&mut buf)?;
            if n == 0 {
                bail!("the daemon closed the connection");
            }
            self.decoder.push(&buf[..n]);
        }
    }
}
