//! Frames: the binary unit of the channel protocol.

use std::fmt;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Serialize, de::DeserializeOwned};

use crate::channel::ChannelId;
use crate::error::FrameError;
use crate::message::{Close, ConnectionMessage, Open, Ready};

/// The length of a frame header in bytes.
pub const HEADER_LEN: usize = 8;

/// The largest payload of one frame in bytes (1 MiB).
///
/// The limit keeps the buffer of a receiver small, also before login.
/// A sender divides larger data into more than one data frame.
pub const MAX_PAYLOAD_LEN: usize = 1 << 20;

/// The type of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FrameType {
    /// Opens the channel. The payload is [`Open`] as JSON.
    Open = 1,
    /// Accepts the channel. The payload is [`Ready`] as JSON.
    Ready = 2,
    /// Carries raw bytes of the channel.
    Data = 3,
    /// Carries one JSON message. The channel kind defines the message.
    Control = 4,
    /// Closes the channel. The payload is [`Close`] as JSON.
    Close = 5,
}

impl FrameType {
    /// All frame types, in the sequence of their numbers.
    pub const ALL: [Self; 5] = [
        Self::Open,
        Self::Ready,
        Self::Data,
        Self::Control,
        Self::Close,
    ];

    /// Returns the name of the frame type, as the TypeScript side writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Ready => "Ready",
            Self::Data => "Data",
            Self::Control => "Control",
            Self::Close => "Close",
        }
    }
}

impl TryFrom<u8> for FrameType {
    type Error = FrameError;

    fn try_from(raw: u8) -> Result<Self, FrameError> {
        Self::ALL
            .into_iter()
            .find(|frame_type| *frame_type as u8 == raw)
            .ok_or(FrameError::UnknownFrameType(raw))
    }
}

impl fmt::Display for FrameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The header of a frame: channel, type, and payload length.
///
/// A header is always valid.
/// [`FrameHeader::new`] and [`FrameHeader::decode`] refuse all other values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    channel: ChannelId,
    frame_type: FrameType,
    payload_len: u32,
}

impl FrameHeader {
    /// Returns a header for a payload of `payload_len` bytes.
    ///
    /// # Errors
    ///
    /// - [`FrameError::PayloadTooLarge`] if `payload_len` is more than [`MAX_PAYLOAD_LEN`].
    /// - [`FrameError::InvalidConnectionFrame`] if `channel` is the connection channel
    ///   and `frame_type` is not [`FrameType::Control`].
    pub fn new(
        channel: ChannelId,
        frame_type: FrameType,
        payload_len: usize,
    ) -> Result<Self, FrameError> {
        if payload_len > MAX_PAYLOAD_LEN {
            return Err(FrameError::PayloadTooLarge { len: payload_len });
        }
        if channel.is_connection() && frame_type != FrameType::Control {
            return Err(FrameError::InvalidConnectionFrame(frame_type));
        }
        Ok(Self {
            channel,
            frame_type,
            // MAX_PAYLOAD_LEN is less than 2^24, thus the value fits.
            payload_len: payload_len as u32,
        })
    }

    /// Returns the channel of the frame.
    #[must_use]
    pub const fn channel(self) -> ChannelId {
        self.channel
    }

    /// Returns the type of the frame.
    #[must_use]
    pub const fn frame_type(self) -> FrameType {
        self.frame_type
    }

    /// Returns the length of the payload in bytes.
    #[must_use]
    pub const fn payload_len(self) -> usize {
        self.payload_len as usize
    }

    /// Returns the header as bytes.
    ///
    /// Bytes 0 to 3 are the channel, byte 4 is the type, and bytes 5 to 7 are the length.
    /// The numbers are little-endian.
    #[must_use]
    pub fn encode(self) -> [u8; HEADER_LEN] {
        let type_and_len = (self.payload_len << 8) | self.frame_type as u32;
        let mut out = [0; HEADER_LEN];
        out[..4].copy_from_slice(&self.channel.get().to_le_bytes());
        out[4..].copy_from_slice(&type_and_len.to_le_bytes());
        out
    }

    /// Decodes a header.
    ///
    /// # Errors
    ///
    /// - [`FrameError::UnknownFrameType`] if the frame type is unknown.
    /// - The errors of [`FrameHeader::new`].
    pub fn decode(bytes: [u8; HEADER_LEN]) -> Result<Self, FrameError> {
        let [c0, c1, c2, c3, frame_type, l0, l1, l2] = bytes;
        let channel = ChannelId::new(u32::from_le_bytes([c0, c1, c2, c3]));
        let frame_type = FrameType::try_from(frame_type)?;
        let payload_len = u32::from_le_bytes([l0, l1, l2, 0]);
        Self::new(channel, frame_type, payload_len as usize)
    }
}

/// One frame: a header and a payload.
///
/// A clone of a frame does not copy the payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    header: FrameHeader,
    payload: Bytes,
}

