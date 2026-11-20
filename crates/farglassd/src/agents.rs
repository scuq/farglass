//! The agents that are connected to farglassd, one for each user (ADR-003, ADR-015).
//!
//! Each agent connects to the agent socket as the user who owns the session.
//! The kernel tells farglassd the uid of the peer (SO_PEERCRED). farglassd
//! gives a browser connection of a user only to the agent of that uid.
//!
//! One agent link carries the channels of all browser connections of its user.
//! Each browser connection numbers its channels from 1, thus farglassd gives
//! each channel a new number on the agent link. The agent sees only that number.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{Close, CloseReason, LinkMessage};
use farglass_proto::{ChannelId, ChannelIdAllocator, Frame, FrameType, PROTOCOL_VERSION, Side};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixListener;
use tokio::sync::{broadcast, mpsc};
use tokio_util::codec::{FramedRead, FramedWrite};

/// The agent must send its hello message in this time.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest number of agent links of one uid at the same time, with links that wait for their hello (F051).
pub const MAX_LINKS_PER_UID: usize = 4;

/// The frames that wait for the agent. When the queue is full, the browser
/// connections of the user wait.
const TO_AGENT_QUEUE: usize = 256;

/// The number of a browser connection. farglassd uses each number one time only.
pub type ConnectionId = u64;

/// An error of the agent socket.
#[derive(Debug, thiserror::Error)]
#[error("The agent socket {} is not usable: {source}.", path.display())]
pub struct BindError {
    /// The socket.
    pub path: PathBuf,
    /// The cause.
    pub source: io::Error,
}

/// A message of an agent for farglassd, with the uid of the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    /// The agent of the user connected.
    Connected(u32),
    /// logind asks to lock the session of the user.
    Lock(u32),
    /// logind asks to unlock the session of the user.
    Unlock(u32),
}

/// The agents that are connected now, by uid.
#[derive(Debug)]
pub struct Registry {
    agents: Mutex<HashMap<u32, Arc<Agent>>>,
    next_agent: AtomicU64,
    next_connection: AtomicU64,
    events: broadcast::Sender<AgentEvent>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            agents: Mutex::new(HashMap::new()),
            next_agent: AtomicU64::new(0),
            next_connection: AtomicU64::new(0),
            events: broadcast::channel(64).0,
        }
    }
}

impl Registry {
    /// Returns a new registry with no agents.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Returns a receiver that gets the connects of the agents and their lock and unlock requests.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.events.subscribe()
    }

    /// Returns the agent of the user `uid`, if one is connected.
    #[must_use]
    pub fn get(&self, uid: u32) -> Option<Arc<Agent>> {
        lock(&self.agents).get(&uid).cloned()
    }

    /// Returns the uids that have a connected agent, in sorted sequence.
    #[must_use]
    pub fn uids(&self) -> Vec<u32> {
        let mut uids: Vec<u32> = lock(&self.agents).keys().copied().collect();
        uids.sort_unstable();
        uids
    }

    /// Returns a new number for a browser connection.
    pub fn connection_id(&self) -> ConnectionId {
        self.next_connection.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Serves the agent link of the user `uid` on `stream` until the link stops.
    ///
    /// The caller must know the uid from the kernel, not from the peer.
    /// If the user already has an agent, the new agent replaces it, for example after a restart.
    pub async fn serve<S>(self: &Arc<Self>, stream: S, uid: u32)
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (read, write) = tokio::io::split(stream);
        let mut reader = FramedRead::new(read, FrameCodec);
        let mut writer = FramedWrite::new(write, FrameCodec);

        let hello = LinkMessage::Hello {
            version: PROTOCOL_VERSION,
        };
        let sent = match Frame::json(ChannelId::CONNECTION, FrameType::Control, &hello) {
            Ok(frame) => writer.send(frame).await.map_err(|e| e.to_string()),
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = sent {
            tracing::warn!(uid, %error, "the agent link failed before the hello message");
            return;
        }
        match tokio::time::timeout(HELLO_TIMEOUT, reader.next()).await {
            Ok(Some(Ok(frame))) => match parse_link(&frame) {
                Some(LinkMessage::Hello { version }) if version == PROTOCOL_VERSION => {}
                other => {
                    tracing::warn!(uid, message = ?other, "the agent did not send a valid hello message");
                    return;
                }
            },
            Ok(Some(Err(error))) => {
                tracing::warn!(uid, %error, "the agent link failed");
                return;
            }
            Ok(None) => return,
            Err(_) => {
                tracing::warn!(uid, "the agent sent no hello message");
                return;
            }
        }

        let (to_agent, mut queue) = mpsc::channel::<Frame>(TO_AGENT_QUEUE);
        let agent = Arc::new(Agent {
            id: self.next_agent.fetch_add(1, Ordering::Relaxed),
            uid,
            to_agent,
            routes: Mutex::new(Routes::default()),
            events: self.events.clone(),
        });
        if let Some(old) = lock(&self.agents).insert(uid, agent.clone()) {
            tracing::info!(uid, "a new agent replaced the old agent of the user");
            old.stop("A new agent replaced the agent of the session.");
        }
        tracing::info!(uid, "agent connected");
        let _ = self.events.send(AgentEvent::Connected(uid));

        let write_loop = async {
            while let Some(frame) = queue.recv().await {
                if writer.send(frame).await.is_err() {
                    break;
                }
            }
        };
        let read_loop = async {
            while let Some(Ok(frame)) = reader.next().await {
                if !agent.deliver(frame).await {
                    break;
                }
            }
        };
        tokio::select! {
            () = write_loop => {}
            () = read_loop => {}
        }

        {
            let mut agents = lock(&self.agents);
            if agents
                .get(&uid)
                .is_some_and(|current| current.id == agent.id)
            {
                agents.remove(&uid);
            }
        }
        agent.stop("The agent of the session stopped.");
        tracing::info!(uid, "agent disconnected");
    }
}

