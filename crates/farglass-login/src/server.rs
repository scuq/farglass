//! The login service: one connection is one login attempt.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};

use farglass_proto::codec::FrameCodec;
use farglass_proto::login::{LoginEvent, LoginRequest};
use farglass_proto::{ChannelId, Frame, FrameType};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, mpsc};
use tokio_util::codec::Framed;
use zeroize::Zeroizing;

use crate::limiter::{Key, Limiter};
use crate::pam::{self, Conversation, Login};

/// farglassd must send the start message in this time.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// The user must answer one prompt in this time.
pub const PROMPT_TIMEOUT: Duration = Duration::from_secs(60);

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
    /// The largest number of attempts at the same time.
    pub max_attempts: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            service: "farglass".to_owned(),
            pam_dir: None,
            allow_root: false,
            fail_delay: Duration::from_secs(2),
            max_attempts: 32,
        }
    }
}

/// The login service.
#[derive(Debug)]
pub struct Service {
    settings: Settings,
    limiter: Limiter,
    attempts: Arc<Semaphore>,
}

impl Service {
    /// Returns a service with `settings`.
    #[must_use]
    pub fn new(settings: Settings) -> Arc<Self> {
        let attempts = Arc::new(Semaphore::new(settings.max_attempts));
        Arc::new(Self {
            settings,
            limiter: Limiter::default(),
            attempts,
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
        let mut link = Framed::new(stream, FrameCodec);

        let start = tokio::time::timeout(START_TIMEOUT, link.next())
            .await
            .map_err(|_| "no start message")?
            .ok_or("the connection closed before the start message")??;
        let LoginRequest::Start { user, rhost } = request(&start)? else {
            return Err("the first message was not a start message".into());
        };
        let address = Key::Address(rhost.clone());
        let user_key = user.clone().map(Key::User);

        let blocked = [Some(&address), user_key.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(|key| self.limiter.blocked(key))
            .max();
        if let Some(left) = blocked {
            tracing::info!(rhost, ?user, ?peer, "a blocked login was refused");
            return send(
                &mut link,
                &LoginEvent::Blocked {
                    retry_after: left.as_secs().max(1),
                },
            )
            .await;
        }
        let Ok(_permit) = self.attempts.clone().try_acquire_owned() else {
            tracing::warn!(rhost, "too many logins at the same time");
            let message =
                "The server has too many logins at the same time. Try again later.".to_owned();
            return send(&mut link, &LoginEvent::Failure { message }).await;
        };

        let (events, mut from_pam) = mpsc::unbounded_channel();
        let (answers, to_pam) = std_mpsc::channel();
        let settings = self.settings.clone();
        let pam_user = user.clone();
        let pam_rhost = rhost.clone();
        let pam = tokio::task::spawn_blocking(move || {
            let mut bridge = Bridge {
                events,
                answers: to_pam,
                next_prompt: 0,
            };
            let params = Login {
                service: &settings.service,
                confdir: settings.pam_dir.as_deref(),
                user: pam_user.as_deref(),
                rhost: &pam_rhost,
            };
            pam::login(params, &mut bridge)
        });

        // Relay the conversation until PAM completes.
        loop {
            tokio::select! {
                event = from_pam.recv() => match event {
                    Some(event) => send(&mut link, &event).await?,
                    // The PAM thread stopped. Its result follows.
                    None => break,
                },
                frame = link.next() => match frame {
                    Some(Ok(frame)) => match request(&frame)? {
                        LoginRequest::Answer { prompt, text } => {
                            let _ = answers.send((prompt, text));
                        }
                        LoginRequest::Start { .. } => return Err("a second start message".into()),
                    },
                    // farglassd stopped the attempt. Dropping `answers` stops the PAM conversation.
                    _ => {
                        drop(answers);
                        let _ = pam.await;
                        return Ok(());
                    }
                },
            }
        }

        match pam.await? {
            Ok(name) => self.succeeded(&mut link, &address, name, &rhost).await,
            Err(failure) => {
                self.limiter.failed(address);
                if let Some(name) = failure.user.clone().or(user) {
                    self.limiter.failed(Key::User(name));
                }
                tracing::info!(rhost, user = ?failure.user, reason = failure.reason, "a login failed");
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

    async fn succeeded(
        &self,
        link: &mut Framed<UnixStream, FrameCodec>,
        address: &Key,
        name: String,
        rhost: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let uid = match nix::unistd::User::from_name(&name) {
            Ok(Some(user)) => user.uid.as_raw(),
            _ => {
                tracing::warn!(
                    user = name,
                    "PAM accepted a user that has no account on the host"
                );
                tokio::time::sleep(self.settings.fail_delay).await;
                let message = FAILURE_MESSAGE.to_owned();
                return send(link, &LoginEvent::Failure { message }).await;
            }
        };
        if uid == 0 && !self.settings.allow_root {
            tracing::warn!(rhost, "a login as root was refused");
            self.limiter.failed(address.clone());
            tokio::time::sleep(self.settings.fail_delay).await;
            let message = "The login as root is not permitted.".to_owned();
            return send(link, &LoginEvent::Failure { message }).await;
        }
        self.limiter.succeeded(address);
        self.limiter.succeeded(&Key::User(name.clone()));
        tracing::info!(rhost, user = name, uid, "a login succeeded");
        send(link, &LoginEvent::Success { user: name, uid }).await
    }
}

/// The conversation of one attempt: each prompt goes to farglassd, and the answer comes back.
struct Bridge {
    events: mpsc::UnboundedSender<LoginEvent>,
    answers: std_mpsc::Receiver<(u32, Zeroizing<String>)>,
    next_prompt: u32,
}

impl Conversation for Bridge {
    fn prompt(&mut self, text: &str, echo: bool) -> Option<Zeroizing<String>> {
        self.next_prompt += 1;
        let prompt = self.next_prompt;
        let event = LoginEvent::Prompt {
            prompt,
            text: text.to_owned(),
            echo,
        };
        self.events.send(event).ok()?;
        let deadline = Instant::now() + PROMPT_TIMEOUT;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            match self.answers.recv_timeout(left) {
                Ok((number, answer)) if number == prompt => return Some(answer),
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
