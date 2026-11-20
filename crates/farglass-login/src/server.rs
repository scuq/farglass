//! The login service: one connection is one login attempt.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use farglass_proto::codec::FrameCodec;
use farglass_proto::login::{LoginEvent, LoginRequest};
use farglass_proto::{ChannelId, Frame, FrameType};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, mpsc};
use tokio_util::codec::Framed;
use zeroize::Zeroizing;

use crate::audit::{self, Event};
use crate::host::{Host, NoSessions, Started};
use crate::limiter::{Key, Limiter};
use crate::pam::{self, Conversation, Login};

/// farglassd must send the start message in this time.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// The whole login, from the start message to the session steps, must end in this time (F051).
pub const LOGIN_DEADLINE: Duration = Duration::from_secs(60);

/// The message for each failure that can tell if a user exists.
const FAILURE_MESSAGE: &str = "The login failed.";

/// The settings of the login service.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The PAM service.
    pub service: String,
    /// A directory that replaces `/etc/pam.d`, for tests.
    pub pam_dir: Option<PathBuf>,
    /// Reports whether root can log in.
    pub allow_root: bool,
    /// The wait after a failed attempt, before the answer. It makes guesses slow.
    pub fail_delay: Duration,
    /// The largest number of attempts at the same time, with the reserved places.
    pub max_attempts: usize,
    /// The places of `max_attempts` that only a browser with a valid device cookie can use (F051).
    pub reserved_attempts: usize,
    /// The largest number of attempts at the same time from one address, IPv6 grouped by /64.
    pub max_per_address: usize,
    /// The time for the whole login. A login that is not complete in this time fails.
    pub login_deadline: Duration,
    /// Only members of this group can log in (F058). `None` lets each user log in, for tests.
    pub allowed_group: Option<String>,
    /// More uids that can use the login socket, besides root, the user farglass, and the own user (F056).
    pub peer_uids: Vec<u32>,
    /// The actions on the host: the logind sessions and the start of the web session.
    pub host: Arc<dyn Host>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            service: "farglass".to_owned(),
            pam_dir: None,
            allow_root: false,
            fail_delay: Duration::from_secs(2),
            max_attempts: 32,
            reserved_attempts: 8,
            max_per_address: 4,
            login_deadline: LOGIN_DEADLINE,
            allowed_group: None,
            peer_uids: Vec::new(),
            host: Arc::new(NoSessions),
        }
    }
}

/// The login service.
#[derive(Debug)]
pub struct Service {
    settings: Settings,
    limiter: Arc<Limiter>,
    /// The places that each browser can use.
    open: Arc<Semaphore>,
    /// The places for browsers with a valid device cookie.
    reserved: Arc<Semaphore>,
    /// The running attempts of each address group.
    per_address: Arc<Mutex<HashMap<String, usize>>>,
}

/// A place of an address group. Dropping it frees the place.
struct AddressPlace {
    map: Arc<Mutex<HashMap<String, usize>>>,
    group: String,
}

impl AddressPlace {
    /// Takes a place for `group`, or returns `None` if the group has `max` places already.
    fn take(map: &Arc<Mutex<HashMap<String, usize>>>, group: &str, max: usize) -> Option<Self> {
        let mut counts = map.lock().unwrap_or_else(PoisonError::into_inner);
        let count = counts.entry(group.to_owned()).or_insert(0);
        if *count >= max {
            return None;
        }
        *count += 1;
        Some(Self {
            map: map.clone(),
            group: group.to_owned(),
        })
    }
}

impl Drop for AddressPlace {
    fn drop(&mut self) {
        let mut counts = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = counts.get_mut(&self.group) {
            *count -= 1;
            if *count == 0 {
                counts.remove(&self.group);
            }
        }
    }
}

/// What the PAM conversation learned: the username that the user typed, and a block of that user.
#[derive(Debug, Default)]
struct Seen {
    name: Option<String>,
    blocked: Option<Duration>,
}

