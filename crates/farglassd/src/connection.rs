//! One browser connection: the hello exchange, the login, and the relay to the agent of its user.
//!
//! The code does not know the transport. A transport gives a stream of frames
//! and a sink for frames (ADR-005).

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use std::time::Duration;

use farglass_proto::login::LoginReply;
use farglass_proto::message::{Close, CloseReason, ConnectionMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameError, FrameType, PROTOCOL_VERSION, Side};
use futures_util::{Sink, SinkExt as _, Stream, StreamExt as _};
use tokio::sync::mpsc;

use crate::agents::{Agent, ConnectionId, Registry, ToAgent};
use crate::cookie::SessionStore;
use crate::login::{Identity, LoginTask, SharedUser};
use crate::origin::OriginPolicy;
use crate::presence::{IDLE_LOGOUT, Presence};

/// The frames from the agent that wait for the browser. When the queue is
/// full, the agent link of the user waits. Flow control comes with F014.
const TO_SHELL_QUEUE: usize = 64;

/// The answers that wait for farglass-login.
const ANSWER_QUEUE: usize = 4;

/// The state that all connections share.
#[derive(Debug)]
pub struct Context {
    /// The connected agents.
    pub registry: Arc<Registry>,
    /// The login socket of farglass-login.
    pub login_socket: PathBuf,
    /// The user of each connection with no login. Only for development.
    pub dev_user: Option<Identity>,
    /// The longest wait for the agent of a user after a login.
    pub agent_wait: Duration,
    /// The tickets and the sessions of the cookies.
    pub sessions: Arc<SessionStore>,
    /// The Origin check.
    pub origin: OriginPolicy,
    /// The browser connections of each user, and the logout announcement.
    pub presence: Arc<Presence>,
    /// The time with no browser connection before the idle logout. `None` turns it off.
    pub idle_logout: Option<Duration>,
    /// Reports whether responses get Strict-Transport-Security. Only with a
    /// certificate of the administrator: with a self-signed certificate, the
    /// header stops the user from accepting the next certificate.
    pub hsts: bool,
}

impl Context {
    /// Returns a context with `registry` and `login_socket` and the defaults for all other fields.
    #[must_use]
    pub fn new(registry: Arc<Registry>, login_socket: PathBuf) -> Self {
        Self {
            registry,
            login_socket,
            dev_user: None,
            agent_wait: AGENT_WAIT,
            sessions: Arc::new(SessionStore::default()),
            origin: OriginPolicy::default(),
            presence: Arc::new(Presence::default()),
            idle_logout: Some(IDLE_LOGOUT),
            hsts: false,
        }
    }
}

/// The default wait for the agent after a login. A new session starts the agent in about one second.
pub const AGENT_WAIT: Duration = Duration::from_secs(30);

/// A cause to close the browser connection.
#[derive(Debug, thiserror::Error)]
pub enum Violation {
    /// A frame was not valid.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// The shell has a different protocol version.
    #[error("the shell has protocol version {0}")]
    Version(u16),
    /// The first frame was not a hello message.
    #[error("the first frame was not a hello message")]
    NoHello,
    /// The shell sent a second hello message.
    #[error("the shell sent a second hello message")]
    SecondHello,
    /// The transport got a message that is not binary.
    #[error("the message was not binary")]
    NotBinary,
    /// The shell opened a channel with a number that it cannot use.
    #[error("the shell cannot open channel {0}")]
    ChannelNumber(ChannelId),
    /// The shell sent a frame on a channel that it did not open.
    #[error("channel {0} is not open")]
    NotOpen(ChannelId),
}

/// The state of one channel of the shell.
#[derive(Debug)]
enum Local {
    /// A channel of the agent.
    Agent {
        agent: Arc<Agent>,
        /// The number of the channel on the agent link.
        number: ChannelId,
        closed_by_shell: bool,
        closed_by_peer: bool,
    },
    /// The login channel, which farglassd serves.
    Login {
        /// The queue to the login task. `None` after the shell closed the channel.
        answers: Option<mpsc::Sender<LoginReply>>,
        closed_by_shell: bool,
        closed_by_peer: bool,
    },
}

