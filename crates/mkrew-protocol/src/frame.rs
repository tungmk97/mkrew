//! Frame codec (SPEC §7 "Framing").
//!
//! Every frame is `u32` big-endian length + `u8` kind + payload, where the length counts the
//! kind byte plus the payload. Kind `0x01` carries one JSON [`Message`]; kind `0x02` carries
//! terminal output: the session's 16-byte binary ULID followed by raw PTY bytes.

use ulid::Ulid;

use crate::{Message, ProtocolError};

/// Kind byte of a JSON message frame.
pub const KIND_JSON: u8 = 0x01;
/// Kind byte of a binary terminal output frame.
pub const KIND_OUTPUT: u8 = 0x02;
/// Upper bound for the length field, so a corrupt prefix cannot make us allocate gigabytes.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

const HEADER_LEN: usize = 4;
const ULID_LEN: usize = 16;

/// One decoded frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// A request, response or event.
    Message(Message),
    /// Raw PTY bytes of one session (`session.output`).
    Output { session_id: Ulid, data: Vec<u8> },
}

/// Encodes a frame including its length prefix.
pub fn encode(frame: &Frame) -> Result<Vec<u8>, ProtocolError> {
    // Build header + payload in one buffer; the length prefix is patched in at the end.
    let mut out = match frame {
        Frame::Message(message) => {
            let mut out = vec![0, 0, 0, 0, KIND_JSON];
            serde_json::to_writer(&mut out, message)?;
            out
        }
        Frame::Output { session_id, data } => {
            let mut out = Vec::with_capacity(HEADER_LEN + 1 + ULID_LEN + data.len());
            out.extend_from_slice(&[0, 0, 0, 0, KIND_OUTPUT]);
            out.extend_from_slice(&session_id.to_bytes());
            out.extend_from_slice(data);
            out
        }
    };
    let len = out.len() - HEADER_LEN;
    if len > MAX_FRAME_LEN {
        return Err(ProtocolError::FrameTooLarge(len));
    }
    // `len <= MAX_FRAME_LEN` (16 MiB) was checked above, so it fits in a u32.
    out[..HEADER_LEN].copy_from_slice(&(len as u32).to_be_bytes());
    Ok(out)
}