/// Logs a security event of a login.
///
/// The username goes into the log only if the host has that account (F062): a password
/// typed in the username field must not go into the journal. Other names give `user=?`.
fn log_event<'a>(reason: &'a str, addr: &'a str, user: Option<&'a str>) -> Event<'a> {
    Event {
        event: "login",
        reason,
        addr,
        user: user.map(|name| if account_exists(name) { name } else { "?" }),
        ..Event::default()
    }
}

/// Reports whether the host has the account `name`. A read error gives `false`: the name stays out of the log.
fn account_exists(name: &str) -> bool {
    matches!(nix::unistd::User::from_name(name), Ok(Some(_)))
}

impl Service {
    /// Returns a service with `settings`.
    #[must_use]
    pub fn new(settings: Settings) -> Arc<Self> {
        let reserved = settings.reserved_attempts.min(settings.max_attempts);
        Arc::new(Self {
            open: Arc::new(Semaphore::new(settings.max_attempts - reserved)),
            reserved: Arc::new(Semaphore::new(reserved)),
            per_address: Arc::default(),
            settings,
            limiter: Arc::default(),
        })
    }

    /// Accepts connections on `listener` and serves each one in its own task.
    pub async fn run(self: Arc<Self>, listener: UnixListener) {
        loop {
            let stream = match listener.accept().await {
                Ok((stream, _)) => stream,
                Err(error) => {
                    tracing::warn!(%error, "the login socket did not accept a connection");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let service = self.clone();
            tokio::spawn(async move {
                if let Err(error) = service.serve(stream).await {
                    tracing::debug!(%error, "a login connection failed");
                }
            });
        }
    }

    /// Serves one login attempt.
    async fn serve(
        &self,
        stream: UnixStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let peer = stream.peer_cred().map(|cred| cred.uid()).ok();
        // Only farglassd connects: root, the user farglass, or the user of farglass-login (F056).
        if !peer.is_some_and(|uid| peer_allowed(uid) || self.settings.peer_uids.contains(&uid)) {
            Event {
                event: "login-socket",
                reason: "peer",
                uid: peer,
                ..Event::default()
            }
            .refused();
            return Ok(());
        }
        let mut link = Framed::new(stream, FrameCodec);

        let start = tokio::time::timeout(START_TIMEOUT, link.next())
            .await
            .map_err(|_| "no start message")?
            .ok_or("the connection closed before the start message")??;
        let (user, rhost, device, check) = match request(&start)? {
            // A password check needs the user: farglassd gives the user of the connection.
            LoginRequest::Start {
                check: true,
                user: None,
                ..
            } => {
                return Err("a password check with no user".into());
            }
            LoginRequest::Start {
                user,
                rhost,
                device,
                check,
            } => (user, rhost, device, check),
            LoginRequest::Logout { uid } => return self.logout(&mut link, uid, peer).await,
            LoginRequest::Member { uid } => {
                let allowed = self.uid_allowed(uid);
                return send(&mut link, &LoginEvent::Member { allowed }).await;
            }
            _ => return Err("the first message was not a start message".into()),
        };
        let group = audit::address_group(&rhost);
        let address = Key::Address(group.clone());
        let user_key = user.clone().map(Key::User);

        if let Some(left) = self.limiter.blocked(&address) {
            log_event("blocked-address", &rhost, user.as_deref()).refused();
            return blocked(&mut link, left).await;
        }
        if let Some(left) = user_key.as_ref().and_then(|key| self.limiter.blocked(key)) {
            log_event("blocked-user", &rhost, user.as_deref()).refused();
            return blocked(&mut link, left).await;
        }
        let Some(_place) =
            AddressPlace::take(&self.per_address, &group, self.settings.max_per_address)
        else {
            log_event("address-cap", &rhost, user.as_deref()).refused();
            let message =
                "There are too many logins from your address at the same time. Try again later."
                    .to_owned();
            return send(&mut link, &LoginEvent::Failure { message }).await;
        };
        let permit = match self.open.clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) if device => self.reserved.clone().try_acquire_owned().ok(),
            Err(_) => None,
        };
        let Some(_permit) = permit else {
            log_event("login-pool-full", &rhost, user.as_deref()).refused();
            let message =
                "The server has too many logins at the same time. Try again later.".to_owned();
            return send(&mut link, &LoginEvent::Failure { message }).await;
        };
        let deadline = Instant::now() + self.settings.login_deadline;

        let (events, mut from_pam) = mpsc::unbounded_channel();
        let (answers, to_pam) = std_mpsc::channel();
        let seen = Arc::new(Mutex::new(Seen::default()));
        let settings = self.settings.clone();
        let pam_user = user.clone();
        let pam_rhost = rhost.clone();
        let bridge = Bridge {
            events,
            answers: to_pam,
            next_prompt: 0,
            deadline,
            limiter: self.limiter.clone(),
            seen: seen.clone(),
            last_echo: None,
        };
        let pam = tokio::task::spawn_blocking(move || {
            let mut bridge = bridge;
            let params = Login {
                service: &settings.service,
                confdir: settings.pam_dir.as_deref(),
                user: pam_user.as_deref(),
                rhost: &pam_rhost,
            };
            pam::login(params, &mut bridge)
        });

        // Relay the conversation until PAM completes, or until the deadline.
        let mut answered = false;
        let relay = async {
            loop {
                tokio::select! {
                    event = from_pam.recv() => match event {
                        Some(event) => send(&mut link, &event).await?,
                        // The PAM thread stopped. Its result follows.
                        None => return Ok(true),
                    },
                    frame = link.next() => match frame {
                        Some(Ok(frame)) => match request(&frame)? {
                            LoginRequest::Answer { prompt, text } => {
                                answered = true;
                                let _ = answers.send((prompt, text));
                            }
                            LoginRequest::Start { .. }
                            | LoginRequest::Logout { .. }
                            | LoginRequest::Member { .. } => {
                                return Err("a second start message".into());
                            }
                            // A confirm or a second factor before the question. Ignore it.
                            LoginRequest::Confirm { .. } | LoginRequest::SecondFactor { .. } => {}
                        },
                        // farglassd stopped the attempt.
                        _ => return Ok(false),
                    },
                }
            }
        };
        let relayed: Result<bool, Box<dyn std::error::Error + Send + Sync>> =
            match tokio::time::timeout_at(deadline.into(), relay).await {
                Ok(result) => result,
                Err(_) => {
                    // Dropping `answers` stops the PAM conversation.
                    drop(answers);
                    let _ = pam.await;
                    let name = lock(&seen).name.clone().or(user);
                    self.count_failure(&address, name.as_deref());
                    log_event("login-deadline", &rhost, name.as_deref()).refused();
                    let message = "The login took too long. Try again.".to_owned();
                    return send(&mut link, &LoginEvent::Failure { message }).await;
                }
            };
        if !relayed? {
            // farglassd stopped the attempt. Dropping `answers` stops the PAM conversation.
            drop(answers);
            let _ = pam.await;
            if answered {
                // An attempt that got an answer and then stopped counts as a guess (F051).
                let name = lock(&seen).name.clone().or(user);
                self.count_failure(&address, name.as_deref());
                log_event("abandoned", &rhost, name.as_deref()).refused();
            }
            return Ok(());
        }

        let result = pam.await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                // A panic of the PAM thread is a refusal, never a success (ADR-042).
                Event {
                    detail: Some(&error.to_string()),
                    ..log_event("pam-thread", &rhost, user.as_deref())
                }
                .fault();
                tokio::time::sleep(self.settings.fail_delay).await;
                let message = FAILURE_MESSAGE.to_owned();
                return send(&mut link, &LoginEvent::Failure { message }).await;
            }
        };
        match result {
            Ok(name) => {
                self.succeeded(&mut link, &address, name, &rhost, deadline, check)
                    .await
            }
            Err(failure) => {
                let name = failure.user.clone().or(user);
                self.count_failure(&address, name.as_deref());
                // The conversation stopped at the deadline: PAM failed for no answer.
                if Instant::now() >= deadline {
                    log_event("login-deadline", &rhost, name.as_deref()).refused();
                    let message = "The login took too long. Try again.".to_owned();
                    return send(&mut link, &LoginEvent::Failure { message }).await;
                }
                let user_blocked = lock(&seen).blocked;
                if let Some(left) = user_blocked {
                    log_event("blocked-user", &rhost, name.as_deref()).refused();
                    return blocked(&mut link, left).await;
                }
                let reason = if failure.account {
                    "account"
                } else {
                    "wrong-password"
                };
                Event {
                    detail: Some(&failure.reason),
                    ..log_event(reason, &rhost, name.as_deref())
                }
                .refused();
                tokio::time::sleep(self.settings.fail_delay).await;
                let message = if failure.account {
                    failure.reason
                } else {
                    FAILURE_MESSAGE.to_owned()
                };
                send(&mut link, &LoginEvent::Failure { message }).await
            }
        }
    }

    /// Reports whether the user `name` with the primary group `gid` is in the allowed group (F058).
    ///
    /// A fault, for example a read error of the group database, gives `false` (ADR-042).
    fn allowed(&self, name: &str, gid: nix::unistd::Gid, addr: &str) -> bool {
        let Some(group) = &self.settings.allowed_group else {
            return true;
        };
        match membership(group, name, gid) {
            Ok(true) => true,
            Ok(false) => {
                log_event("not-in-group", addr, Some(name)).refused();
                false
            }
            Err(error) => {
                Event {
                    detail: Some(&error),
                    ..log_event("group-check", addr, Some(name))
                }
                .fault();
                false
            }
        }
    }

    /// Reports whether the user `uid` is in the allowed group, for a reconnect with a session cookie.
    fn uid_allowed(&self, uid: u32) -> bool {
        if self.settings.allowed_group.is_none() {
            return true;
        }
        match nix::unistd::User::from_uid(uid.into()) {
            Ok(Some(user)) => {
                let allowed = self.allowed(&user.name, user.gid, "");
                if !allowed {
                    Event {
                        event: "session",
                        reason: "not-in-group",
                        user: Some(&user.name),
                        uid: Some(uid),
                        ..Event::default()
                    }
                    .refused();
                }
                allowed
            }
            other => {
                let detail = match other {
                    Err(error) => error.to_string(),
                    _ => "no account".to_owned(),
                };
                Event {
                    event: "session",
                    reason: "group-check",
                    uid: Some(uid),
                    detail: Some(&detail),
                    ..Event::default()
                }
                .fault();
                false
            }
        }
    }

    /// Counts a failed or abandoned attempt for the address and, if known, the user.
    fn count_failure(&self, address: &Key, user: Option<&str>) {
        self.limiter.failed(address.clone());
        if let Some(name) = user {
            self.limiter.failed(Key::User(name.to_owned()));
        }
    }

    async fn succeeded(
        &self,
        link: &mut Framed<UnixStream, FrameCodec>,
        address: &Key,
        name: String,
        rhost: &str,
        deadline: Instant,
        check: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // A correct password does not end a block of the user (F051).
        if let Some(left) = self.limiter.blocked(&Key::User(name.clone())) {
            self.count_failure(address, Some(&name));
            log_event("blocked-user", rhost, Some(&name)).refused();
            return blocked(link, left).await;
        }
        let (uid, gid) = match nix::unistd::User::from_name(&name) {
            Ok(Some(user)) => (user.uid.as_raw(), user.gid),
            other => {
                // A read error of the user database is a refusal (ADR-042).
                let detail = match other {
                    Err(error) => error.to_string(),
                    _ => "PAM accepted a user that has no account on the host".to_owned(),
                };
                Event {
                    detail: Some(&detail),
                    ..log_event("no-account", rhost, Some(&name))
                }
                .fault();
                tokio::time::sleep(self.settings.fail_delay).await;
                let message = FAILURE_MESSAGE.to_owned();
                return send(link, &LoginEvent::Failure { message }).await;
            }
        };
        if uid == 0 && !self.settings.allow_root {
            log_event("root", rhost, Some(&name)).refused();
            self.limiter.failed(address.clone());
            tokio::time::sleep(self.settings.fail_delay).await;
            let message = "The login as root is not permitted.".to_owned();
            return send(link, &LoginEvent::Failure { message }).await;
        }
        // A user outside the allowed group fails as a wrong password does (F058).
        // The login never reaches the passkey step, thus nothing shows the membership.
        if !self.allowed(&name, gid, rhost) {
            self.count_failure(address, Some(&name));
            tokio::time::sleep(self.settings.fail_delay).await;
            let message = FAILURE_MESSAGE.to_owned();
            return send(link, &LoginEvent::Failure { message }).await;
        }
        // A password check ends here: no second factor and no session (F061).
        if check {
            self.limiter.succeeded(address);
            self.limiter.succeeded(&Key::User(name.clone()));
            Event {
                event: "password-check",
                uid: Some(uid),
                ..log_event("", rhost, Some(&name))
            }
            .accepted();
            return send(
                link,
                &LoginEvent::Success {
                    user: name,
                    uid,
                    ticket: None,
                },
            )
            .await;
        }
        // farglassd checks the passkey of the user, if the user has one (F034).
        // A wrong passkey counts as a failed login.
        send(
            link,
            &LoginEvent::Verified {
                user: name.clone(),
                uid,
            },
        )
        .await?;
        if !self.second_factor(link, deadline).await? {
            self.count_failure(address, Some(&name));
            Event {
                uid: Some(uid),
                ..log_event("second-factor", rhost, Some(&name))
            }
            .refused();
            tokio::time::sleep(self.settings.fail_delay).await;
            let message = "The passkey or the recovery code was not accepted.".to_owned();
            return send(link, &LoginEvent::Failure { message }).await;
        }
        // The password is correct. Reset the counters before the session steps,
        // because a decline or an error of the host is not a guess.
        self.limiter.succeeded(address);
        self.limiter.succeeded(&Key::User(name.clone()));
        Event {
            uid: Some(uid),
            ..log_event("", rhost, Some(&name))
        }
        .accepted();

        // One user has one graphical session at a time (ADR-015).
        let host = self.settings.host.clone();
        let local = match blocking(move || host.local_sessions(uid)).await {
            Ok(local) => local,
            Err(error) => {
                tracing::error!(user = name, %error, "logind did not give the sessions of the user");
                return session_failed(link).await;
            }
        };
        if !local.is_empty() {
            send(
                link,
                &LoginEvent::Confirm {
                    sessions: local.clone(),
                },
            )
            .await?;
            if !self.confirmed(link, deadline).await? {
                tracing::info!(user = name, rhost, "the user kept the local session");
                let message = "The login stopped. The local session continues.".to_owned();
                return send(link, &LoginEvent::Failure { message }).await;
            }
            let ids: Vec<String> = local.into_iter().map(|session| session.id).collect();
            tracing::info!(user = name, rhost, ?ids, "the user ends the local sessions");
            let text = "The local session ends.".to_owned();
            send(link, &LoginEvent::Info { text }).await?;
            let host = self.settings.host.clone();
            if let Err(error) = blocking(move || host.end_sessions(&ids)).await {
                tracing::error!(user = name, %error, "the local sessions did not end");
                return session_failed(link).await;
            }
        }

        let host = self.settings.host.clone();
        let user = name.clone();
        match blocking(move || host.start_session(&user, uid)).await {
            Ok(Started::New) => {
                tracing::info!(rhost, user = name, uid, "a login started a new web session")
            }
            Ok(Started::Attached) => tracing::info!(
                rhost,
                user = name,
                uid,
                "a login attached to the web session"
            ),
            Err(error) => {
                tracing::error!(user = name, %error, "the web session did not start");
                return session_failed(link).await;
            }
        }
        send(
            link,
            &LoginEvent::Success {
                user: name,
                uid,
                ticket: None,
            },
        )
        .await
    }

    /// Ends the web sessions of `uid`. Local sessions continue.
    async fn logout(
        &self,
        link: &mut Framed<UnixStream, FrameCodec>,
        uid: u32,
        peer: Option<u32>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let host = self.settings.host.clone();
        match blocking(move || host.end_web_sessions(uid)).await {
            Ok(sessions) => {
                tracing::info!(uid, sessions, ?peer, "logout");
                send(link, &LoginEvent::LoggedOut { sessions }).await
            }
            Err(error) => {
                tracing::error!(uid, %error, "the web sessions did not end");
                let message = "The session did not end. Tell the administrator.".to_owned();
                send(link, &LoginEvent::Failure { message }).await
            }
        }
    }

    /// Waits for the answer of farglassd to [`LoginEvent::Verified`]. Returns `false` for a
    /// failed second factor, a timeout, or a closed connection.
    async fn second_factor(
        &self,
        link: &mut Framed<UnixStream, FrameCodec>,
        deadline: Instant,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let wait = async {
            while let Some(frame) = link.next().await {
                if let LoginRequest::SecondFactor { accept } = request(&frame?)? {
                    return Ok(accept);
                }
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(false)
        };
        tokio::time::timeout_at(deadline.into(), wait)
            .await
            .unwrap_or(Ok(false))
    }

    /// Waits for the answer to a confirm event. Returns `false` for a decline, a timeout,
    /// or a closed connection.
    async fn confirmed(
        &self,
        link: &mut Framed<UnixStream, FrameCodec>,
        deadline: Instant,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let wait = async {
            while let Some(frame) = link.next().await {
                if let LoginRequest::Confirm { accept } = request(&frame?)? {
                    return Ok(accept);
                }
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(false)
        };
        tokio::time::timeout_at(deadline.into(), wait)
            .await
            .unwrap_or(Ok(false))
    }
}

/// Runs a blocking host action in a separate thread.
async fn blocking<T: Send + 'static>(
    action: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    tokio::task::spawn_blocking(action)
        .await
        .unwrap_or_else(|error| Err(std::io::Error::other(error)))
}

