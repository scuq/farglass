//! Channel protocol between the shell and the host programs.
//!
//! One connection carries numbered channels.
//! A [`Frame`] is an 8-byte header and a payload.
//! The payload of a [`FrameType::Data`] frame is raw bytes.
//! The payload of all other frames is JSON, with the types in [`message`].
//!
//! All frames of a channel go on that channel.
//! Thus a transport can put each channel on its own stream.
//!
//! `docs/protocol.md` is the reference for the wire format.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod channel;
mod error;
mod frame;
pub mod message;
pub mod typescript;

pub use channel::{ChannelId, ChannelIdAllocator, Side};
pub use error::FrameError;
pub use frame::{Frame, FrameHeader, FrameType, HEADER_LEN, MAX_PAYLOAD_LEN};

/// The version of the channel protocol.
///
/// The two sides send it in [`message::ConnectionMessage::Hello`].
pub const PROTOCOL_VERSION: u16 = 1;