/// One connected agent.
#[derive(Debug)]
pub struct Agent {
    id: u64,
    uid: u32,
    to_agent: mpsc::Sender<Frame>,
    routes: Mutex<Routes>,
    events: broadcast::Sender<AgentEvent>,
}

/// The channels of all browser connections on one agent link.
#[derive(Debug)]
struct Routes {
    numbers: ChannelIdAllocator,
    /// By the number on the agent link.
    channels: HashMap<ChannelId, Route>,
    /// The browser connections that use the agent, with the queue to each one.
    connections: HashMap<ConnectionId, mpsc::Sender<Frame>>,
    /// Reports whether the agent stopped. Then the agent accepts no new channels.
    stopped: bool,
}

impl Default for Routes {
    fn default() -> Self {
        Self {
            // farglassd opens the channels of the shell, thus it uses the numbers of the shell.
            numbers: ChannelIdAllocator::new(Side::Shell),
            channels: HashMap::new(),
            connections: HashMap::new(),
            stopped: false,
        }
    }
}

/// One channel of a browser connection on the agent link.
#[derive(Debug)]
struct Route {
    connection: ConnectionId,
    /// The number of the channel on the browser connection.
    shell: ChannelId,
    closed_by_shell: bool,
    closed_by_agent: bool,
}

/// A frame and the agent link that it must go to.
#[derive(Debug)]
pub struct ToAgent {
    agent: Arc<Agent>,
    frame: Frame,
}

impl ToAgent {
    /// Puts the frame in the queue of the agent. Waits while the queue is full.
    ///
    /// The function is cancel safe: if it does not complete, the frame did not go.
    pub async fn send(&self) -> bool {
        match self.agent.to_agent.reserve().await {
            Ok(permit) => {
                permit.send(self.frame.clone());
                true
            }
            Err(_) => false,
        }
    }
}

impl Agent {
    /// Returns the uid of the user of the agent.
    #[must_use]
    pub const fn uid(&self) -> u32 {
        self.uid
    }

    /// Opens a channel of a browser connection on the agent link.
    ///
    /// `open` is the open frame from the shell. `to_shell` is the queue of the
    /// browser connection. Returns the frames for the agent: an attach message
    /// the first time that the connection uses the agent, and the open frame
    /// with the number on the agent link. Returns `None` if the agent stopped.
    pub fn open(
        self: &Arc<Self>,
        connection: ConnectionId,
        to_shell: &mpsc::Sender<Frame>,
        open: &Frame,
    ) -> Option<(ChannelId, Vec<ToAgent>)> {
        let mut routes = lock(&self.routes);
        if routes.stopped {
            return None;
        }
        let mut frames = Vec::with_capacity(2);
        if let Entry::Vacant(entry) = routes.connections.entry(connection) {
            entry.insert(to_shell.clone());
            frames.push(self.link_frame(&LinkMessage::Attach { connection })?);
        }
        let number = routes.numbers.allocate()?;
        routes.channels.insert(
            number,
            Route {
                connection,
                shell: open.channel(),
                closed_by_shell: false,
                closed_by_agent: false,
            },
        );
        frames.push(self.to(open.with_channel(number).ok()?));
        Some((number, frames))
    }

    /// Returns the frame of the shell for the agent link, with the number of the channel there.
    ///
    /// After the close frames of the two sides, the number is free.
    #[must_use]
    pub fn forward(self: &Arc<Self>, number: ChannelId, frame: &Frame) -> Option<ToAgent> {
        let moved = frame.with_channel(number).ok()?;
        if frame.frame_type() == FrameType::Close {
            let mut routes = lock(&self.routes);
            if let Some(route) = routes.channels.get_mut(&number) {
                route.closed_by_shell = true;
                if route.closed_by_agent {
                    routes.channels.remove(&number);
                }
            }
        }
        Some(self.to(moved))
    }