async fn session_failed(
    link: &mut Framed<UnixStream, FrameCodec>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let message = "The password is correct, but the session did not start. Tell the administrator."
        .to_owned();
    send(link, &LoginEvent::Failure { message }).await
}

/// The conversation of one attempt: each prompt goes to farglassd, and the answer comes back.
struct Bridge {
    events: mpsc::UnboundedSender<LoginEvent>,
    answers: std_mpsc::Receiver<(u32, Zeroizing<String>)>,
    next_prompt: u32,
    /// The end of the whole login.
    deadline: Instant,
    limiter: Arc<Limiter>,
    /// The username that the user typed, and a block of it.
    seen: Arc<Mutex<Seen>>,
    /// The last answer to a prompt with echo: the username for the next password prompt.
    last_echo: Option<String>,
}

impl Conversation for Bridge {
    fn prompt(&mut self, text: &str, echo: bool) -> Option<Zeroizing<String>> {
        // A blocked user gets no password prompt (F051).
        if !echo && let Some(name) = self.last_echo.clone() {
            let blocked = self.limiter.blocked(&Key::User(name.clone()));
            let mut seen = lock(&self.seen);
            seen.name = Some(name);
            if blocked.is_some() {
                seen.blocked = blocked;
                return None;
            }
        }
        self.next_prompt += 1;
        let prompt = self.next_prompt;
        let event = LoginEvent::Prompt {
            prompt,
            text: text.to_owned(),
            echo,
        };
        self.events.send(event).ok()?;
        loop {
            let left = self.deadline.checked_duration_since(Instant::now())?;
            match self.answers.recv_timeout(left) {
                Ok((number, answer)) if number == prompt => {
                    if echo {
                        self.last_echo = Some(answer.trim().to_owned());
                    }
                    return Some(answer);
                }
                // An answer to an older prompt.
                Ok(_) => {}
                // No answer in time, or farglassd stopped the attempt.
                Err(_) => return None,
            }
        }
    }

