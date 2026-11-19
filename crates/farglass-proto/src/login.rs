//! The messages between farglassd and farglass-login.
//!
//! One connection to the login socket is one login attempt. Each message is
//! JSON in a control frame on channel 0.

use serde::{Deserialize, Serialize};
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
}

/// A message from farglass-login to farglassd.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The login succeeded. The connection ends after this message.
    Success {
        /// The username that PAM accepted.
        user: String,
        /// The uid of the user.
        uid: u32,
    },
    /// The login failed. The connection ends after this message.
    Failure {
        /// The text for the user. It does not tell if the user exists.
        message: String,
    },
    /// Too many failures from the address or for the user. The connection ends after this message.
    Blocked {
        /// The time in seconds until the next attempt can start.
        retry_after: u64,
    },
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
