//! The polkit authentication agent of the session.
//!
//! The agent registers with polkit for its logind session. When a program of
//! the session asks for an action that needs an authentication, polkit calls
//! `BeginAuthentication`. The agent sends the request to the polkit channels
//! of the shell and relays the PAM conversation of `polkit-agent-helper-1`.
//! The helper tells polkit the result.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use farglass_proto::message::{PolkitEvent, PolkitReply};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, watch};
use zbus::Connection;
use zeroize::Zeroizing;

/// The helper of polkit that does the PAM conversation as root.
pub const HELPER: &str = "/usr/lib/polkit-1/polkit-agent-helper-1";

/// The object path of the agent on the system bus.
pub const AGENT_PATH: &str = "/org/farglass/PolkitAgent";

/// The longest time of one request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The id of one polkit channel.
pub type ViewerId = u64;

/// Why a request did not succeed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The user, polkit, or the end of all channels cancelled the request.
    Cancelled(String),
}

/// A command of the shell for a request.
#[derive(Debug)]
enum ShellCommand {
    Start(String),
    Answer(Zeroizing<String>),
    Cancel,
}

/// The function that gives an event to one polkit channel.
type EventSink = Arc<dyn Fn(PolkitEvent) + Send + Sync>;

#[derive(Default)]
struct Inner {
    viewers: HashMap<ViewerId, EventSink>,
    requests: HashMap<u32, mpsc::UnboundedSender<ShellCommand>>,
    cookies: HashMap<String, u32>,
}

/// The polkit agent: the open channels of the shell and the running requests.
pub struct Polkit {
    helper: PathBuf,
    inner: Mutex<Inner>,
    next_viewer: AtomicU64,
    next_request: AtomicU32,
    /// The number of open channels.
    viewers: watch::Sender<usize>,
}

impl std::fmt::Debug for Polkit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Polkit")
            .field("helper", &self.helper)
            .finish_non_exhaustive()
    }
}

/// One open polkit channel. Dropping it removes the channel.
pub struct ViewerGuard {
    polkit: Arc<Polkit>,
    id: ViewerId,
}

impl std::fmt::Debug for ViewerGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerGuard").field("id", &self.id).finish()
    }
}

impl Drop for ViewerGuard {
    fn drop(&mut self) {
        let count = {
            let mut inner = self.polkit.lock();
            inner.viewers.remove(&self.id);
            inner.viewers.len()
        };
        self.polkit.viewers.send_replace(count);
    }
}

