//! One browser WebSocket ↔ one daemon connection (ADR 0051). The browser speaks SPEC §7 JSON
//! in text messages. The relay greets the daemon itself, refuses `hello` and `session.resize`
//! (a phone width would reflow the agent), adds `ansi` (the `mk attach` painter) next to every
//! screen snapshot, and sends terminal output as binary messages: the 26-character session id,
//! then the raw PTY bytes.

use std::io::ErrorKind;

use anyhow::{anyhow, bail};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use interprocess::local_socket::tokio::{RecvHalf, SendHalf, Stream, prelude::*};
use mkrew_protocol::paint::paint;
use mkrew_protocol::{
    ClientKind, ErrorCode, Event, Frame, FrameDecoder, HelloParams, HelloResult, Message, Outcome,
    PROTOCOL_VERSION, Request, ResponseError, ScreenSnapshot, encode,
};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const READ_BUFFER: usize = 64 * 1024;
/// Messages waiting for a slow browser. When full the relay stops reading the daemon, so the
/// daemon drops queued output and sends a `session.snapshot` instead (SPEC §7 backpressure).
const OUTBOX: usize = 64;
/// Browser ids start at 1.
const HELLO_ID: u64 = 0;

type Ws = WebSocketStream<TcpStream>;

/// A greeted daemon connection.
pub struct Daemon {
    rx: RecvHalf,
    tx: SendHalf,
    decoder: FrameDecoder,
}

/// Why the relay ended, for `mk web`'s stderr; `Ok` when the browser left.
pub async fn relay(ws: Ws, daemon: Daemon) -> anyhow::Result<()> {
    let Daemon { rx, tx, decoder } = daemon;
    let (mut sink, stream) = ws.split();
    let (out, mut outbox) = mpsc::channel::<WsMessage>(OUTBOX);
    let replies = out.clone();
    // Each loop runs until its side closes; none is cancelled and resumed (racing single pipe
    // reads in `select!` loses data on Windows, see mkrew-ui `client.rs`).
    let ended = tokio::select! {
        r = write_browser(&mut sink, &mut outbox) => r,
        r = daemon_to_browser(rx, decoder, out) => r,
        r = browser_to_daemon(stream, tx, replies) => r,
    };
    let last = match &ended {
        Ok(()) => WsMessage::Close(None),
        Err(e) => close(&format!("{e:#}")),
    };
    let _ = sink.send(last).await;
    ended
}

async fn write_browser(
    sink: &mut SplitSink<Ws, WsMessage>,
    outbox: &mut mpsc::Receiver<WsMessage>,
) -> anyhow::Result<()> {
    while let Some(message) = outbox.recv().await {
        sink.send(message).await?;
    }
    Ok(())
}

async fn daemon_to_browser(
    mut rx: RecvHalf,
    mut decoder: FrameDecoder,
    out: mpsc::Sender<WsMessage>,
) -> anyhow::Result<()> {
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        while let Some(frame) = decoder.next_frame()? {
            out.send(browser_message(frame)?).await?;
        }
        let n = rx.read(&mut buf).await?;
        if n == 0 {
            bail!("the Mkrew daemon closed the connection");
        }
        decoder.push(&buf[..n]);
    }
}

async fn browser_to_daemon(
    mut stream: SplitStream<Ws>,
    mut tx: SendHalf,
    replies: mpsc::Sender<WsMessage>,
) -> anyhow::Result<()> {
    while let Some(message) = stream.next().await {
        // A phone that sleeps or loses signal drops the TCP link: that is it leaving, not a fault.
        let Ok(message) = message else { break };
        match message {
            WsMessage::Text(text) => match daemon_request(&text) {
                FromBrowser::Forward(bytes) => tx.write_all(&bytes).await?,
                FromBrowser::Reply(reply) => replies.send(WsMessage::text(reply)).await?,
                FromBrowser::Ignore => {}
            },
            WsMessage::Close(_) => break,
            // Pings are answered by tungstenite; the page sends no binary messages.
            _ => {}
        }
    }
    Ok(())
}