    /// Stops the use of the agent by a browser connection.
    ///
    /// Returns a close frame for each channel that the shell did not close,
    /// and the detach message. The connection gets no more frames from the agent.
    #[must_use]
    pub fn detach(self: &Arc<Self>, connection: ConnectionId) -> Vec<ToAgent> {
        let mut routes = lock(&self.routes);
        if routes.connections.remove(&connection).is_none() {
            return Vec::new();
        }
        let mut frames = Vec::new();
        let close = Close::new(CloseReason::Cancelled);
        routes.channels.retain(|number, route| {
            if route.connection != connection {
                return true;
            }
            if !route.closed_by_shell
                && let Ok(frame) = Frame::close(*number, &close)
            {
                frames.push(self.to(frame));
            }
            false
        });
        if let Some(frame) = self.link_frame(&LinkMessage::Detach { connection }) {
            frames.push(frame);
        }
        frames
    }

    fn to(self: &Arc<Self>, frame: Frame) -> ToAgent {
        ToAgent {
            agent: self.clone(),
            frame,
        }
    }

    /// Sends `message` to the agent in a separate task. A full queue does not delay the caller.
    pub fn tell(self: &Arc<Self>, message: &LinkMessage) {
        if let Some(frame) = self.link_frame(message) {
            tokio::spawn(async move {
                frame.send().await;
            });
        }
    }

    fn link_frame(self: &Arc<Self>, message: &LinkMessage) -> Option<ToAgent> {
        let frame = Frame::json(ChannelId::CONNECTION, FrameType::Control, message).ok()?;
        Some(self.to(frame))
    }

    /// Gives one frame from the agent to its browser connection.
    ///
    /// Returns `false` if the agent broke the protocol. Then the link stops.
    async fn deliver(&self, frame: Frame) -> bool {
        let number = frame.channel();
        if number.is_connection() {
            let event = match parse_link(&frame) {
                Some(LinkMessage::Lock) => AgentEvent::Lock(self.uid),
                Some(LinkMessage::Unlock) => AgentEvent::Unlock(self.uid),
                message => {
                    tracing::warn!(
                        uid = self.uid,
                        ?message,
                        "the agent sent a link message that only farglassd sends"
                    );
                    return false;
                }
            };
            let _ = self.events.send(event);
            return true;
        }
        let target = {
            let mut routes = lock(&self.routes);
            let Some(route) = routes.channels.get_mut(&number) else {
                if frame.frame_type() == FrameType::Open {
                    // A later version lets the agent open channels to a browser connection.
                    tracing::warn!(uid = self.uid, %number, "the agent tried to open a channel");
                    return false;
                }
                // A frame of a channel that the shell closed, or of a connection that stopped.
                return true;
            };
            let shell = route.shell;
            let connection = route.connection;
            if frame.frame_type() == FrameType::Close {
                route.closed_by_agent = true;
                if route.closed_by_shell {
                    routes.channels.remove(&number);
                }
            }
            routes
                .connections
                .get(&connection)
                .cloned()
                .map(|queue| (queue, shell))
        };
        if let Some((queue, shell)) = target
            && let Ok(moved) = frame.with_channel(shell)
        {
            // If the browser connection stopped, its queue is closed. Drop the frame.
            let _ = queue.send(moved).await;
        }
        true
    }

    /// Marks the agent as stopped and closes each open channel on the browser side.
    fn stop(&self, message: &str) {
        let mut routes = lock(&self.routes);
        routes.stopped = true;
        let close = Close {
            reason: CloseReason::InternalError,
            message: Some(message.to_owned()),
        };
        for route in routes
            .channels
            .values()
            .filter(|route| !route.closed_by_agent)
        {
            if let Some(queue) = routes.connections.get(&route.connection)
                && let Ok(frame) = Frame::close(route.shell, &close)
            {
                // The connection must learn the close. A full queue drops it,
                // and then the connection closes the channel when it detaches.
                let _ = queue.try_send(frame);
            }
        }
        routes.channels.clear();
        routes.connections.clear();
    }
}

/// Creates the agent socket. Each user can connect: the peer uid decides
/// which browser connections an agent gets.
///
/// An old socket at `path` is removed. A file of a different type stays, and the function fails.
///
/// # Errors
///
/// [`BindError`] if the socket cannot be created.
pub fn bind(path: &Path) -> Result<UnixListener, BindError> {
    let error = |source| BindError {
        path: path.to_owned(),
        source,
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(error)?;
    }
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_socket() => fs::remove_file(path).map_err(error)?,
        Ok(_) => {
            return Err(error(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the path exists and is not a socket",
            )));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(error(e)),
    }
    let listener = UnixListener::bind(path).map_err(error)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o666)).map_err(error)?;
    Ok(listener)
}