impl Polkit {
    /// Returns an agent that runs `helper` for each attempt.
    #[must_use]
    pub fn new(helper: impl Into<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            helper: helper.into(),
            inner: Mutex::new(Inner::default()),
            next_viewer: AtomicU64::new(1),
            next_request: AtomicU32::new(1),
            viewers: watch::Sender::new(0),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds a polkit channel. `send` gets each event. Dropping the guard removes the channel.
    pub fn add_viewer(
        self: &Arc<Self>,
        send: impl Fn(PolkitEvent) + Send + Sync + 'static,
    ) -> ViewerGuard {
        let id = self.next_viewer.fetch_add(1, Ordering::Relaxed);
        let count = {
            let mut inner = self.lock();
            inner.viewers.insert(id, Arc::new(send));
            inner.viewers.len()
        };
        self.viewers.send_replace(count);
        ViewerGuard {
            polkit: self.clone(),
            id,
        }
    }

    /// Sends `event` to each open channel.
    fn broadcast(&self, event: &PolkitEvent) {
        let viewers: Vec<EventSink> = self.lock().viewers.values().cloned().collect();
        for send in viewers {
            send(event.clone());
        }
    }

    /// Gives a reply of the shell to its request.
    pub fn reply(&self, reply: PolkitReply) {
        let (request, command) = match reply {
            PolkitReply::Start { request, identity } => (request, ShellCommand::Start(identity)),
            PolkitReply::Answer { request, text } => (request, ShellCommand::Answer(text)),
            PolkitReply::Cancel { request } => (request, ShellCommand::Cancel),
        };
        if let Some(sender) = self.lock().requests.get(&request) {
            let _ = sender.send(command);
        }
    }

    /// Cancels the request of `cookie`, when polkit calls CancelAuthentication.
    pub fn cancel_cookie(&self, cookie: &str) {
        let inner = self.lock();
        if let Some(sender) = inner
            .cookies
            .get(cookie)
            .and_then(|id| inner.requests.get(id))
        {
            let _ = sender.send(ShellCommand::Cancel);
        }
    }

    /// Runs one request until the authentication succeeds or the request is cancelled.
    ///
    /// # Errors
    ///
    /// [`Outcome::Cancelled`] if no channel is open, if the user or polkit cancels,
    /// if the last channel closes, or after [`REQUEST_TIMEOUT`].
    pub async fn authenticate(
        self: &Arc<Self>,
        action: &str,
        message: &str,
        identities: Vec<String>,
        cookie: &str,
    ) -> Result<(), Outcome> {
        let Some(first) = default_identity(&identities) else {
            return Err(Outcome::Cancelled("polkit gave no identity".to_owned()));
        };
        let mut viewers = self.viewers.subscribe();
        if *viewers.borrow_and_update() == 0 {
            return Err(Outcome::Cancelled(
                "no client of the session is connected".to_owned(),
            ));
        }
        let request = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (sender, mut commands) = mpsc::unbounded_channel();
        {
            let mut inner = self.lock();
            inner.requests.insert(request, sender);
            inner.cookies.insert(cookie.to_owned(), request);
        }
        self.broadcast(&PolkitEvent::Begin {
            request,
            action: action.to_owned(),
            message: message.to_owned(),
            identities: identities.clone(),
            identity: first.clone(),
        });

        let result = self
            .run(
                request,
                cookie,
                &identities,
                first,
                &mut commands,
                &mut viewers,
            )
            .await;
        {
            let mut inner = self.lock();
            inner.requests.remove(&request);
            inner.cookies.remove(cookie);
        }
        self.broadcast(&PolkitEvent::Done {
            request,
            success: result.is_ok(),
        });
        result
    }

    async fn run(
        &self,
        request: u32,
        cookie: &str,
        identities: &[String],
        first: String,
        commands: &mut mpsc::UnboundedReceiver<ShellCommand>,
        viewers: &mut watch::Receiver<usize>,
    ) -> Result<(), Outcome> {
        let cancelled = |why: &str| Err(Outcome::Cancelled(why.to_owned()));
        let mut attempt = Attempt::start(&self.helper, &first, cookie).await.ok();
        let timeout = tokio::time::sleep(REQUEST_TIMEOUT);
        tokio::pin!(timeout);
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(ShellCommand::Start(identity)) if identities.contains(&identity) => {
                        attempt = Attempt::start(&self.helper, &identity, cookie).await.ok();
                    }
                    Some(ShellCommand::Start(_)) => {}
                    Some(ShellCommand::Answer(text)) => {
                        if let Some(current) = &mut attempt {
                            current.answer(&text).await;
                        }
                    }
                    Some(ShellCommand::Cancel) | None => return cancelled("cancelled"),
                },
                event = next_event(&mut attempt) => match event {
                    HelperEvent::Prompt { text, echo } => self.broadcast(&PolkitEvent::Prompt { request, text, echo }),
                    HelperEvent::Info(text) => self.broadcast(&PolkitEvent::Info { request, text }),
                    HelperEvent::Success => return Ok(()),
                    HelperEvent::Failure => {
                        attempt = None;
                        self.broadcast(&PolkitEvent::Failed { request });
                    }
                },
                changed = viewers.changed() => {
                    if changed.is_err() || *viewers.borrow_and_update() == 0 {
                        return cancelled("the last client of the session closed");
                    }
                }
                () = &mut timeout => return cancelled("the request took too long"),
            }
        }
    }
}