impl Local {
    fn closed_by_shell(&self) -> bool {
        match self {
            Self::Agent {
                closed_by_shell, ..
            }
            | Self::Login {
                closed_by_shell, ..
            } => *closed_by_shell,
        }
    }

    /// Marks the close of one side. Returns `true` if the two sides closed the channel.
    fn mark_closed(&mut self, by_shell: bool) -> bool {
        let (shell, peer) = match self {
            Self::Agent {
                closed_by_shell,
                closed_by_peer,
                ..
            }
            | Self::Login {
                closed_by_shell,
                closed_by_peer,
                ..
            } => (closed_by_shell, closed_by_peer),
        };
        if by_shell {
            *shell = true;
        } else {
            *peer = true;
        }
        *shell && *peer
    }
}

/// A frame for an agent, and the channel of the shell that it belongs to.
#[derive(Debug)]
struct Pending {
    shell: ChannelId,
    frame: ToAgent,
}

struct Connection<'a, O> {
    id: ConnectionId,
    context: &'a Context,
    rhost: String,
    user: SharedUser,
    outgoing: &'a mut O,
    to_shell: mpsc::Sender<Frame>,
    channels: HashMap<ChannelId, Local>,
    /// The agents that this connection attached to.
    agents: Vec<Arc<Agent>>,
    /// The highest channel number that the shell opened.
    last_opened: u32,
    pending: VecDeque<Pending>,
}

/// Runs one browser connection until the shell closes it or breaks the protocol.
///
/// `peer` is the address of the browser, for the rate limiter of the login.
/// `user` is the user from the session cookie or from `dev_user`, or `None`.
/// The function sends the hello message first, with the user if the
/// connection has one. It gives each channel to the agent of the user.
///
/// # Errors
///
/// [`Violation`] if the shell breaks the protocol. The caller must close the transport.
pub async fn run<I, O, E>(
    context: &Context,
    peer: Option<IpAddr>,
    user: Option<Identity>,
    incoming: &mut I,
    outgoing: &mut O,
) -> Result<(), Violation>
where
    I: Stream<Item = Result<Frame, Violation>> + Unpin,
    O: Sink<Frame, Error = E> + Unpin,
{
    let user = SharedUser::new(user, context.presence.clone());
    let hello = ConnectionMessage::Hello {
        version: PROTOCOL_VERSION,
        user: user.get().map(|identity| identity.name),
    };
    if outgoing.send(Frame::connection(&hello)?).await.is_err() {
        return Ok(());
    }
    match incoming.next().await {
        None => return Ok(()),
        Some(frame) => {
            let frame = frame?;
            if !frame.channel().is_connection() {
                return Err(Violation::NoHello);
            }
            match frame.parse::<ConnectionMessage>()? {
                ConnectionMessage::Hello { version, .. } if version == PROTOCOL_VERSION => {}
                ConnectionMessage::Hello { version, .. } => {
                    return Err(Violation::Version(version));
                }
                ConnectionMessage::LoggedOut => return Err(Violation::NoHello),
            }
        }
    }

    let (to_shell, mut from_agents) = mpsc::channel(TO_SHELL_QUEUE);
    let mut conn = Connection {
        id: context.registry.connection_id(),
        context,
        rhost: peer.map_or_else(String::new, |ip| ip.to_canonical().to_string()),
        user,
        outgoing,
        to_shell,
        channels: HashMap::new(),
        agents: Vec::new(),
        last_opened: 0,
        pending: VecDeque::new(),
    };
    let result = conn.relay(incoming, &mut from_agents).await;
    conn.detach();
    result
}