/// Incremental decoder: feed it whatever the transport read, then drain complete frames.
///
/// Partial frames stay buffered until the rest arrives. Consumed bytes are only tracked by an
/// offset and compacted on the next [`push`](Self::push), so draining many small frames from
/// one read does not shift the buffer once per frame.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    start: usize,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes read from the transport.
    pub fn push(&mut self, bytes: &[u8]) {
        if self.start > 0 {
            self.buf.drain(..self.start);
            self.start = 0;
        }
        self.buf.extend_from_slice(bytes);
    }

    /// Number of buffered bytes not yet returned as a frame.
    pub fn buffered(&self) -> usize {
        self.buf.len() - self.start
    }

    /// Returns the next complete frame, or `None` if more bytes are needed.
    ///
    /// After an error the stream is out of sync; the caller should drop the connection.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, ProtocolError> {
        let pending = &self.buf[self.start..];
        let Some(header) = pending.first_chunk::<HEADER_LEN>() else {
            return Ok(None);
        };
        let len = u32::from_be_bytes(*header) as usize;
        if len > MAX_FRAME_LEN {
            return Err(ProtocolError::FrameTooLarge(len));
        }
        if len == 0 {
            return Err(ProtocolError::EmptyFrame);
        }
        let Some(body) = pending.get(HEADER_LEN..HEADER_LEN + len) else {
            return Ok(None);
        };
        self.start += HEADER_LEN + len;
        let (kind, payload) = (body[0], &body[1..]);
        let frame = match kind {
            KIND_JSON => Frame::Message(serde_json::from_slice(payload)?),
            KIND_OUTPUT => {
                let Some((id, data)) = payload.split_first_chunk::<ULID_LEN>() else {
                    return Err(ProtocolError::TruncatedOutput(payload.len()));
                };
                Frame::Output {
                    session_id: Ulid::from_bytes(*id),
                    data: data.to_vec(),
                }
            }
            other => return Err(ProtocolError::UnknownKind(other)),
        };
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientKind, Event, Request, SessionState};

    fn hello() -> Frame {
        Frame::Message(Message::Request {
            id: 1,
            request: Request::Hello(crate::HelloParams {
                client: ClientKind::Cli,
                protocol_version: crate::PROTOCOL_VERSION,
            }),
        })
    }

    fn output() -> Frame {
        Frame::Output {
            session_id: Ulid::from_parts(1_700_000_000_000, 42),
            data: b"\x1b[31mred\x1b[0m\r\n\x00\xff".to_vec(),
        }
    }

    fn decode_all(bytes: &[u8]) -> Vec<Frame> {
        let mut decoder = FrameDecoder::new();
        decoder.push(bytes);
        let mut frames = Vec::new();
        while let Some(frame) = decoder.next_frame().unwrap() {
            frames.push(frame);
        }
        assert_eq!(decoder.buffered(), 0);
        frames
    }

    #[test]
    fn json_frame_layout() {
        let bytes = encode(&hello()).unwrap();
        let len = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert_eq!(len, bytes.len() - 4);
        assert_eq!(bytes[4], KIND_JSON);
        assert!(bytes[5..].starts_with(b"{"));
    }

    #[test]
    fn output_frame_layout_is_raw_bytes() {
        let frame = output();
        let bytes = encode(&frame).unwrap();
        assert_eq!(bytes[4], KIND_OUTPUT);
        let Frame::Output { session_id, data } = &frame else {
            unreachable!()
        };
        assert_eq!(&bytes[5..21], &session_id.to_bytes());
        assert_eq!(&bytes[21..], data.as_slice());
    }

    #[test]
    fn round_trip_mixed_frames() {
        let frames = vec![
            hello(),
            output(),
            Frame::Message(Message::Event(Event::SessionState(
                crate::SessionStateEvent {
                    session_id: Ulid::from_parts(5, 6),
                    state: SessionState::NeedsInput,
                    source: crate::StateSource::Screen,
                    changed_at: 1,
                },
            ))),
            Frame::Output {
                session_id: Ulid::from_parts(7, 8),
                data: Vec::new(),
            },
        ];
        let mut bytes = Vec::new();
        for frame in &frames {
            bytes.extend(encode(frame).unwrap());
        }
        assert_eq!(decode_all(&bytes), frames);
    }

    #[test]
    fn partial_reads_byte_by_byte() {
        let frames = vec![output(), hello(), output()];
        let mut bytes = Vec::new();
        for frame in &frames {
            bytes.extend(encode(frame).unwrap());
        }
        let mut decoder = FrameDecoder::new();
        let mut decoded = Vec::new();
        for byte in &bytes {
            decoder.push(std::slice::from_ref(byte));
            while let Some(frame) = decoder.next_frame().unwrap() {
                decoded.push(frame);
            }
        }
        assert_eq!(decoded, frames);
        assert_eq!(decoder.buffered(), 0);
    }

    #[test]
    fn partial_header_and_body_wait_for_more() {
        let bytes = encode(&output()).unwrap();
        let mut decoder = FrameDecoder::new();
        decoder.push(&bytes[..3]);
        assert!(decoder.next_frame().unwrap().is_none());
        decoder.push(&bytes[3..10]);
        assert!(decoder.next_frame().unwrap().is_none());
        decoder.push(&bytes[10..]);
        assert_eq!(decoder.next_frame().unwrap(), Some(output()));
        assert!(decoder.next_frame().unwrap().is_none());
    }

    #[test]
    fn rejects_oversized_length_before_buffering_body() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&(MAX_FRAME_LEN as u32 + 1).to_be_bytes());
        assert!(matches!(
            decoder.next_frame(),
            Err(ProtocolError::FrameTooLarge(_))
        ));
    }

    #[test]
    fn encode_rejects_oversized_output() {
        let frame = Frame::Output {
            session_id: Ulid::nil(),
            data: vec![0; MAX_FRAME_LEN],
        };
        assert!(matches!(
            encode(&frame),
            Err(ProtocolError::FrameTooLarge(_))
        ));
    }

    #[test]
    fn rejects_unknown_kind_empty_and_truncated_output() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&[0, 0, 0, 1, 0x7f]);
        assert!(matches!(
            decoder.next_frame(),
            Err(ProtocolError::UnknownKind(0x7f))
        ));

        let mut decoder = FrameDecoder::new();
        decoder.push(&[0, 0, 0, 0]);
        assert!(matches!(
            decoder.next_frame(),
            Err(ProtocolError::EmptyFrame)
        ));

        let mut decoder = FrameDecoder::new();
        decoder.push(&[0, 0, 0, 4, KIND_OUTPUT, 1, 2, 3]);
        assert!(matches!(
            decoder.next_frame(),
            Err(ProtocolError::TruncatedOutput(3))
        ));
    }

    #[test]
    fn rejects_invalid_json() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&[0, 0, 0, 3, KIND_JSON, b'{', b'}']);
        assert!(matches!(decoder.next_frame(), Err(ProtocolError::Json(_))));
    }
}
