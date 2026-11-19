//! The JSON messages of the channel protocol.
//!
//! A receiver ignores a field that it does not know.
//! A receiver refuses a variant that it does not know.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A message on the connection channel (channel 0).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ConnectionMessage {
    /// The first message of each side.
    ///
    /// If the versions are different, the receiver closes the connection.
    Hello {
        /// The protocol version of the sender.
        version: u16,
    },
}

/// The payload of an open frame. It selects the kind of the channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Open {
    /// A channel that sends each data frame back to the sender.
    Echo,
}

/// The payload of a ready frame. The receiver of the open frame accepts the channel.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Ready {}

/// The payload of a close frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Close {
    /// The cause of the close.
    pub reason: CloseReason,
    /// More information for a log or for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

impl Close {
    /// Returns a close payload with `reason` and no message.
    #[must_use]
    pub const fn new(reason: CloseReason) -> Self {
        Self {
            reason,
            message: None,
        }
    }
}

/// The cause of a close frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CloseReason {
    /// The work of the channel is complete.
    Done,
    /// The peer does not need the channel.
    Cancelled,
    /// The receiver does not know the channel kind or an option.
    NotSupported,
    /// The object that the channel refers to does not exist.
    NotFound,
    /// The user does not have the permission.
    AccessDenied,
    /// A limit of the receiver stops the channel.
    ResourceLimit,
    /// The peer sent a frame that the protocol does not permit.
    ProtocolError,
    /// The sender of the close frame has a defect or a failure.
    InternalError,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn open_uses_the_kind_tag() {
        assert_eq!(
            serde_json::to_value(Open::Echo).unwrap(),
            json!({"kind": "echo"})
        );
    }

    #[test]
    fn close_omits_an_empty_message() {
        let close = Close::new(CloseReason::AccessDenied);
        assert_eq!(
            serde_json::to_value(&close).unwrap(),
            json!({"reason": "access-denied"})
        );
    }

    #[test]
    fn unknown_field_is_ignored() {
        let close: Close =
            serde_json::from_value(json!({"reason": "done", "added-later": 1})).unwrap();
        assert_eq!(close, Close::new(CloseReason::Done));
    }

    #[test]
    fn unknown_variant_is_refused() {
        assert!(serde_json::from_value::<Open>(json!({"kind": "telepathy"})).is_err());
        assert!(serde_json::from_value::<Close>(json!({"reason": "bored"})).is_err());
        assert!(serde_json::from_value::<ConnectionMessage>(json!({"type": "bye"})).is_err());
    }
}