impl<O, E> Connection<'_, O>
where
    O: Sink<Frame, Error = E> + Unpin,
{
    async fn relay<I>(
        &mut self,
        incoming: &mut I,
        from_agents: &mut mpsc::Receiver<Frame>,
    ) -> Result<(), Violation>
    where
        I: Stream<Item = Result<Frame, Violation>> + Unpin,
    {
        let mut logouts = self.context.presence.subscribe();
        loop {
            tokio::select! {
                // Frames to the browser first: they free the queue of the agent link.
                biased;
                logout = logouts.recv() => {
                    if let Ok(uid) = logout
                        && self.user.get().is_some_and(|user| user.uid == uid)
                    {
                        if let Ok(frame) = Frame::connection(&ConnectionMessage::LoggedOut) {
                            let _ = self.outgoing.send(frame).await;
                        }
                        return Ok(());
                    }
                }
                Some(frame) = from_agents.recv() => {
                    self.note_peer_frame(&frame);
                    if self.outgoing.send(frame).await.is_err() {
                        return Ok(());
                    }
                }
                sent = send_first(&self.pending), if !self.pending.is_empty() => {
                    let Some(pending) = self.pending.pop_front() else { continue };
                    if !sent && !self.agent_gone(pending.shell).await {
                        return Ok(());
                    }
                }
                // Read from the browser only when no frame waits for an agent.
                frame = incoming.next(), if self.pending.is_empty() => match frame {
                    None => return Ok(()),
                    Some(frame) => {
                        if !self.handle_shell(frame?).await? {
                            return Ok(());
                        }
                    }
                },
            }
        }
    }

    /// Handles one frame from the shell. Returns `false` if the browser stopped.
    async fn handle_shell(&mut self, frame: Frame) -> Result<bool, Violation> {
        let channel = frame.channel();
        if channel.is_connection() {
            return match frame.parse::<ConnectionMessage>()? {
                ConnectionMessage::Hello { .. } => Err(Violation::SecondHello),
                // Only farglassd sends this message.
                ConnectionMessage::LoggedOut => Err(Violation::SecondHello),
            };
        }

        if frame.frame_type() == FrameType::Open {
            if channel.opener() != Some(Side::Shell) || channel.get() <= self.last_opened {
                return Err(Violation::ChannelNumber(channel));
            }
            self.last_opened = channel.get();
            return match frame.parse::<Open>() {
                Ok(Open::Login) => self.open_login(channel).await,
                // farglassd gives each other kind to the agent, also a kind that it does not know.
                Ok(_) | Err(_) => self.open_agent(&frame).await,
            };
        }

        let Some(local) = self.channels.get_mut(&channel) else {
            if channel.opener() == Some(Side::Shell) && channel.get() <= self.last_opened {
                // A channel that farglassd refused, or that is closed. Drop the frame.
                return Ok(true);
            }
            return Err(Violation::NotOpen(channel));
        };
        if local.closed_by_shell() {
            return Ok(true);
        }
        let is_close = frame.frame_type() == FrameType::Close;
        match local {
            Local::Agent { agent, number, .. } => {
                if let Some(to_agent) = agent.forward(*number, &frame) {
                    self.pending.push_back(Pending {
                        shell: channel,
                        frame: to_agent,
                    });
                }
            }
            Local::Login { answers, .. } => {
                if is_close {
                    // Dropping the queue stops the attempt. The task answers with a close frame.
                    *answers = None;
                } else if frame.frame_type() == FrameType::Control {
                    let answer = frame.parse::<LoginReply>()?;
                    if let Some(answers) = answers {
                        // A full queue means the shell sends answers that nobody asked for.
                        let _ = answers.try_send(answer);
                    }
                }
            }
        }
        if is_close && local.mark_closed(true) {
            self.channels.remove(&channel);
        }
        Ok(true)
    }

    /// Starts a login on a new channel, or refuses it.
    async fn open_login(&mut self, channel: ChannelId) -> Result<bool, Violation> {
        if self.user.get().is_some() {
            return self
                .refuse(
                    channel,
                    CloseReason::NotSupported,
                    "The connection already has a user.",
                )
                .await;
        }
        if self
            .channels
            .values()
            .any(|local| matches!(local, Local::Login { .. }))
        {
            return self
                .refuse(
                    channel,
                    CloseReason::ResourceLimit,
                    "A login is already running.",
                )
                .await;
        }
        let (answers, receiver) = mpsc::channel(ANSWER_QUEUE);
        let task = LoginTask {
            socket: self.context.login_socket.clone(),
            rhost: self.rhost.clone(),
            channel,
            answers: receiver,
            to_shell: self.to_shell.clone(),
            user: self.user.clone(),
            registry: self.context.registry.clone(),
            agent_wait: self.context.agent_wait,
            sessions: self.context.sessions.clone(),
        };
        tokio::spawn(task.run());
        self.channels.insert(
            channel,
            Local::Login {
                answers: Some(answers),
                closed_by_shell: false,
                closed_by_peer: false,
            },
        );
        Ok(true)
    }

    /// Gives a new channel to the agent of the user, or refuses it.
    async fn open_agent(&mut self, frame: &Frame) -> Result<bool, Violation> {
        let channel = frame.channel();
        let Some(user) = self.user.get() else {
            return self
                .refuse(
                    channel,
                    CloseReason::AccessDenied,
                    "The connection has no login.",
                )
                .await;
        };
        let Some(agent) = self.context.registry.get(user.uid) else {
            return self
                .refuse(
                    channel,
                    CloseReason::NotFound,
                    "The user has no session agent.",
                )
                .await;
        };
        let Some((number, frames)) = agent.open(self.id, &self.to_shell, frame) else {
            return self
                .refuse(channel, CloseReason::NotFound, "The session agent stopped.")
                .await;
        };
        if !self.agents.iter().any(|known| Arc::ptr_eq(known, &agent)) {
            self.agents.push(agent.clone());
        }
        self.channels.insert(
            channel,
            Local::Agent {
                agent,
                number,
                closed_by_shell: false,
                closed_by_peer: false,
            },
        );
        self.pending.extend(frames.into_iter().map(|frame| Pending {
            shell: channel,
            frame,
        }));
        Ok(true)
    }

    /// Sends a close frame for a channel that the shell opened. Returns `false` if the browser stopped.
    async fn refuse(
        &mut self,
        channel: ChannelId,
        reason: CloseReason,
        message: &str,
    ) -> Result<bool, Violation> {
        let close = Close {
            reason,
            message: Some(message.to_owned()),
        };
        let frame = Frame::close(channel, &close)?;
        Ok(self.outgoing.send(frame).await.is_ok())
    }

    /// Updates the state of a channel for a frame that goes from the agent or the login task to the shell.
    fn note_peer_frame(&mut self, frame: &Frame) {
        if frame.frame_type() != FrameType::Close {
            return;
        }
        let channel = frame.channel();
        if let Some(local) = self.channels.get_mut(&channel)
            && local.mark_closed(false)
        {
            self.channels.remove(&channel);
        }
    }

    /// The agent of a channel stopped before it got a frame. Closes the channel
    /// on the browser side. Returns `false` if the browser stopped.
    async fn agent_gone(&mut self, channel: ChannelId) -> bool {
        let Some(Local::Agent { closed_by_peer, .. }) = self.channels.remove(&channel) else {
            return true;
        };
        if closed_by_peer {
            return true;
        }
        let close = Close {
            reason: CloseReason::InternalError,
            message: Some("The session agent stopped.".to_owned()),
        };
        match Frame::close(channel, &close) {
            Ok(frame) => self.outgoing.send(frame).await.is_ok(),
            Err(_) => true,
        }
    }

    /// Tells each agent that the connection stopped. The agents get the frames
    /// in a separate task, thus a full queue does not delay the end of the connection.
    /// Dropping the login channels stops their attempts.
    fn detach(&mut self) {
        self.channels.clear();
        let mut frames: Vec<ToAgent> = self.pending.drain(..).map(|p| p.frame).collect();
        for agent in self.agents.drain(..) {
            frames.extend(agent.detach(self.id));
        }
        if frames.is_empty() {
            return;
        }
        tokio::spawn(async move {
            for frame in frames {
                frame.send().await;
            }
        });
    }
}

/// Sends the first pending frame. Returns `false` if its agent stopped.
async fn send_first(pending: &VecDeque<Pending>) -> bool {
    match pending.front() {
        Some(first) => first.frame.send().await,
        None => std::future::pending().await,
    }
}
