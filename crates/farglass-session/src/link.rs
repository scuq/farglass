//! The link from the agent to farglassd.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use farglass_proto::codec::{CodecError, FrameCodec};
use farglass_proto::message::{
    Bus, Close, CloseReason, DbusEvent, DbusRequest, LinkMessage, NotifyReply, Open, PolkitReply,
    PtyEvent, PtyRequest, Ready, StateEvent, StateRequest,
};
use farglass_proto::{ChannelId, Frame, FrameError, FrameType, PROTOCOL_VERSION, Side};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

use crate::dbus::{self, Buses, DbusOptions, Filter, Subscriptions};
use crate::logind::Logind;
use crate::notify::{self, Notifications};
use crate::polkit::{HELPER, Polkit, ViewerGuard};
use crate::state::StateStore;
use crate::terminal::{
    AttachError, Output, Sink, TerminalLink, TerminalOptions, Terminals, ViewerId,
};

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

/// What the agent keeps for the session while the link to farglassd breaks:
/// the state store and the terminals.
#[derive(Debug)]
pub struct Session {
    /// The state store.
    pub store: StateStore,
    /// The terminals.
    pub terminals: Terminals,
    /// The connections to the D-Bus buses.
    pub buses: Arc<Buses>,
    /// The polkit authentication agent.
    pub polkit: Arc<Polkit>,
    /// The desktop notifications.
    pub notifications: Arc<Notifications>,
    /// The logind session: its lock signals and its LockedHint.
    pub logind: Arc<Logind>,
}

impl Default for Session {
    fn default() -> Self {
        let buses = Buses::new(DbusOptions::default());
        Self {
            store: StateStore::default(),
            terminals: Terminals::new(TerminalOptions::from_env()),
            logind: Logind::new(buses.clone()),
            buses,
            polkit: Polkit::new(HELPER),
            notifications: Notifications::new(),
        }
    }
}

/// Connects to the agent socket at `path` and serves the link until farglassd closes it.
///
/// `session` continues after the link stops.
///
/// # Errors
///
/// [`LinkError`] if the connection fails or farglassd breaks the protocol.
pub async fn connect(path: &Path, session: &mut Session) -> Result<(), LinkError> {
    let stream = UnixStream::connect(path).await?;
    serve_with(stream, session).await
}

/// Serves the agent link on `stream` with a new session, until farglassd closes it.
///
/// # Errors
///
/// [`LinkError`] if the stream fails or farglassd breaks the protocol.
pub async fn serve<S>(stream: S) -> Result<(), LinkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    serve_with(stream, &mut Session::default()).await
}

