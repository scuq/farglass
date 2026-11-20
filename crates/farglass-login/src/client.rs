//! The client side of the login socket, for farglassd.

use std::io;
use std::path::Path;

use farglass_proto::codec::{CodecError, FrameCodec};
use farglass_proto::login::{LoginEvent, LoginRequest};
use farglass_proto::{ChannelId, Frame, FrameError, FrameType};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;
use zeroize::Zeroizing;

/// An error of a login attempt.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The connection to farglass-login failed.
    #[error("the connection to farglass-login failed: {0}")]
    Io(#[from] io::Error),
    /// farglass-login sent a frame that is not valid.
    #[error("farglass-login sent a message that is not valid: {0}")]
    Frame(#[from] FrameError),
    /// farglass-login closed the connection before the result.
    #[error("farglass-login closed the connection before the result")]
    Closed,
}

impl From<CodecError> for ClientError {
    fn from(error: CodecError) -> Self {
        match error {
            CodecError::Frame(error) => Self::Frame(error),
            CodecError::Io(error) => Self::Io(error),
        }
    }
}

/// Ends the web sessions of `uid` through the login socket at `path`.
/// Returns the number of sessions that ended.
///
/// # Errors
///
/// [`ClientError`] if the connection fails, or if farglass-login could not end the sessions.
pub async fn logout(path: &Path, uid: u32) -> Result<u32, ClientError> {
    let stream = UnixStream::connect(path).await?;
    let mut attempt = Attempt {
        link: Framed::new(stream, FrameCodec),
    };
    attempt.send(&LoginRequest::Logout { uid }).await?;
    match attempt.next().await? {
        LoginEvent::LoggedOut { sessions } => Ok(sessions),
        LoginEvent::Failure { message } => Err(ClientError::Io(io::Error::other(message))),
        _ => Err(ClientError::Closed),
    }
}

/// One login attempt. Drop it to stop the attempt.
#[derive(Debug)]
pub struct Attempt {
    link: Framed<UnixStream, FrameCodec>,
}

impl Attempt {
    /// Connects to the login socket at `path` and starts an attempt.
    /// `device` reports whether the browser has a valid device cookie (F051).
    ///
    /// # Errors
    ///
    /// [`ClientError`] if the connection fails.
    pub async fn start(
        path: &Path,
        user: Option<String>,
        rhost: String,
        device: bool,
    ) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(path).await?;
        let mut attempt = Self {
            link: Framed::new(stream, FrameCodec),
        };
        attempt
            .send(&LoginRequest::Start {
                user,
                rhost,
                device,
            })
            .await?;
        Ok(attempt)
    }

    /// Returns the next event. After `Success`, `Failure`, or `Blocked`, the attempt is complete.
    ///
    /// # Errors
    ///
    /// [`ClientError`] if the connection fails or closes before the result.
    pub async fn next(&mut self) -> Result<LoginEvent, ClientError> {
        let frame = self.link.next().await.ok_or(ClientError::Closed)??;
        Ok(frame.parse()?)
    }

    /// Answers the prompt with the number `prompt`.
    ///
    /// # Errors
    ///
    /// [`ClientError`] if the connection fails.
    pub async fn answer(
        &mut self,
        prompt: u32,
        text: Zeroizing<String>,
    ) -> Result<(), ClientError> {
        self.send(&LoginRequest::Answer { prompt, text }).await
    }

    /// Answers a confirm event: `true` ends the local sessions, `false` stops the login.
    ///
    /// # Errors
    ///
    /// [`ClientError`] if the connection fails.
    pub async fn confirm(&mut self, accept: bool) -> Result<(), ClientError> {
        self.send(&LoginRequest::Confirm { accept }).await
    }

    /// Sends a reply of the shell.
    ///
    /// # Errors
    ///
    /// [`ClientError`] if the connection fails.
    pub async fn reply(
        &mut self,
        reply: farglass_proto::login::LoginReply,
    ) -> Result<(), ClientError> {
        match reply.into_request() {
            Some(request) => self.send(&request).await,
            None => Ok(()),
        }
    }

    /// Sends a request that farglassd makes itself, for example [`LoginRequest::SecondFactor`].
    ///
    /// # Errors
    ///
    /// [`ClientError`] if the link to farglass-login failed.
    pub async fn request(&mut self, request: LoginRequest) -> Result<(), ClientError> {
        self.send(&request).await
    }

    async fn send(&mut self, request: &LoginRequest) -> Result<(), ClientError> {
        let frame = Frame::json(ChannelId::CONNECTION, FrameType::Control, request)?;
        self.link.send(frame).await?;
        Ok(())
    }
}
