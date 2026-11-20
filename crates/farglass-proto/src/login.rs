//! The messages between farglassd and farglass-login.
//!
//! One connection to the login socket is one login attempt. Each message is
//! JSON in a control frame on channel 0.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zeroize::Zeroizing;

/// A message from farglassd to farglass-login.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LoginRequest {
    /// Starts the attempt. It is the first message of the connection.
    Start {
        /// The username, or `None` to let the PAM modules ask for it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user: Option<String>,
        /// The address of the browser, for the rate limiter and for `PAM_RHOST`.
        rhost: String,
        /// Reports whether the browser has a valid device cookie of farglassd. Such a
        /// browser can use the reserved login places (F051).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        device: bool,
        /// Only checks the password of `user`, with the limits of a login (F061). After PAM,
        /// farglass-login sends [`LoginEvent::Success`] with no second factor and starts no session.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        check: bool,
    },
    /// Answers a prompt.
    Answer {
        /// The number of the prompt.
        prompt: u32,
        /// The text that the user typed. farglass-login clears it from memory after use.
        text: Zeroizing<String>,
    },
    /// Ends the web sessions of the user `uid`, instead of a login. It is the first
    /// and only message of the connection. Local sessions continue.
    Logout {
        /// The uid of the user.
        uid: u32,
    },
    /// Answers [`LoginEvent::Confirm`].
    Confirm {
        /// `true` ends the local graphical sessions and starts the web session.
        /// `false` stops the login, and the local sessions continue.
        accept: bool,
    },
    /// Asks if the user `uid` can use farglass: a member of the allowed group (F058).
    /// It is the first and only message of the connection. The answer is [`LoginEvent::Member`].
    Member {
        /// The uid of the user of a session cookie.
        uid: u32,
    },
    /// Answers [`LoginEvent::Verified`] (F034).
    SecondFactor {
        /// `true` if the user has no passkey or used one, or a recovery code. `false` stops the login as a failure.
        accept: bool,
    },
}

/// A local graphical session of the user, which a web login ends (ADR-015).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LocalSession {
    /// The logind session id.
    pub id: String,
    /// The session type: `x11`, `wayland`, or `mir`.
    pub kind: String,
    /// The seat, for example `seat0`, if the session has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub seat: Option<String>,
}

/// A message from farglass-login to farglassd. farglassd gives it to the shell on the login channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LoginEvent {
    /// A PAM module asks a question.
    Prompt {
        /// The number of the prompt. The answer must have the same number.
        prompt: u32,
        /// The question, for example "Password: ".
        text: String,
        /// Reports whether the login screen can show the answer as the user types it.
        echo: bool,
    },
    /// A PAM module gives information.
    Info {
        /// The text for the user.
        text: String,
    },
    /// A PAM module gives an error message.
    Error {
        /// The text for the user.
        text: String,
    },
    /// The password is correct, but the user has local graphical sessions.
    ///
    /// The user must confirm that they end, and that work that is not saved
    /// in them is lost. farglassd answers with [`LoginRequest::Confirm`].
    Confirm {
        /// The local graphical sessions.
        sessions: Vec<LocalSession>,
    },
    /// PAM accepted the user (F034). farglass-login waits for [`LoginRequest::SecondFactor`]
    /// before the session steps. farglassd does not give this event to the shell.
    Verified {
        /// The username that PAM accepted.
        user: String,
        /// The uid of the user.
        uid: u32,
    },
    /// From farglassd to the shell: the user must use a passkey (F034).
    /// The shell answers with [`LoginReply::Passkey`] or [`LoginReply::RecoveryCode`].
    Passkey {
        /// The `publicKey` options of `navigator.credentials.get`, with each binary value in base64url.
        /// `None` if no passkey of the user belongs to this host name: only a recovery code works.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "unknown")]
        options: Option<serde_json::Value>,
        /// The host names of the passkeys of the user that are not this host name.
        elsewhere: Vec<String>,
    },
    /// The login succeeded. The connection ends after this message.
    Success {
        /// The username that PAM accepted.
        user: String,
        /// The uid of the user.
        uid: u32,
        /// From farglassd to the shell: a one-time ticket. The shell gives it to
        /// `POST /session`, and the answer sets the session cookie.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        ticket: Option<String>,
    },
    /// The login failed. The connection ends after this message.
    Failure {
        /// The text for the user. It does not tell if the user exists.
        message: String,
    },
    /// The answer to [`LoginRequest::Logout`]. The connection ends after this message.
    LoggedOut {
        /// The number of web sessions that ended.
        sessions: u32,
    },
    /// The answer to [`LoginRequest::Member`]. farglassd does not give this event to the shell.
    Member {
        /// `true` if the user can use farglass. A fault of the host gives `false` (ADR-042).
        allowed: bool,
    },
    /// Too many failures from the address or for the user. The connection ends after this message.
    Blocked {
        /// The time in seconds until the next attempt can start.
        #[ts(type = "number")]
        retry_after: u64,
    },
}

/// A message of the shell on the login channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LoginReply {
    /// Answers a prompt.
    Answer {
        /// The number of the prompt.
        prompt: u32,
        /// The text that the user typed.
        #[ts(type = "string")]
        text: Zeroizing<String>,
    },
    /// Answers [`LoginEvent::Confirm`].
    Confirm {
        /// `true` ends the local graphical sessions. `false` stops the login.
        accept: bool,
    },
    /// Answers [`LoginEvent::Passkey`]: the credential of `navigator.credentials.get`, with each binary value in base64url.
    Passkey {
        /// The `PublicKeyCredential` as JSON.
        #[ts(type = "unknown")]
        credential: serde_json::Value,
    },
    /// Answers [`LoginEvent::Passkey`] with a recovery code instead of a passkey.
    RecoveryCode {
        /// The code. farglassd uses each code one time.
        #[ts(type = "string")]
        code: Zeroizing<String>,
    },
}

impl LoginReply {
    /// Returns the request for farglass-login, or `None` for an answer that farglassd checks itself.
    #[must_use]
    pub fn into_request(self) -> Option<LoginRequest> {
        match self {
            Self::Answer { prompt, text } => Some(LoginRequest::Answer { prompt, text }),
            Self::Confirm { accept } => Some(LoginRequest::Confirm { accept }),
            Self::Passkey { .. } | Self::RecoveryCode { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn messages_use_the_type_tag() {
        let start = LoginRequest::Start {
            user: None,
            rhost: "192.0.2.10".to_owned(),
            device: false,
            check: false,
        };
        assert_eq!(
            serde_json::to_value(&start).unwrap(),
            json!({"type": "start", "rhost": "192.0.2.10"})
        );
        let answer: LoginRequest =
            serde_json::from_value(json!({"type": "answer", "prompt": 1, "text": "secret"}))
                .unwrap();
        assert!(
            matches!(answer, LoginRequest::Answer { prompt: 1, ref text } if text.as_str() == "secret")
        );
        assert_eq!(
            serde_json::to_value(LoginEvent::Blocked { retry_after: 30 }).unwrap(),
            json!({"type": "blocked", "retry_after": 30})
        );
    }
}
