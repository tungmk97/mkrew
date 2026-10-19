//! Mkrew IPC protocol (SPEC §7): the frame codec shared by daemon, CLI and UI, and the
//! JSON message types carried in `0x01` frames.
//!
//! No async runtime here: the codec works on byte slices so any transport can drive it.

pub mod files;
pub mod frame;
pub mod message;
pub mod paint;

pub use files::*;
pub use frame::{Frame, FrameDecoder, KIND_JSON, KIND_OUTPUT, MAX_FRAME_LEN, encode};
pub use message::*;

/// Bumped on any incompatible change; `hello` with another value gets `version_mismatch`.
pub const PROTOCOL_VERSION: u32 = 45;

/// Errors from encoding or decoding frames.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("frame of {0} bytes exceeds the {MAX_FRAME_LEN} byte limit")]
    FrameTooLarge(usize),
    #[error("empty frame (no kind byte)")]
    EmptyFrame,
    #[error("unknown frame kind 0x{0:02x}")]
    UnknownKind(u8),
    #[error("output frame of {0} bytes is shorter than the 16-byte session id")]
    TruncatedOutput(usize),
    #[error("invalid JSON message: {0}")]
    Json(#[from] serde_json::Error),
}
