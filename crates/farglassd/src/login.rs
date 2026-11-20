//! The login channel: farglassd relays a PAM login between the shell and farglass-login.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use farglass_login::client::Attempt;
use std::time::{Duration, Instant};

use farglass_proto::login::{LoginEvent, LoginReply, LoginRequest};
use farglass_proto::message::{Close, CloseReason, Ready};
use farglass_proto::{ChannelId, Frame, FrameType};
use tokio::sync::mpsc;

use crate::agents::Registry;
use crate::cookie::SessionStore;
use crate::passkeys::PasskeyStore;
use crate::presence::{Presence, PresenceGuard};

/// A user who logged in, or the user of `dev_user`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The uid. It selects the agent.
    pub uid: u32,
    /// The username.
    pub name: String,
}

impl Identity {
    /// Returns the identity of the user with the uid `uid`. The name comes from the user database.
    #[must_use]
    pub fn from_uid(uid: u32) -> Self {
        let name = nix::unistd::User::from_uid(uid.into())
            .ok()
            .flatten()
            .map_or_else(|| uid.to_string(), |user| user.name);
        Self { uid, name }
    }
}

/// The user of one browser connection. The login task sets it after a success.
///
/// While the connection has a user, it counts as a connection of that user
/// for the idle logout.
#[derive(Clone, Debug)]
pub struct SharedUser {
    inner: Arc<Mutex<Option<(Identity, PresenceGuard)>>>,
    presence: Arc<Presence>,
}

impl SharedUser {
    /// Returns a shared user with the value `user`.
    #[must_use]
    pub fn new(user: Option<Identity>, presence: Arc<Presence>) -> Self {
        let shared = Self {
            inner: Arc::new(Mutex::new(None)),
            presence,
        };
        if let Some(user) = user {
            shared.set(user);
        }
        shared
    }

    /// Returns the current user.
    #[must_use]
    pub fn get(&self) -> Option<Identity> {
        self.lock().as_ref().map(|(identity, _)| identity.clone())
    }