/// Returns the user of the first attempt: the user of the agent, if polkit permits it, else the first identity.
fn default_identity(identities: &[String]) -> Option<String> {
    let me = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .map(|user| user.name);
    me.filter(|name| identities.contains(name))
        .or_else(|| identities.first().cloned())
}

/// What the helper says.
#[derive(Debug, PartialEq, Eq)]
enum HelperEvent {
    Prompt { text: String, echo: bool },
    Info(String),
    Success,
    Failure,
}

/// Parses one line of the helper. An unknown line or the end gives a failure.
fn parse_line(line: Option<&str>) -> HelperEvent {
    let Some(line) = line else {
        return HelperEvent::Failure;
    };
    if let Some(text) = line.strip_prefix("PAM_PROMPT_ECHO_OFF ") {
        return HelperEvent::Prompt {
            text: text.to_owned(),
            echo: false,
        };
    }
    if let Some(text) = line.strip_prefix("PAM_PROMPT_ECHO_ON ") {
        return HelperEvent::Prompt {
            text: text.to_owned(),
            echo: true,
        };
    }
    if let Some(text) = line
        .strip_prefix("PAM_ERROR_MSG ")
        .or_else(|| line.strip_prefix("PAM_TEXT_INFO "))
    {
        return HelperEvent::Info(text.to_owned());
    }
    match line {
        "SUCCESS" => HelperEvent::Success,
        _ => HelperEvent::Failure,
    }
}

