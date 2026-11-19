//! The login channel: farglassd relays a PAM login between the shell and farglass-login.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use farglass_login::client::Attempt;
use std::time::{Duration, Instant};

use farglass_proto::login::{LoginEvent, LoginReply};
use farglass_proto::message::{Close, CloseReason, Ready};
use farglass_proto::{ChannelId, Frame, FrameType};
use tokio::sync::mpsc;

use crate::agents::Registry;

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
#[derive(Clone, Debug, Default)]
pub struct SharedUser(Arc<Mutex<Option<Identity>>>);

impl SharedUser {
    /// Returns a shared user with the value `user`.
    #[must_use]
    pub fn new(user: Option<Identity>) -> Self {
        Self(Arc::new(Mutex::new(user)))
    }

    /// Returns the current user.
    #[must_use]
    pub fn get(&self) -> Option<Identity> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, user: Identity) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(user);
    }
}

/// The parameters of one login channel.
#[derive(Debug)]
pub struct LoginTask {
    /// The login socket of farglass-login.
    pub socket: PathBuf,
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
        let mut attempt = Attempt::start(&self.socket, None, self.rhost.clone())
            .await
            .map_err(|error| {
                tracing::error!(socket = %self.socket.display(), %error, "the login service is not available");
                "The login service is not available.".to_owned()
            })?;
        self.send(Frame::ready(self.channel, &Ready {})).await;

        loop {
            tokio::select! {
                event = attempt.next() => {
                    let event = event.map_err(|error| {
                        tracing::error!(%error, "the login service stopped during a login");
                        "The login service stopped.".to_owned()
                    })?;
                    let done = match &event {
                        LoginEvent::Success { user, uid } => {
                            self.wait_for_agent(*uid).await;
                            self.user.set(Identity { uid: *uid, name: user.clone() });
                            true
                        }
                        LoginEvent::Failure { .. } | LoginEvent::Blocked { .. } => true,
                        LoginEvent::Prompt { .. }
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