    fn set(&self, user: Identity) {
        let guard = self.presence.enter(user.uid);
        *self.lock() = Some((user, guard));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(Identity, PresenceGuard)>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The parameters of one login channel.
#[derive(Debug)]
pub struct LoginTask {
    /// The login socket of farglass-login.
    pub socket: PathBuf,
    /// The only user who can log in, on the lock screen of a locked session.
    pub only_user: Option<String>,
    /// The lock state of the sessions. A login unlocks the session of its user.
    pub presence: Arc<Presence>,
    /// The address of the browser.
    pub rhost: String,
    /// The channel of the shell.
    pub channel: ChannelId,
    /// The replies of the shell.
    pub answers: mpsc::Receiver<LoginReply>,
    /// The queue to the shell.
    pub to_shell: mpsc::Sender<Frame>,
    /// The user of the connection. The task sets it after a success.
    pub user: SharedUser,
    /// The connected agents.
    pub registry: Arc<Registry>,
    /// The longest wait for the agent of the user after a success.
    pub agent_wait: Duration,
    /// The store that gives the ticket for the session cookie.
    pub sessions: Arc<SessionStore>,
    /// The passkeys of the users (F034). `None` turns the second factor off.
    pub passkeys: Option<Arc<PasskeyStore>>,
    /// The origin of the page, from the checked `Origin` header. A passkey belongs to its host name.
    pub origin: Option<String>,
}

impl LoginTask {
    /// Runs the login until it has a result, or until the shell closes the channel.
    ///
    /// The task always ends with a close frame on the channel.
    pub async fn run(mut self) {
        let close = match self.relay().await {
            Ok(()) => Close::new(CloseReason::Done),
            Err(message) => Close {
                reason: CloseReason::InternalError,
                message: Some(message),
            },
        };
        if let Ok(frame) = Frame::close(self.channel, &close) {
            let _ = self.to_shell.send(frame).await;
        }
    }

    async fn relay(&mut self) -> Result<(), String> {
        let mut attempt = Attempt::start(&self.socket, self.only_user.clone(), self.rhost.clone())
            .await
            .map_err(|error| {
                tracing::error!(socket = %self.socket.display(), %error, "the login service is not available");
                "The login service is not available.".to_owned()
            })?;
        self.send(Frame::ready(self.channel, &Ready {})).await;

        loop {
            tokio::select! {
                event = attempt.next() => {
                    let mut event = event.map_err(|error| {
                        tracing::error!(%error, "the login service stopped during a login");
                        "The login service stopped.".to_owned()
                    })?;
                    if let LoginEvent::Verified { uid, .. } = event {
                        // The shell does not get this event: farglassd answers it (F034).
                        let Some(accept) = self.second_factor(uid).await else {
                            return Ok(());
                        };
                        if attempt.request(LoginRequest::SecondFactor { accept }).await.is_err() {
                            return Err("The login service stopped.".to_owned());
                        }
                        continue;
                    }
                    let done = match &mut event {
                        LoginEvent::Success { user, uid, ticket } => {
                            self.wait_for_agent(*uid).await;
                            let identity = Identity { uid: *uid, name: user.clone() };
                            *ticket = Some(self.sessions.ticket(identity.clone()));
                            // The connection gets its user first: the unlock then does not end it.
                            self.user.set(identity);
                            crate::lock::unlock(&self.presence, &self.registry, *uid, "login");
                            true
                        }
                        LoginEvent::Failure { .. } | LoginEvent::Blocked { .. } | LoginEvent::LoggedOut { .. } => true,
                        LoginEvent::Prompt { .. }
                        | LoginEvent::Verified { .. }
                        | LoginEvent::Passkey { .. }
                        | LoginEvent::Info { .. }
                        | LoginEvent::Error { .. }
                        | LoginEvent::Confirm { .. } => false,
                    };
                    self.send(Frame::json(self.channel, FrameType::Control, &event)).await;
                    if done {
                        return Ok(());
                    }
                }
                answer = self.answers.recv() => match answer {
                    Some(reply) => {
                        if attempt.reply(reply).await.is_err() {
                            return Err("The login service stopped.".to_owned());
                        }
                    }
                    // The shell closed the channel or the connection. Dropping the attempt stops PAM.
                    None => return Ok(()),
                },
            }
        }
    }

    /// Asks the shell for the passkey of the user `uid`, if the user has one (F034).
    /// Reports whether the second factor passed, or `None` if the shell closed the channel.
    async fn second_factor(&mut self, uid: u32) -> Option<bool> {
        let Some(store) = self.passkeys.clone() else {
            return Some(true);
        };
        if !store.required(uid) {
            return Some(true);
        }
        let challenge = store.challenge(uid, self.origin.as_deref());
        let event = LoginEvent::Passkey {
            options: challenge.options.clone(),
            elsewhere: challenge.elsewhere.clone(),
        };
        self.send(Frame::json(self.channel, FrameType::Control, &event))
            .await;
        loop {
            match self.answers.recv().await? {
                LoginReply::Passkey { credential } => {
                    return Some(store.verify(uid, self.origin.as_deref(), &challenge, credential));
                }
                LoginReply::RecoveryCode { code } => return Some(store.use_code(uid, &code)),
                // An answer to a different question.
                LoginReply::Answer { .. } | LoginReply::Confirm { .. } => {}
            }
        }
    }

    /// Waits until the agent of `uid` connects, for [`LoginTask::agent_wait`] at most.
    ///
    /// A new web session starts its agent after the success. Without the wait,
    /// the first channel of the shell finds no agent.
    async fn wait_for_agent(&self, uid: u32) {
        let start = Instant::now();
        while self.registry.get(uid).is_none() {
            if start.elapsed() >= self.agent_wait {
                tracing::warn!(uid, "the agent of the user did not connect after the login");
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn send(&self, frame: Result<Frame, farglass_proto::FrameError>) {
        if let Ok(frame) = frame {
            let _ = self.to_shell.send(frame).await;
        }
    }
}

/// Logs out the user `uid`: removes the cookie sessions, tells each browser
/// connection of the user, and ends the web sessions through farglass-login.
pub async fn logout(context: &crate::connection::Context, uid: u32, cause: &str) {
    context.sessions.remove_user(uid);
    context.presence.announce_logout(uid);
    match farglass_login::client::logout(&context.login_socket, uid).await {
        Ok(sessions) => tracing::info!(uid, sessions, cause, "logout"),
        Err(error) => {
            tracing::warn!(uid, cause, %error, "the web session did not end through farglass-login")
        }
    }
}
