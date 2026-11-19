//! The link from the agent to farglassd.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::time::Duration;

use farglass_proto::codec::{CodecError, FrameCodec};
use farglass_proto::message::{
    Close, CloseReason, LinkMessage, Open, Ready, StateEvent, StateRequest,
};
use farglass_proto::{ChannelId, Frame, FrameError, FrameType, PROTOCOL_VERSION, Side};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

use crate::state::StateStore;

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
/// `store` is the state store of the session. It continues after the link stops.
///
/// # Errors
///
/// [`LinkError`] if the connection fails or farglassd breaks the protocol.
pub async fn connect(path: &Path, store: &mut StateStore) -> Result<(), LinkError> {
    let stream = UnixStream::connect(path).await?;
    serve_with(stream, store).await
}

/// Serves the agent link on `stream` with a new state store, until farglassd closes it.
///
/// # Errors
///
/// [`LinkError`] if the stream fails or farglassd breaks the protocol.
pub async fn serve<S>(stream: S) -> Result<(), LinkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    serve_with(stream, &mut StateStore::default()).await
}

/// Serves the agent link on `stream` with the state store `store`, until farglassd closes it.
///
/// # Errors
///
/// [`LinkError`] if the stream fails or farglassd breaks the protocol.
pub async fn serve_with<S>(stream: S, store: &mut StateStore) -> Result<(), LinkError>
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

    let mut channels: HashMap<ChannelId, Channel> = HashMap::new();
    // The state channels that asked for each key. They get each change of the key.
    let mut watchers: HashMap<String, HashSet<ChannelId>> = HashMap::new();
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

        if frame.frame_type() == FrameType::Open {
            if channel.opener() != Some(Side::Shell) || channels.contains_key(&channel) {
                return Err(LinkError::Protocol(format!(
                    "farglassd cannot open channel {channel}"
                )));
            }
            let (state, answer) = open(channel, &frame)?;
            channels.insert(channel, state);
            link.send(answer).await?;
            continue;
        }

        let Some(state) = channels.get_mut(&channel) else {
            // A frame of a channel that is closed on the two sides.
            continue;
        };
        match frame.frame_type() {
            FrameType::Close => {
                // The agent answers a close frame of the shell with its own close frame.
                // If the agent closed the channel first, this is the answer.
                if !state.closed_by_agent {
                    link.send(Frame::close(channel, &Close::new(CloseReason::Done))?)
                        .await?;
                }
                channels.remove(&channel);
                watchers.retain(|_, ids| {
                    ids.remove(&channel);
                    !ids.is_empty()
                });
            }
            FrameType::Data | FrameType::Control if !state.closed_by_agent => match state.kind {
                Kind::Echo => link.send(frame).await?,
                Kind::State if frame.frame_type() == FrameType::Control => {
                    let refusal = match frame.parse::<StateRequest>() {
                        Ok(StateRequest::Get { key }) => {
                            let value = store.get(&key);
                            watchers.entry(key.clone()).or_default().insert(channel);
                            let event = StateEvent::Value { key, value };
                            link.send(Frame::json(channel, FrameType::Control, &event)?)
                                .await?;
                            None
                        }
                        Ok(StateRequest::Set { key, value }) => {
                            match store.set(&key, value.clone()) {
                                Ok(()) => {
                                    let event = StateEvent::Value {
                                        key: key.clone(),
                                        value,
                                    };
                                    let others = watchers
                                        .get(&key)
                                        .into_iter()
                                        .flatten()
                                        .filter(|id| **id != channel);
                                    for other in others {
                                        link.send(Frame::json(*other, FrameType::Control, &event)?)
                                            .await?;
                                    }
                                    None
                                }
                                Err(error) => Some((CloseReason::ResourceLimit, error.to_string())),
                            }
                        }
                        Err(error) => Some((
                            CloseReason::ProtocolError,
                            format!("The state request is not valid: {error}"),
                        )),
                    };
                    if let Some((reason, message)) = refusal {
                        state.closed_by_agent = true;
                        let close = Close {
                            reason,
                            message: Some(message),
                        };
                        link.send(Frame::close(channel, &close)?).await?;
                    }
                }
                Kind::State => {}
            },
            // Frames that the opener sent before it got the refusal, or a ready frame
            // that farglassd does not send on a channel of the shell.
            FrameType::Data | FrameType::Control | FrameType::Ready | FrameType::Open => {}
        }
    }
    Ok(())
}

/// The kind of an open channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Sends each data frame and each control frame back.
    Echo,
    /// Gives the values of the state store of the session.
    State,
}

/// One channel of the link.
#[derive(Debug)]
struct Channel {
    kind: Kind,
    /// Reports whether the agent sent a close frame.
    closed_by_agent: bool,
}

/// Accepts or refuses a new channel. Returns its state and the answer to the open frame.
fn open(channel: ChannelId, frame: &Frame) -> Result<(Channel, Frame), LinkError> {
    match frame.parse::<Open>() {
        Ok(Open::Echo) => {
            tracing::debug!(%channel, "opened an echo channel");
            let state = Channel {
                kind: Kind::Echo,
                closed_by_agent: false,
            };
            Ok((state, Frame::ready(channel, &Ready {})?))
        }
        Ok(Open::State) => {
            tracing::debug!(%channel, "opened a state channel");
            let state = Channel {
                kind: Kind::State,
                closed_by_agent: false,
            };
            Ok((state, Frame::ready(channel, &Ready {})?))
        }
        // farglassd serves the login channel itself. It never gives it to the agent.
        Ok(Open::Login) | Err(_) => {
            let close = Close {
                reason: CloseReason::NotSupported,
                message: Some("The agent does not know this channel kind.".to_owned()),
            };
            let state = Channel {
                kind: Kind::Echo,
                closed_by_agent: true,
            };
            Ok((state, Frame::close(channel, &close)?))
        }
    }
}

fn link_message(frame: &Frame) -> Result<LinkMessage, LinkError> {
    frame
        .parse()
        .map_err(|error| LinkError::Protocol(format!("a link message is not valid: {error}")))
}