/// Reads the claim of a session worker (F057): one frame with [`LinkMessage::Claim`].
///
/// The function reads exactly the bytes of the frame: the hello of the agent that
/// follows stays in the stream. Returns the uid of the claim, never 0.
///
/// # Errors
///
/// A text with the cause if the frame is not a valid claim.
pub async fn read_claim<S: AsyncRead + Unpin>(stream: &mut S) -> Result<u32, String> {
    use tokio::io::AsyncReadExt as _;
    let mut head = [0_u8; farglass_proto::HEADER_LEN];
    stream
        .read_exact(&mut head)
        .await
        .map_err(|error| error.to_string())?;
    let header = farglass_proto::FrameHeader::decode(head).map_err(|error| error.to_string())?;
    if header.payload_len() > 256 {
        return Err("the claim is too long".to_owned());
    }
    let mut bytes = head.to_vec();
    bytes.resize(farglass_proto::HEADER_LEN + header.payload_len(), 0);
    stream
        .read_exact(&mut bytes[farglass_proto::HEADER_LEN..])
        .await
        .map_err(|error| error.to_string())?;
    let frame = Frame::from_message(bytes.into()).map_err(|error| error.to_string())?;
    match parse_link(&frame) {
        Some(LinkMessage::Claim { uid }) if uid != 0 => Ok(uid),
        other => Err(format!("the first message was {other:?}, not a claim")),
    }
}

/// Accepts agents on `listener` and serves each one in its own task.
///
/// A link from root must start with the claim of a session worker (F057). A link
/// from a different uid is the agent of that uid only with `allow_user_agents`,
/// for development and tests. Each uid has at most [`MAX_LINKS_PER_UID`] links
/// at the same time (F051).
pub async fn accept_loop(listener: UnixListener, registry: Arc<Registry>, allow_user_agents: bool) {
    let links: Arc<Mutex<HashMap<u32, usize>>> = Arc::default();
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(error) => {
                tracing::warn!(%error, "the agent socket did not accept a connection");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let peer = match stream.peer_cred() {
            Ok(cred) => cred.uid(),
            Err(error) => {
                tracing::warn!(%error, "the agent socket gave no peer credentials");
                continue;
            }
        };
        let event = |reason, uid| farglass_login::audit::Event {
            event: "agent",
            reason,
            uid: Some(uid),
            ..Default::default()
        };
        if peer != 0 && !allow_user_agents {
            event("not-from-worker", peer).refused();
            continue;
        }
        let Some(place) = LinkPlace::take(&links, peer) else {
            event("uid-cap", peer).refused();
            continue;
        };
        let registry = registry.clone();
        tokio::spawn(async move {
            let mut stream = stream;
            let uid = if peer == 0 {
                match tokio::time::timeout(HELLO_TIMEOUT, read_claim(&mut stream)).await {
                    Ok(Ok(uid)) => uid,
                    Ok(Err(error)) => {
                        farglass_login::audit::Event {
                            detail: Some(&error),
                            ..event("claim", 0)
                        }
                        .refused();
                        return;
                    }
                    Err(_) => {
                        event("claim-timeout", 0).refused();
                        return;
                    }
                }
            } else {
                peer
            };
            event("", uid).accepted();
            registry.serve(stream, uid).await;
            drop(place);
        });
    }
}

/// A place of a uid for one agent link. Dropping it frees the place.
struct LinkPlace {
    links: Arc<Mutex<HashMap<u32, usize>>>,
    uid: u32,
}

impl LinkPlace {
    fn take(links: &Arc<Mutex<HashMap<u32, usize>>>, uid: u32) -> Option<Self> {
        let mut counts = lock(links);
        let count = counts.entry(uid).or_insert(0);
        if *count >= MAX_LINKS_PER_UID {
            return None;
        }
        *count += 1;
        Some(Self {
            links: links.clone(),
            uid,
        })
    }
}

impl Drop for LinkPlace {
    fn drop(&mut self) {
        let mut counts = lock(&self.links);
        if let Some(count) = counts.get_mut(&self.uid) {
            *count -= 1;
            if *count == 0 {
                counts.remove(&self.uid);
            }
        }
    }
}

fn parse_link(frame: &Frame) -> Option<LinkMessage> {
    if !frame.channel().is_connection() {
        return None;
    }
    frame.parse().ok()
}

/// Locks `mutex`. A panic in another task does not make the data unusable here.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
