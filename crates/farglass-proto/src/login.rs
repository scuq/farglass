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
}

impl From<LoginReply> for LoginRequest {
    fn from(reply: LoginReply) -> Self {
        match reply {
            LoginReply::Answer { prompt, text } => Self::Answer { prompt, text },
            LoginReply::Confirm { accept } => Self::Confirm { accept },
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