/// Serves the agent link on `stream` with `session`, until farglassd closes it.
///
/// # Errors
///
/// [`LinkError`] if the stream fails or farglassd breaks the protocol.
pub async fn serve_with<S>(stream: S, session: &mut Session) -> Result<(), LinkError>
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

    // Dropping the channels at the end of the link detaches their terminals.
    let mut channels: HashMap<ChannelId, Channel> = HashMap::new();
    // The state channels that asked for each key. They get each change of the key.
    let mut watchers: HashMap<String, HashSet<ChannelId>> = HashMap::new();
    // The output of the terminals, with the channel of each attach.
    let (sink, mut output) = mpsc::unbounded_channel::<(ViewerId, Output)>();
    let mut viewers: HashMap<ViewerId, ChannelId> = HashMap::new();
    // The frames of the tasks of the dbus channels: replies and signals.
    let (events, mut frames) = mpsc::unbounded_channel::<Frame>();
    // The lock and unlock requests of logind go to farglassd.
    let mut lock_requests = session.logind.subscribe();

    loop {
        let frame = tokio::select! {
            frame = link.next() => match frame {
                Some(frame) => frame?,
                None => return Ok(()),
            },
            Some(event) = frames.recv() => {
                // A frame of a channel that closed in the meantime goes nowhere.
                if let Some(Channel { closed_by_agent: false, .. }) = channels.get(&event.channel()) {
                    link.send(event).await?;
                }
                continue;
            }
            Ok(lock) = lock_requests.recv() => {
                let message = if lock { LinkMessage::Lock } else { LinkMessage::Unlock };
                link.send(Frame::json(ChannelId::CONNECTION, FrameType::Control, &message)?).await?;
                continue;
            }
            Some((viewer, out)) = output.recv() => {
                let Some(&channel) = viewers.get(&viewer) else { continue };
                if let Some(Channel { closed_by_agent: false, .. }) = channels.get(&channel) {
                    let exited = terminal_output(&mut link, channel, out).await?;
                    if exited && let Some(state) = channels.get_mut(&channel) {
                        state.closed_by_agent = true;
                        state.kind = Kind::Closed;
                        viewers.remove(&viewer);
                    }
                }
                continue;
            }
        };
        let channel = frame.channel();
        if channel.is_connection() {
            match link_message(&frame)? {
                LinkMessage::Attach { connection } => {
                    tracing::info!(connection, "a browser connection attached");
                }
                LinkMessage::Detach { connection } => {
                    tracing::info!(connection, "a browser connection detached");
                }
                LinkMessage::LockedHint { locked } => session.logind.set_locked_hint(locked),
                LinkMessage::Hello { .. } | LinkMessage::Lock | LinkMessage::Unlock => {
                    return Err(LinkError::Protocol(format!(
                        "farglassd sent a message that only the agent sends: {:?}",
                        link_message(&frame)?
                    )));
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
            let (state, answer) = open(channel, &frame, session, &sink, &events)?;
            if let Kind::Pty(terminal) = &state.kind {
                viewers.insert(terminal.viewer(), channel);
            }
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
                // A pty channel detaches. The terminal continues.
                if let Some(Channel {
                    kind: Kind::Pty(terminal),
                    ..
                }) = channels.remove(&channel)
                {
                    viewers.remove(&terminal.viewer());
                }
                watchers.retain(|_, ids| {
                    ids.remove(&channel);
                    !ids.is_empty()
                });
            }
            FrameType::Data | FrameType::Control if !state.closed_by_agent => {
                let refusal = match &mut state.kind {
                    Kind::Echo => {
                        link.send(frame).await?;
                        None
                    }
                    Kind::Pty(terminal) => pty_frame(terminal, frame),
                    Kind::Dbus { bus, subscriptions }
                        if frame.frame_type() == FrameType::Control =>
                    {
                        dbus_request(
                            &session.buses,
                            *bus,
                            subscriptions,
                            channel,
                            &frame,
                            &events,
                        )
                        .await
                    }
                    Kind::Dbus { .. } => None,
                    Kind::Polkit { .. } if frame.frame_type() == FrameType::Control => {
                        match frame.parse::<PolkitReply>() {
                            Ok(reply) => {
                                session.polkit.reply(reply);
                                None
                            }
                            Err(error) => Some((
                                CloseReason::ProtocolError,
                                format!("The polkit reply is not valid: {error}"),
                            )),
                        }
                    }
                    Kind::Polkit { .. } => None,
                    Kind::Notify { .. } if frame.frame_type() == FrameType::Control => {
                        match frame.parse::<NotifyReply>() {
                            Ok(reply) => {
                                session.notifications.reply(reply);
                                None
                            }
                            Err(error) => Some((
                                CloseReason::ProtocolError,
                                format!("The notification reply is not valid: {error}"),
                            )),
                        }
                    }
                    Kind::Notify { .. } => None,
                    Kind::State if frame.frame_type() == FrameType::Control => {
                        state_request(
                            &mut link,
                            &mut session.store,
                            &mut watchers,
                            channel,
                            &frame,
                        )
                        .await?
                    }
                    Kind::State | Kind::Closed => None,
                };
                if let Some((reason, message)) = refusal {
                    state.closed_by_agent = true;
                    state.kind = Kind::Closed;
                    let close = Close {
                        reason,
                        message: Some(message),
                    };
                    link.send(Frame::close(channel, &close)?).await?;
                }
            }
            // Frames that the opener sent before it got the refusal, or a ready frame
            // that farglassd does not send on a channel of the shell.
            FrameType::Data | FrameType::Control | FrameType::Ready | FrameType::Open => {}
        }
    }
}

type Link<S> = Framed<S, FrameCodec>;

/// A refusal of a request: the reason and the message of the close frame.
type Refusal = Option<(CloseReason, String)>;

