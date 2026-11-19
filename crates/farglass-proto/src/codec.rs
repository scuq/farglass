//! A tokio-util codec for frames on a byte stream, for example a Unix socket.

use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use crate::{Frame, FrameError};

/// Encodes and decodes frames on a byte stream.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameCodec;

/// An error of the frame codec: a protocol error or an I/O error.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// The peer sent a frame that is not valid. The stream cannot continue.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// The stream failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = CodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, CodecError> {
        Ok(Frame::decode(src)?)
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = CodecError;

    fn encode(&mut self, frame: Frame, dst: &mut BytesMut) -> Result<(), CodecError> {
        frame.encode(dst);
        Ok(())
    }
}