impl Frame {
    /// Returns a frame with the payload `payload`.
    ///
    /// # Errors
    ///
    /// The errors of [`FrameHeader::new`].
    pub fn new(
        channel: ChannelId,
        frame_type: FrameType,
        payload: impl Into<Bytes>,
    ) -> Result<Self, FrameError> {
        let payload = payload.into();
        let header = FrameHeader::new(channel, frame_type, payload.len())?;
        Ok(Self { header, payload })
    }

    /// Returns a frame with `message` as the JSON payload.
    ///
    /// # Errors
    ///
    /// - [`FrameError::Json`] if `message` does not serialize.
    /// - The errors of [`FrameHeader::new`].
    pub fn json<T: Serialize + ?Sized>(
        channel: ChannelId,
        frame_type: FrameType,
        message: &T,
    ) -> Result<Self, FrameError> {
        Self::new(channel, frame_type, serde_json::to_vec(message)?)
    }

    /// Returns an open frame for `channel`.
    ///
    /// # Errors
    ///
    /// The errors of [`Frame::json`].
    pub fn open(channel: ChannelId, open: &Open) -> Result<Self, FrameError> {
        Self::json(channel, FrameType::Open, open)
    }

    /// Returns a ready frame for `channel`.
    ///
    /// # Errors
    ///
    /// The errors of [`Frame::json`].
    pub fn ready(channel: ChannelId, ready: &Ready) -> Result<Self, FrameError> {
        Self::json(channel, FrameType::Ready, ready)
    }

    /// Returns a data frame for `channel`.
    ///
    /// # Errors
    ///
    /// The errors of [`FrameHeader::new`].
    pub fn data(channel: ChannelId, payload: impl Into<Bytes>) -> Result<Self, FrameError> {
        Self::new(channel, FrameType::Data, payload)
    }

    /// Returns a close frame for `channel`.
    ///
    /// # Errors
    ///
    /// The errors of [`Frame::json`].
    pub fn close(channel: ChannelId, close: &Close) -> Result<Self, FrameError> {
        Self::json(channel, FrameType::Close, close)
    }

    /// Returns a frame for the connection channel.
    ///
    /// # Errors
    ///
    /// The errors of [`Frame::json`].
    pub fn connection(message: &ConnectionMessage) -> Result<Self, FrameError> {
        Self::json(ChannelId::CONNECTION, FrameType::Control, message)
    }

    /// Returns the same frame on the channel `channel`. The payload is not copied.
    ///
    /// farglassd uses this function to give the frames of a browser connection
    /// other channel numbers on the agent link.
    ///
    /// # Errors
    ///
    /// The errors of [`FrameHeader::new`].
    pub fn with_channel(&self, channel: ChannelId) -> Result<Self, FrameError> {
        let header = FrameHeader::new(channel, self.frame_type(), self.payload.len())?;
        Ok(Self {
            header,
            payload: self.payload.clone(),
        })
    }

    /// Returns the header of the frame.
    #[must_use]
    pub const fn header(&self) -> FrameHeader {
        self.header
    }

    /// Returns the channel of the frame.
    #[must_use]
    pub const fn channel(&self) -> ChannelId {
        self.header.channel()
    }

    /// Returns the type of the frame.
    #[must_use]
    pub const fn frame_type(&self) -> FrameType {
        self.header.frame_type()
    }

    /// Returns the payload of the frame.
    #[must_use]
    pub const fn payload(&self) -> &Bytes {
        &self.payload
    }

    /// Returns the payload and drops the header.
    #[must_use]
    pub fn into_payload(self) -> Bytes {
        self.payload
    }

    /// Parses the JSON payload as `T`.
    ///
    /// # Errors
    ///
    /// [`FrameError::Json`] if the payload is not valid JSON for `T`.
    pub fn parse<T: DeserializeOwned>(&self) -> Result<T, FrameError> {
        Ok(serde_json::from_slice(&self.payload)?)
    }

    /// Returns the length of the encoded frame in bytes.
    #[must_use]
    pub const fn encoded_len(&self) -> usize {
        HEADER_LEN + self.payload.len()
    }

    /// Appends the encoded frame to `dst`.
    ///
    /// To send a large payload with no copy, write [`Frame::header`] and
    /// [`Frame::payload`] to the transport as two buffers.
    pub fn encode(&self, dst: &mut BytesMut) {
        dst.reserve(self.encoded_len());
        dst.put_slice(&self.header.encode());
        dst.put_slice(&self.payload);
    }

    /// Returns the encoded frame.
    #[must_use]
    pub fn to_bytes(&self) -> Bytes {
        let mut dst = BytesMut::with_capacity(self.encoded_len());
        self.encode(&mut dst);
        dst.freeze()
    }