/// Connects and greets like the UI does (`mkrew-ui` `client.rs`).
pub async fn open_daemon(socket: &str) -> anyhow::Result<Daemon> {
    let stream = match Stream::connect(mkrew_daemon::ipc::name(socket)?).await {
        Ok(stream) => stream,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
            bail!("the Mkrew daemon is not running on this PC")
        }
        Err(e) => return Err(anyhow::Error::new(e).context("connect to the Mkrew daemon")),
    };
    let (mut rx, mut tx) = stream.split();
    let hello = Frame::Message(Message::Request {
        id: HELLO_ID,
        request: Request::Hello(HelloParams {
            client: ClientKind::Ui,
            protocol_version: PROTOCOL_VERSION,
        }),
    });
    tx.write_all(&encode(&hello)?).await?;
    let mut decoder = FrameDecoder::new();
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        if let Some(frame) = decoder.next_frame()? {
            return match frame {
                Frame::Message(Message::Response {
                    id: HELLO_ID,
                    outcome,
                }) => {
                    outcome.into_result::<HelloResult>().map_err(|e| match e {
                        ResponseError::Daemon(body) => {
                            anyhow!("the Mkrew daemon refused mk web: {}", body.message)
                        }
                        other => anyhow!("invalid hello reply: {other}"),
                    })?;
                    Ok(Daemon { rx, tx, decoder })
                }
                other => Err(anyhow!("the daemon sent {other:?} before greeting")),
            };
        }
        let n = rx.read(&mut buf).await?;
        if n == 0 {
            bail!("the Mkrew daemon closed the connection during hello");
        }
        decoder.push(&buf[..n]);
    }
}

/// What a browser text message becomes.
#[derive(Debug, PartialEq, Eq)]
pub enum FromBrowser {
    /// An encoded §7 frame for the daemon.
    Forward(Vec<u8>),
    /// An error response for the browser (same id).
    Reply(String),
    /// Not JSON, or no id to answer.
    Ignore,
}

pub fn daemon_request(text: &str) -> FromBrowser {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return FromBrowser::Ignore;
    };
    let id = value.get("id").and_then(serde_json::Value::as_u64);
    let refuse = |why: String| match id {
        Some(id) => serde_json::to_string(&Message::Response {
            id,
            outcome: Outcome::error(ErrorCode::InvalidRequest, why),
        })
        .map_or(FromBrowser::Ignore, FromBrowser::Reply),
        None => FromBrowser::Ignore,
    };
    match serde_json::from_value::<Message>(value) {
        Ok(Message::Request {
            request: Request::Hello(_),
            ..
        }) => refuse("mk web already greeted the daemon".into()),
        Ok(Message::Request {
            request: Request::SessionResize(_),
            ..
        }) => refuse("the web client never resizes a session".into()),
        Ok(message @ Message::Request { .. }) => encode(&Frame::Message(message))
            .map_or_else(|e| refuse(e.to_string()), FromBrowser::Forward),
        Ok(_) => refuse("only requests are accepted".into()),
        Err(e) => refuse(format!("invalid request: {e}")),
    }
}

pub fn browser_message(frame: Frame) -> serde_json::Result<WsMessage> {
    Ok(match frame {
        Frame::Output { session_id, data } => {
            let mut bytes = session_id.to_string().into_bytes();
            bytes.extend_from_slice(&data);
            WsMessage::binary(bytes)
        }
        Frame::Message(message) => WsMessage::text(json_with_ansi(&message)?),
    })
}

/// The message as JSON; a response holding a `snapshot` (e.g. `session.attach`) and a
/// `session.snapshot` event also get `ansi`: the escape sequences that draw it.
pub fn json_with_ansi(message: &Message) -> serde_json::Result<String> {
    let mut value = serde_json::to_value(message)?;
    match message {
        Message::Response {
            outcome: Outcome::Result(result),
            ..
        } => {
            if let Some(snapshot) = result
                .get("snapshot")
                .and_then(|s| ScreenSnapshot::deserialize(s).ok())
            {
                value["result"]["ansi"] = paint(&snapshot).into();
            }
        }
        Message::Event(Event::SessionSnapshot(event)) => {
            // The history is already in the page's scrollback from the attach.
            let snapshot = ScreenSnapshot {
                history: Vec::new(),
                ..event.snapshot.clone()
            };
            value["data"]["ansi"] = paint(&snapshot).into();
        }
        _ => {}
    }
    serde_json::to_string(&value)
}

