//! One browser connection: the hello exchange and the relay to the agent of its user.
//!
//! The code does not know the transport. A transport gives a stream of frames
//! and a sink for frames (ADR-005).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use farglass_proto::message::{Close, CloseReason, ConnectionMessage};
use farglass_proto::{ChannelId, Frame, FrameError, FrameType, PROTOCOL_VERSION, Side};
use futures_util::{Sink, SinkExt as _, Stream, StreamExt as _};
use tokio::sync::mpsc;

use crate::agents::{Agent, ConnectionId, Registry, ToAgent};

/// The frames from the agent that wait for the browser. When the queue is
/// full, the agent link of the user waits. Flow control comes with F014.
const TO_SHELL_QUEUE: usize = 64;

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

/// The user of a browser connection, or no user before the login.
pub type User = Option<u32>;

/// The state of one channel of the shell.
#[derive(Debug)]
struct Local {
    agent: Arc<Agent>,
    /// The number of the channel on the agent link.
    number: ChannelId,
    closed_by_shell: bool,
    closed_by_agent: bool,
}

/// A frame for an agent, and the channel of the shell that it belongs to.
#[derive(Debug)]
struct Pending {
    shell: ChannelId,
    frame: ToAgent,
}

struct Connection<'a, O> {
    id: ConnectionId,
    user: User,
    registry: &'a Registry,
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
/// `user` is the uid of the user who logged in, or `None`. The function sends
/// the hello message first. It gives each channel to the agent of the user.
///
/// # Errors
///
/// [`Violation`] if the shell breaks the protocol. The caller must close the transport.
pub async fn run<I, O, E>(
    registry: &Registry,
    user: User,
    incoming: &mut I,
    outgoing: &mut O,
) -> Result<(), Violation>
where
    I: Stream<Item = Result<Frame, Violation>> + Unpin,
    O: Sink<Frame, Error = E> + Unpin,
{
    let hello = ConnectionMessage::Hello {
        version: PROTOCOL_VERSION,
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
                ConnectionMessage::Hello { version } if version == PROTOCOL_VERSION => {}
                ConnectionMessage::Hello { version } => return Err(Violation::Version(version)),
            }
        }
    }

    let (to_shell, mut from_agents) = mpsc::channel(TO_SHELL_QUEUE);
    let mut conn = Connection {
        id: registry.connection_id(),
        user,
        registry,
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
        loop {
            tokio::select! {
                // Frames to the browser first: they free the queue of the agent link.
                biased;
                Some(frame) = from_agents.recv() => {
                    self.note_agent_frame(&frame);
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
            };
        }

        if frame.frame_type() == FrameType::Open {
            if channel.opener() != Some(Side::Shell) || channel.get() <= self.last_opened {
                return Err(Violation::ChannelNumber(channel));
            }
            self.last_opened = channel.get();
            return self.open(&frame).await;
        }

        let Some(local) = self.channels.get_mut(&channel) else {
            if channel.opener() == Some(Side::Shell) && channel.get() <= self.last_opened {
                // A channel that farglassd refused, or that is closed. Drop the frame.
                return Ok(true);
            }
            return Err(Violation::NotOpen(channel));
        };
        if local.closed_by_shell {
            return Ok(true);
        }
        let Some(to_agent) = local.agent.forward(local.number, &frame) else {
            return Ok(true);
        };
        if frame.frame_type() == FrameType::Close {
            local.closed_by_shell = true;
            if local.closed_by_agent {
                self.channels.remove(&channel);
            }
        }
        self.pending.push_back(Pending {
            shell: channel,
            frame: to_agent,
        });
        Ok(true)
    }

    /// Gives a new channel to the agent of the user, or refuses it.
    async fn open(&mut self, frame: &Frame) -> Result<bool, Violation> {
        let channel = frame.channel();
        let Some(uid) = self.user else {
            return self
                .refuse(
                    channel,
                    CloseReason::AccessDenied,
                    "The connection has no login.",
                )
                .await;
        };
        let Some(agent) = self.registry.get(uid) else {
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
            Local {
                agent,
                number,
                closed_by_shell: false,
                closed_by_agent: false,
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

    /// Updates the state of a channel for a frame that goes from the agent to the shell.
    fn note_agent_frame(&mut self, frame: &Frame) {
        if frame.frame_type() != FrameType::Close {
            return;
        }
        let channel = frame.channel();
        if let Some(local) = self.channels.get_mut(&channel) {
            local.closed_by_agent = true;
            if local.closed_by_shell {
                self.channels.remove(&channel);
            }
        }
    }

    /// The agent of a channel stopped before it got a frame. Closes the channel
    /// on the browser side. Returns `false` if the browser stopped.
    async fn agent_gone(&mut self, channel: ChannelId) -> bool {
        let Some(local) = self.channels.remove(&channel) else {
            return true;
        };
        if local.closed_by_agent {
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
    fn detach(&mut self) {
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