/// Handles a frame of the shell on a pty channel.
fn pty_frame(terminal: &TerminalLink, frame: Frame) -> Refusal {
    if frame.frame_type() == FrameType::Data {
        terminal.input(frame.into_payload());
        return None;
    }
    match frame.parse::<PtyRequest>() {
        Ok(PtyRequest::Resize { cols, rows }) => terminal.resize(rows, cols),
        Ok(PtyRequest::Ack { bytes }) => terminal.ack(bytes),
        Ok(PtyRequest::Terminate) => terminal.terminate(),
        Err(error) => {
            return Some((
                CloseReason::ProtocolError,
                format!("The pty request is not valid: {error}"),
            ));
        }
    }
    None
}

/// Handles a request on a dbus channel. A call runs in its own task, thus a slow
/// service, for example one that waits for a polkit prompt, does not stop the link.
async fn dbus_request(
    buses: &Arc<Buses>,
    bus: Bus,
    subscriptions: &mut Subscriptions,
    channel: ChannelId,
    frame: &Frame,
    events: &mpsc::UnboundedSender<Frame>,
) -> Refusal {
    let request = match frame.parse::<DbusRequest>() {
        Ok(request) => request,
        Err(error) => {
            return Some((
                CloseReason::ProtocolError,
                format!("The dbus request is not valid: {error}"),
            ));
        }
    };
    let send = {
        let events = events.clone();
        move |event: DbusEvent| {
            if let Ok(frame) = Frame::json(channel, FrameType::Control, &event) {
                let _ = events.send(frame);
            }
        }
    };
    match request {
        DbusRequest::Call {
            id,
            destination,
            path,
            interface,
            method,
            signature,
            args,
            interactive,
        } => {
            let buses = buses.clone();
            let call = dbus::Call {
                destination,
                path,
                interface,
                method,
                signature,
                args,
                interactive,
            };
            tokio::spawn(async move { send(dbus::call(&buses, bus, id, call).await) });
        }
        DbusRequest::Subscribe {
            id,
            sender,
            path,
            interface,
            member,
        } => {
            let filter = Filter {
                sender,
                path,
                interface,
                member,
            };
            match dbus::subscribe(buses, bus, id, filter, send.clone()).await {
                Ok(task) => {
                    subscriptions.insert(id, task);
                    send(DbusEvent::Reply {
                        id,
                        values: Vec::new(),
                    });
                }
                Err(error) => send(error),
            }
        }
        DbusRequest::Unsubscribe { id } => {
            subscriptions.remove(id);
            send(DbusEvent::Reply {
                id,
                values: Vec::new(),
            });
        }
    }
    None
}

/// Sends one output of a terminal on its channel. Returns `true` if the terminal stopped.
async fn terminal_output<S>(
    link: &mut Link<S>,
    channel: ChannelId,
    output: Output,
) -> Result<bool, LinkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match output {
        Output::Attached { new, replay } => {
            let bytes: usize = replay.iter().map(Bytes::len).sum();
            let event = PtyEvent::Attached {
                new,
                replay: u32::try_from(bytes).unwrap_or(u32::MAX),
            };
            link.send(Frame::json(channel, FrameType::Control, &event)?)
                .await?;
            for chunk in replay {
                link.send(Frame::data(channel, chunk)?).await?;
            }
            Ok(false)
        }
        Output::Data(chunk) => {
            link.send(Frame::data(channel, chunk)?).await?;
            Ok(false)
        }
        Output::Exited { code, signal } => {
            link.send(Frame::json(
                channel,
                FrameType::Control,
                &PtyEvent::Exited { code, signal },
            )?)
            .await?;
            link.send(Frame::close(channel, &Close::new(CloseReason::Done))?)
                .await?;
            Ok(true)
        }
    }
}

/// Handles a request on a state channel.
async fn state_request<S>(
    link: &mut Link<S>,
    store: &mut StateStore,
    watchers: &mut HashMap<String, HashSet<ChannelId>>,
    channel: ChannelId,
    frame: &Frame,
) -> Result<Refusal, LinkError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    Ok(match frame.parse::<StateRequest>() {
        Ok(StateRequest::Get { key }) => {
            let value = store.get(&key);
            watchers.entry(key.clone()).or_default().insert(channel);
            let event = StateEvent::Value { key, value };
            link.send(Frame::json(channel, FrameType::Control, &event)?)
                .await?;
            None
        }
        Ok(StateRequest::Set { key, value }) => match store.set(&key, value.clone()) {
            Ok(()) => {
                let event = StateEvent::Value {
                    key: key.clone(),
                    value,
                };
                let others: Vec<ChannelId> = watchers
                    .get(&key)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|id| *id != channel)
                    .collect();
                for other in others {
                    link.send(Frame::json(other, FrameType::Control, &event)?)
                        .await?;
                }
                None
            }
            Err(error) => Some((CloseReason::ResourceLimit, error.to_string())),
        },
        Err(error) => Some((
            CloseReason::ProtocolError,
            format!("The state request is not valid: {error}"),
        )),
    })
}