fn close(reason: &str) -> WsMessage {
    // A close reason must fit in 123 bytes.
    let mut reason = reason.to_owned();
    while reason.len() > 120 {
        reason.pop();
    }
    WsMessage::Close(Some(CloseFrame {
        code: CloseCode::Error,
        reason: reason.into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{
        CellRun, CursorSnapshot, SessionAttachResult, SessionListParams, SessionSnapshotEvent,
        SessionState, SessionStateEvent, StateSource, TermColor, TermModes,
    };
    use ulid::Ulid;

    fn snapshot(line: &str, history: &str) -> ScreenSnapshot {
        let run = |text: &str| {
            vec![CellRun {
                text: text.into(),
                fg: TermColor::Default,
                bg: TermColor::Default,
                flags: 0,
            }]
        };
        ScreenSnapshot {
            cols: 80,
            rows: 24,
            lines: vec![run(line)],
            history: vec![run(history)],
            cursor: CursorSnapshot {
                row: 0,
                col: 0,
                visible: true,
            },
            modes: TermModes::default(),
        }
    }

    fn reply(answer: FromBrowser) -> serde_json::Value {
        match answer {
            FromBrowser::Reply(text) => serde_json::from_str(&text).unwrap(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn forwards_requests_as_frames() {
        let text = r#"{"type":"request","id":3,"method":"session.list","params":{}}"#;
        let FromBrowser::Forward(bytes) = daemon_request(text) else {
            panic!("not forwarded");
        };
        let mut decoder = FrameDecoder::new();
        decoder.push(&bytes);
        assert_eq!(
            decoder.next_frame().unwrap(),
            Some(Frame::Message(Message::Request {
                id: 3,
                request: Request::SessionList(SessionListParams::default()),
            }))
        );
    }

    #[test]
    fn refuses_hello_resize_and_unknown_methods() {
        let hello = r#"{"type":"request","id":1,"method":"hello","params":{"client":"ui","protocol_version":1}}"#;
        let resize = format!(
            r#"{{"type":"request","id":2,"method":"session.resize","params":{{"session_id":"{}","cols":40,"rows":20}}}}"#,
            Ulid::new()
        );
        let unknown = r#"{"type":"request","id":3,"method":"no.such"}"#;
        for (text, id) in [(hello.to_owned(), 1), (resize, 2), (unknown.to_owned(), 3)] {
            let value = reply(daemon_request(&text));
            assert_eq!(value["id"], id, "{value}");
            assert_eq!(value["error"]["code"], "invalid_request", "{value}");
        }
        assert_eq!(daemon_request("not json"), FromBrowser::Ignore);
        assert_eq!(
            daemon_request(r#"{"method":"no.such"}"#),
            FromBrowser::Ignore
        );
    }

    #[test]
    fn attach_result_gets_the_painted_screen() {
        let result = SessionAttachResult {
            state: SessionState::NeedsInput,
            snapshot: snapshot("Apply? (y/n)", "older line"),
        };
        let message = Message::Response {
            id: 9,
            outcome: Outcome::ok(&result).unwrap(),
        };
        let value: serde_json::Value =
            serde_json::from_str(&json_with_ansi(&message).unwrap()).unwrap();
        let ansi = value["result"]["ansi"].as_str().unwrap();
        assert!(
            ansi.contains("Apply? (y/n)") && ansi.contains("older line"),
            "{ansi}"
        );
        assert_eq!(value["result"]["state"], "needs_input");
    }

    #[test]
    fn snapshot_event_is_painted_without_history() {
        let event = Event::SessionSnapshot(SessionSnapshotEvent {
            session_id: Ulid::new(),
            snapshot: snapshot("now", "already scrolled"),
        });
        let value: serde_json::Value =
            serde_json::from_str(&json_with_ansi(&Message::Event(event)).unwrap()).unwrap();
        let ansi = value["data"]["ansi"].as_str().unwrap();
        assert!(
            ansi.contains("now") && !ansi.contains("already scrolled"),
            "{ansi}"
        );
    }

    #[test]
    fn other_messages_pass_unchanged() {
        let event = Message::Event(Event::SessionState(SessionStateEvent {
            session_id: Ulid::new(),
            state: SessionState::Working,
            source: StateSource::Hook,
            changed_at: 5,
        }));
        let value: serde_json::Value =
            serde_json::from_str(&json_with_ansi(&event).unwrap()).unwrap();
        assert_eq!(value, serde_json::to_value(&event).unwrap());
    }

    #[test]
    fn output_goes_out_binary_behind_the_session_id() {
        let session_id = Ulid::new();
        let message = browser_message(Frame::Output {
            session_id,
            data: b"hi".to_vec(),
        })
        .unwrap();
        let bytes = message.into_data();
        assert_eq!(&bytes[..26], session_id.to_string().as_bytes());
        assert_eq!(&bytes[26..], b"hi");
    }
}