    /// Decodes one frame from the start of a byte stream.
    ///
    /// Use this function on a transport that does not keep message boundaries,
    /// for example a Unix socket.
    /// If `src` holds a full frame, the function removes it from `src` and returns it.
    /// If `src` holds less than one frame, the function returns `None` and
    /// reserves space for the remaining bytes.
    /// The payload shares the memory of `src`.
    ///
    /// # Errors
    ///
    /// The errors of [`FrameHeader::decode`].
    /// After an error, `src` is not changed, and the stream cannot continue.
    pub fn decode(src: &mut BytesMut) -> Result<Option<Self>, FrameError> {
        let Some(head) = src.first_chunk::<HEADER_LEN>() else {
            return Ok(None);
        };
        let header = FrameHeader::decode(*head)?;
        let frame_len = HEADER_LEN + header.payload_len();
        if src.len() < frame_len {
            src.reserve(frame_len - src.len());
            return Ok(None);
        }
        src.advance(HEADER_LEN);
        let payload = src.split_to(header.payload_len()).freeze();
        Ok(Some(Self { header, payload }))
    }

    /// Decodes a message that holds one full frame.
    ///
    /// Use this function on a transport that keeps message boundaries,
    /// for example WebSocket.
    /// The payload shares the memory of `message`.
    ///
    /// # Errors
    ///
    /// - [`FrameError::Truncated`] if `message` is shorter than the frame.
    /// - [`FrameError::TrailingBytes`] if `message` is longer than the frame.
    /// - The errors of [`FrameHeader::decode`].
    pub fn from_message(message: Bytes) -> Result<Self, FrameError> {
        let Some(head) = message.first_chunk::<HEADER_LEN>() else {
            return Err(FrameError::Truncated {
                expected: HEADER_LEN,
                actual: message.len(),
            });
        };
        let header = FrameHeader::decode(*head)?;
        let frame_len = HEADER_LEN + header.payload_len();
        if message.len() < frame_len {
            return Err(FrameError::Truncated {
                expected: frame_len,
                actual: message.len(),
            });
        }
        if message.len() > frame_len {
            return Err(FrameError::TrailingBytes {
                extra: message.len() - frame_len,
            });
        }
        Ok(Self {
            header,
            payload: message.slice(HEADER_LEN..),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::CloseReason;

    #[test]
    fn header_layout_is_channel_type_length_little_endian() {
        let header =
            FrameHeader::new(ChannelId::new(0x0403_0201), FrameType::Data, 0x0A_0908).unwrap();
        assert_eq!(header.encode(), [1, 2, 3, 4, 3, 8, 9, 10]);
    }

    #[test]
    fn largest_payload_makes_a_full_cycle() {
        let frame = Frame::data(ChannelId::new(1), vec![0xAB; MAX_PAYLOAD_LEN]).unwrap();
        let decoded = Frame::from_message(frame.to_bytes()).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn payload_above_the_limit_is_refused() {
        let error = Frame::data(ChannelId::new(1), vec![0; MAX_PAYLOAD_LEN + 1]).unwrap_err();
        assert_eq!(error.code(), "payload-too-large");
    }

    #[test]
    fn connection_channel_accepts_only_control_frames() {
        for frame_type in FrameType::ALL {
            let result = Frame::new(ChannelId::CONNECTION, frame_type, Bytes::new());
            assert_eq!(result.is_ok(), frame_type == FrameType::Control);
        }
    }

    #[test]
    fn stream_decode_waits_for_a_full_frame() {
        let close = Close {
            reason: CloseReason::Done,
            message: None,
        };
        let frame = Frame::close(ChannelId::new(7), &close).unwrap();
        let bytes = frame.to_bytes();

        let mut src = BytesMut::new();
        for byte in &bytes[..bytes.len() - 1] {
            src.put_u8(*byte);
            assert_eq!(Frame::decode(&mut src).unwrap(), None);
        }
        src.put_u8(bytes[bytes.len() - 1]);
        assert_eq!(Frame::decode(&mut src).unwrap(), Some(frame));
        assert!(src.is_empty());
    }

    #[test]
    fn stream_decode_refuses_a_large_length_before_the_payload_arrives() {
        let mut src = BytesMut::from(&[1, 0, 0, 0, 3, 0xFF, 0xFF, 0xFF][..]);
        let error = Frame::decode(&mut src).unwrap_err();
        assert_eq!(error.code(), "payload-too-large");
        assert_eq!(src.len(), HEADER_LEN);
    }

    #[test]
    fn decode_does_not_copy_the_payload() {
        let message = Frame::data(ChannelId::new(1), &b"payload"[..])
            .unwrap()
            .to_bytes();
        let frame = Frame::from_message(message.clone()).unwrap();
        assert_eq!(frame.payload().as_ptr(), message[HEADER_LEN..].as_ptr());
    }

    #[test]
    fn with_channel_keeps_the_type_and_the_payload() {
        let frame = Frame::data(ChannelId::new(1), &b"abc"[..]).unwrap();
        let moved = frame.with_channel(ChannelId::new(9)).unwrap();
        assert_eq!(moved.channel(), ChannelId::new(9));
        assert_eq!(moved.frame_type(), FrameType::Data);
        assert_eq!(moved.payload().as_ptr(), frame.payload().as_ptr());
        assert!(frame.with_channel(ChannelId::CONNECTION).is_err());
    }

    #[test]
    fn error_codes_are_unique() {
        let mut codes = FrameError::CODES.to_vec();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), FrameError::CODES.len());
    }
}