    fn info(&mut self, text: &str) {
        let _ = self.events.send(LoginEvent::Info {
            text: text.to_owned(),
        });
    }

    fn error(&mut self, text: &str) {
        let _ = self.events.send(LoginEvent::Error {
            text: text.to_owned(),
        });
    }
}

/// Reports whether the peer `uid` can use the login socket: root, the user `farglass` of
/// farglassd, or the user that runs farglass-login, for tests with no root (F056).
/// A read error of the user database leaves only root and the own user.
#[must_use]
pub fn peer_allowed(uid: u32) -> bool {
    uid == 0
        || uid == nix::unistd::geteuid().as_raw()
        || matches!(nix::unistd::User::from_name("farglass"), Ok(Some(user)) if user.uid.as_raw() == uid)
}

/// Reports whether the user `name` with the primary group `gid` is a member of `group`,
/// by the primary group or a supplementary group.
///
/// # Errors
///
/// The text of the cause if the group does not exist or the group database cannot be read.
pub fn membership(group: &str, name: &str, gid: nix::unistd::Gid) -> Result<bool, String> {
    let wanted = nix::unistd::Group::from_name(group)
        .map_err(|error| format!("the group database: {error}"))?
        .ok_or_else(|| format!("the group {group} does not exist"))?;
    let cname = std::ffi::CString::new(name).map_err(|error| error.to_string())?;
    let groups = nix::unistd::getgrouplist(&cname, gid)
        .map_err(|error| format!("the groups of the user: {error}"))?;
    Ok(groups.contains(&wanted.gid))
}

