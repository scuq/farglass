//! The link from the agent to farglassd.

use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::time::Duration;

use farglass_proto::codec::{CodecError, FrameCodec};
use farglass_proto::message::{Close, CloseReason, LinkMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameError, FrameType, PROTOCOL_VERSION, Side};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

/// farglassd must send its hello message in this time.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// An error that stops the link.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    /// The link failed.
    #[error("the agent link failed: {0}")]
    Io(#[from] io::Error),
    /// farglassd sent a frame that is not valid.
    #[error("farglassd sent a frame that is not valid: {0}")]
    Frame(#[from] FrameError),
    /// farglassd broke the protocol.
    #[error("farglassd broke the protocol: {0}")]
    Protocol(String),
}

impl From<CodecError> for LinkError {
    fn from(error: CodecError) -> Self {
        match error {
            CodecError::Frame(error) => Self::Frame(error),
            CodecError::Io(error) => Self::Io(error),
        }
    }
}

/// Connects to the agent socket at `path` and serves the link until farglassd closes it.
///
/// # Errors
///
/// [`LinkError`] if the connection fails or farglassd breaks the protocol.
pub async fn connect(path: &Path) -> Result<(), LinkError> {
    let stream = UnixStream::connect(path).await?;
    serve(stream).await
}

/// Serves the agent link on `stream` until farglassd closes it.
///
/// # Errors
///
/// [`LinkError`] if the stream fails or farglassd breaks the protocol.
pub async fn serve<S>(stream: S) -> Result<(), LinkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut link = Framed::new(stream, FrameCodec);
    let hello = LinkMessage::Hello {
        version: PROTOCOL_VERSION,
    };
    link.send(Frame::json(
        ChannelId::CONNECTION,
        FrameType::Control,
        &hello,
    )?)
    .await?;

    let first = tokio::time::timeout(HELLO_TIMEOUT, link.next())
        .await
        .map_err(|_| LinkError::Protocol("no hello message".to_owned()))?;
    match first {
        Some(frame) => match link_message(&frame?)? {
            LinkMessage::Hello { version } if version == PROTOCOL_VERSION => {}
            LinkMessage::Hello { version } => {
                return Err(LinkError::Protocol(format!(
                    "farglassd has protocol version {version}, the agent has {PROTOCOL_VERSION}"
                )));
            }
            other => {
                return Err(LinkError::Protocol(format!(
                    "the first message was {other:?}, not a hello message"
                )));
            }
        },
        None => return Ok(()),
    }
    tracing::info!("connected to farglassd");

    // The channels that the agent refused and that wait for the close frame of farglassd.
    let mut refused: HashSet<ChannelId> = HashSet::new();
    while let Some(frame) = link.next().await {
        let frame = frame?;
        let channel = frame.channel();
        if channel.is_connection() {
            match link_message(&frame)? {
                LinkMessage::Attach { connection } => {
                    tracing::info!(connection, "a browser connection attached");
                }
                LinkMessage::Detach { connection } => {
                    tracing::info!(connection, "a browser connection detached");
                }
                LinkMessage::Hello { .. } => {
                    return Err(LinkError::Protocol("a second hello message".to_owned()));
                }
            }
            continue;
        }

        match frame.frame_type() {
            FrameType::Open => {
                if channel.opener() != Some(Side::Shell) || !refused.insert(channel) {
                    return Err(LinkError::Protocol(format!(
                        "farglassd cannot open channel {channel}"
                    )));
                }
                // This version of the agent has no channel kinds. F005 adds the echo channel.
                let message = match frame.parse::<Open>() {
                    Ok(open) => format!("The agent does not support the channel kind {open:?}."),
                    Err(_) => "The agent does not know this channel kind.".to_owned(),
                };
                let close = Close {
                    reason: CloseReason::NotSupported,
                    message: Some(message),
                };
                link.send(Frame::close(channel, &close)?).await?;
            }
            // The answer of farglassd to the refusal. The channel number is free again.
            FrameType::Close => {
                refused.remove(&channel);
            }
            // Frames that the opener sent before it got the refusal.
            FrameType::Ready | FrameType::Data | FrameType::Control => {}
        }
    }
    Ok(())
}

fn link_message(frame: &Frame) -> Result<LinkMessage, LinkError> {
    frame
        .parse()
        .map_err(|error| LinkError::Protocol(format!("a link message is not valid: {error}")))
}