/// One run of the helper for one identity.
struct Attempt {
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Attempt {
    /// Starts the helper for `identity` and gives it the cookie of the request.
    async fn start(
        helper: &std::path::Path,
        identity: &str,
        cookie: &str,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(helper)
            .arg(identity)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("the helper has no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("the helper has no stdout"))?;
        // The helper reads the cookie from its first line.
        stdin.write_all(format!("{cookie}\n").as_bytes()).await?;
        stdin.flush().await?;
        Ok(Self {
            _child: child,
            stdin,
            lines: BufReader::new(stdout).lines(),
        })
    }

    async fn answer(&mut self, text: &str) {
        // A new line in the answer would start a second answer.
        let line = Zeroizing::new(format!("{}\n", text.replace('\n', " ")));
        let _ = self.stdin.write_all(line.as_bytes()).await;
        let _ = self.stdin.flush().await;
    }
}

async fn next_event(attempt: &mut Option<Attempt>) -> HelperEvent {
    match attempt {
        Some(current) => {
            let line = current.lines.next_line().await.ok().flatten();
            parse_line(line.as_deref())
        }
        None => std::future::pending().await,
    }
}

/// The errors of the agent object for polkit.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
pub enum AgentError {
    /// An error of zbus.
    #[zbus(error)]
    ZBus(zbus::Error),
    /// The request did not succeed.
    Cancelled(String),
    /// The caller is not polkit.
    NotAuthorized(String),
}

/// The object `org.freedesktop.PolicyKit1.AuthenticationAgent` on the system bus.
struct AgentObject {
    polkit: Arc<Polkit>,
    connection: Connection,
}

impl AgentObject {
    /// Refuses a caller that is not polkit. A different process could show a prompt to get a password.
    async fn check_caller(&self, header: &zbus::message::Header<'_>) -> Result<(), AgentError> {
        let owner: String = self
            .connection
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetNameOwner",
                &("org.freedesktop.PolicyKit1",),
            )
            .await?
            .body()
            .deserialize()?;
        match header.sender() {
            Some(sender) if sender.as_str() == owner => Ok(()),
            other => {
                tracing::warn!(caller = ?other, "a caller that is not polkit tried to start an authentication");
                Err(AgentError::NotAuthorized(
                    "Only polkit can call the authentication agent.".to_owned(),
                ))
            }
        }
    }
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl AgentObject {
    #[allow(clippy::too_many_arguments)]
    async fn begin_authentication(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        action_id: String,
        message: String,
        _icon_name: String,
        _details: HashMap<String, String>,
        cookie: String,
        identities: Vec<(String, HashMap<String, zbus::OwnedVariant>)>,
    ) -> Result<(), AgentError> {
        self.check_caller(&header).await?;
        let names = identity_names(&identities);
        tracing::info!(action = action_id, identities = ?names, "polkit asks for an authentication");
        self.polkit
            .authenticate(&action_id, &message, names, &cookie)
            .await
            .map_err(|Outcome::Cancelled(why)| AgentError::Cancelled(why))
    }

    async fn cancel_authentication(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        cookie: String,
    ) -> Result<(), AgentError> {
        self.check_caller(&header).await?;
        self.polkit.cancel_cookie(&cookie);
        Ok(())
    }
}

/// Returns the usernames of the identities of polkit: each user, and each member of each group.
fn identity_names(identities: &[(String, HashMap<String, zbus::OwnedVariant>)]) -> Vec<String> {
    let mut names = Vec::new();
    for (kind, details) in identities {
        let id = |key: &str| {
            details
                .get(key)
                .and_then(|value| u32::try_from(&**value).ok())
        };
        let found: Vec<String> = match kind.as_str() {
            "unix-user" => id("uid")
                .and_then(|uid| nix::unistd::User::from_uid(uid.into()).ok().flatten())
                .map(|user| vec![user.name])
                .unwrap_or_default(),
            "unix-group" => id("gid")
                .and_then(|gid| nix::unistd::Group::from_gid(gid.into()).ok().flatten())
                .map(|group| group.mem)
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        for name in found {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Registers the agent with polkit for the logind session `session_id` on the system bus.
///
/// # Errors
///
/// A zbus error if polkit refuses the agent, for example because the session has an agent.
pub async fn register(
    polkit: Arc<Polkit>,
    connection: &Connection,
    session_id: &str,
) -> zbus::Result<()> {
    let object = AgentObject {
        polkit,
        connection: connection.clone(),
    };
    connection.object_server().at(AGENT_PATH, object).await?;
    let subject = (
        "unix-session",
        HashMap::from([("session-id", zbus::Variant::from(session_id))]),
    );
    let locale = std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".to_owned());
    connection
        .call_method(
            Some("org.freedesktop.PolicyKit1"),
            "/org/freedesktop/PolicyKit1/Authority",
            Some("org.freedesktop.PolicyKit1.Authority"),
            "RegisterAuthenticationAgent",
            &(subject, locale.as_str(), AGENT_PATH),
        )
        .await?;
    tracing::info!(
        session = session_id,
        "registered as the polkit authentication agent"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_lines_give_events() {
        assert_eq!(
            parse_line(Some("PAM_PROMPT_ECHO_OFF Password: ")),
            HelperEvent::Prompt {
                text: "Password: ".to_owned(),
                echo: false
            }
        );
        assert_eq!(
            parse_line(Some("PAM_PROMPT_ECHO_ON login:")),
            HelperEvent::Prompt {
                text: "login:".to_owned(),
                echo: true
            }
        );
        assert_eq!(
            parse_line(Some("PAM_TEXT_INFO Touch the key")),
            HelperEvent::Info("Touch the key".to_owned())
        );
        assert_eq!(
            parse_line(Some("PAM_ERROR_MSG Locked")),
            HelperEvent::Info("Locked".to_owned())
        );
        assert_eq!(parse_line(Some("SUCCESS")), HelperEvent::Success);
        assert_eq!(parse_line(Some("FAILURE")), HelperEvent::Failure);
        assert_eq!(parse_line(Some("something else")), HelperEvent::Failure);
        assert_eq!(parse_line(None), HelperEvent::Failure);
    }

    #[test]
    fn identities_give_users_and_group_members_once() {
        let uid = |n: u32| {
            HashMap::from([(
                "uid".to_owned(),
                zbus::OwnedVariant::try_from(zbus::Variant::from(n)).unwrap(),
            )])
        };
        let names = identity_names(&[
            ("unix-user".to_owned(), uid(0)),
            ("unix-user".to_owned(), uid(0)),
        ]);
        assert_eq!(names, ["root"]);
        assert!(identity_names(&[("unix-netgroup".to_owned(), HashMap::new())]).is_empty());
    }
}