/// The kind of an open channel.
#[derive(Debug)]
enum Kind {
    /// Sends each data frame and each control frame back.
    Echo,
    /// Gives the values of the state store of the session.
    State,
    /// Is attached to a terminal.
    Pty(TerminalLink),
    /// Gets the polkit requests of the session. Dropping the channel removes it.
    Polkit {
        /// Dropping the guard removes the channel from the agent.
        _viewer: ViewerGuard,
    },
    /// Gets the desktop notifications. Dropping the channel removes it.
    Notify {
        /// Dropping the guard removes the channel from the agent.
        _viewer: notify::ViewerGuard,
    },
    /// Calls methods and gets signals on a D-Bus bus.
    Dbus {
        bus: Bus,
        /// Dropping the channel ends its subscriptions.
        subscriptions: Subscriptions,
    },
    /// The agent refused or closed the channel.
    Closed,
}

/// One channel of the link.
#[derive(Debug)]
struct Channel {
    kind: Kind,
    /// Reports whether the agent sent a close frame.
    closed_by_agent: bool,
}

/// Accepts or refuses a new channel. Returns its state and the answer to the open frame.
fn open(
    channel: ChannelId,
    frame: &Frame,
    session: &mut Session,
    sink: &Sink,
    events: &mpsc::UnboundedSender<Frame>,
) -> Result<(Channel, Frame), LinkError> {
    let terminals = &mut session.terminals;
    let accept = |kind| {
        Ok((
            Channel {
                kind,
                closed_by_agent: false,
            },
            Frame::ready(channel, &Ready {})?,
        ))
    };
    let refuse = |reason, message: String| {
        let close = Close {
            reason,
            message: Some(message),
        };
        Ok((
            Channel {
                kind: Kind::Closed,
                closed_by_agent: true,
            },
            Frame::close(channel, &close)?,
        ))
    };
    match frame.parse::<Open>() {
        Ok(Open::Echo) => accept(Kind::Echo),
        Ok(Open::State) => accept(Kind::State),
        Ok(Open::Polkit) => {
            let events = events.clone();
            let guard = session.polkit.add_viewer(move |event| {
                if let Ok(frame) = Frame::json(channel, FrameType::Control, &event) {
                    let _ = events.send(frame);
                }
            });
            accept(Kind::Polkit { _viewer: guard })
        }
        Ok(Open::Notify) => {
            let events = events.clone();
            let guard = session.notifications.add_viewer(move |event| {
                if let Ok(frame) = Frame::json(channel, FrameType::Control, &event) {
                    let _ = events.send(frame);
                }
            });
            accept(Kind::Notify { _viewer: guard })
        }
        Ok(Open::Dbus { bus }) => accept(Kind::Dbus {
            bus,
            subscriptions: Subscriptions::default(),
        }),
        Ok(Open::Pty {
            terminal,
            cols,
            rows,
        }) => match terminals.attach(&terminal, rows, cols, sink.clone()) {
            Ok(link) => accept(Kind::Pty(link)),
            Err(error @ AttachError::Limit) => {
                refuse(CloseReason::ResourceLimit, error.to_string())
            }
            Err(error) => {
                tracing::error!(%error, "a terminal did not start");
                refuse(CloseReason::InternalError, error.to_string())
            }
        },
        // farglassd serves the login channel itself. It never gives it to the agent.
        Ok(Open::Login) | Err(_) => refuse(
            CloseReason::NotSupported,
            "The agent does not know this channel kind.".to_owned(),
        ),
    }
}

fn link_message(frame: &Frame) -> Result<LinkMessage, LinkError> {
    frame
        .parse()
        .map_err(|error| LinkError::Protocol(format!("a link message is not valid: {error}")))
}