/// Locks `mutex`. A panic in another thread does not make the data unusable here.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn blocked(
    link: &mut Framed<UnixStream, FrameCodec>,
    left: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    send(
        link,
        &LoginEvent::Blocked {
            retry_after: left.as_secs().max(1),
        },
    )
    .await
}

fn request(frame: &Frame) -> Result<LoginRequest, Box<dyn std::error::Error + Send + Sync>> {
    if !frame.channel().is_connection() {
        return Err("a login message must be on channel 0".into());
    }
    Ok(frame.parse()?)
}

async fn send(
    link: &mut Framed<UnixStream, FrameCodec>,
    event: &LoginEvent,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    link.send(Frame::json(
        ChannelId::CONNECTION,
        FrameType::Control,
        event,
    )?)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_and_the_own_user_can_use_the_socket_and_a_different_user_cannot() {
        assert!(peer_allowed(0));
        assert!(peer_allowed(nix::unistd::geteuid().as_raw()));
        let other = (1..65_000)
            .find(|uid| {
                *uid != nix::unistd::geteuid().as_raw()
                    && !matches!(nix::unistd::User::from_name("farglass"), Ok(Some(user)) if user.uid.as_raw() == *uid)
            })
            .unwrap();
        assert!(!peer_allowed(other));
    }

    #[test]
    fn the_log_names_an_account_of_the_host_and_hides_other_text() {
        let me = nix::unistd::User::from_uid(nix::unistd::getuid())
            .unwrap()
            .unwrap()
            .name;
        let line = log_event("wrong-password", "192.0.2.1", Some(&me))
            .line(crate::audit::Outcome::Refused);
        assert!(line.contains(&format!("user={me} ")), "{line}");
        let line = log_event("wrong-password", "192.0.2.1", Some("Hunter2secret"))
            .line(crate::audit::Outcome::Refused);
        assert!(
            line.contains("user=? ") && !line.contains("Hunter2"),
            "{line}"
        );
    }
}
