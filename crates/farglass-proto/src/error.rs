//! Errors of the frame codec.

use crate::frame::{FrameType, MAX_PAYLOAD_LEN};

/// An error from the encode or the decode of a frame.
///
/// After a decode error, the position in the byte stream is unknown.
/// The receiver must close the connection.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FrameError {
    /// The header has a frame type that this version does not know.
    #[error("the frame type {0} is unknown")]
    UnknownFrameType(u8),

    /// The payload is larger than [`MAX_PAYLOAD_LEN`].
    #[error("the payload length {len} is larger than the limit {MAX_PAYLOAD_LEN}")]
    PayloadTooLarge {
        /// The length of the payload in bytes.
        len: usize,
    },

    /// The connection channel got a frame that is not a control frame.
    #[error("a {0} frame is not valid on the connection channel")]
    InvalidConnectionFrame(FrameType),

    /// The message stops before the end of the frame.
    #[error("the message has {actual} bytes, but the frame has {expected}")]
    Truncated {
        /// The length of the frame in bytes, or of the header if the header is not complete.
        expected: usize,
        /// The length of the message in bytes.
        actual: usize,
    },

    /// The message has more bytes after the frame.
    #[error("the message has {extra} bytes after the frame")]
    TrailingBytes {
        /// The number of bytes after the frame.
        extra: usize,
    },

    /// The JSON payload has a syntax error or does not match the message type.
    #[error("the JSON payload is not valid: {0}")]
    Json(#[from] serde_json::Error),
}

impl FrameError {
    /// The codes of all errors, in the sequence of the variants.
    ///
    /// The TypeScript side uses the same codes.
    pub const CODES: [&'static str; 6] = [
        "unknown-frame-type",
        "payload-too-large",
        "invalid-connection-frame",
        "truncated",
        "trailing-bytes",
        "invalid-json",
    ];

    /// Returns the code of the error. The code does not change between versions.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownFrameType(_) => Self::CODES[0],
            Self::PayloadTooLarge { .. } => Self::CODES[1],
            Self::InvalidConnectionFrame(_) => Self::CODES[2],
            Self::Truncated { .. } => Self::CODES[3],
            Self::TrailingBytes { .. } => Self::CODES[4],
            Self::Json(_) => Self::CODES[5],
        }
    }
}
