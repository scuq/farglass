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
        /// From farglassd: the user of the connection, if it has one.
        /// With no user, the shell must log in on a channel of kind login.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        user: Option<String>,
    },
    /// From farglassd: the user logged out, in this tab or in a different one.
    /// farglassd closes the connection after this message.
    LoggedOut,
}

/// A message on channel 0 of the agent link, between farglassd and an agent.
///
/// The shell never sees these messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LinkMessage {
    /// The first message of each side, as in [`ConnectionMessage::Hello`].
    Hello {
        /// The protocol version of the sender.
        version: u16,
    },
    /// farglassd to the agent: a browser connection of the user uses the agent from now on.
    Attach {
        /// The number of the browser connection. farglassd uses each number one time only.
        connection: u64,
    },
    /// farglassd to the agent: the browser connection stopped.
    ///
    /// farglassd sent a close frame for each open channel of the connection first.
    Detach {
        /// The number of the browser connection.
        connection: u64,
    },
}

/// The payload of an open frame. It selects the kind of the channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Open {
    /// A channel that sends each data frame back to the sender.
    Echo,
    /// A terminal in the agent. The terminal continues when the channel closes.
    ///
    /// If the agent has a terminal with the id `terminal`, the channel attaches
    /// to it and gets its scrollback first. If not, the agent starts the shell of
    /// the user in a new terminal. Data frames carry the input and the output.
    /// Control messages are [`PtyRequest`] and [`PtyEvent`].
    Pty {
        /// The id of the terminal, chosen by the shell, for example the id of the window.
        terminal: String,
        /// The number of columns.
        cols: u16,
        /// The number of rows.
        rows: u16,
    },
    /// A D-Bus connection of the agent, as the user.
    ///
    /// The shell sends [`DbusRequest`] and gets [`DbusEvent`], as control messages.
    /// The agent permits only the bus names of its allowlist.
    Dbus {
        /// The bus.
        bus: Bus,
    },
    /// The desktop notifications of the session. The shell keeps one open while the desktop is open.
    ///
    /// The agent sends [`NotifyEvent`] and gets [`NotifyReply`], as control messages.
    Notify,
    /// The polkit prompts of the session. The shell keeps one open while the desktop shows.
    ///
    /// The agent sends [`PolkitEvent`] and gets [`PolkitReply`], as control messages.
    Polkit,
    /// The state store of the session in the agent. The shell keeps its state
    /// there, for example the open windows, and gets it back after it connects again.
    ///
    /// The shell sends [`StateRequest`] and gets [`StateEvent`], as control messages.
    State,
    /// A PAM login. farglassd serves this channel itself.
    ///
    /// farglassd sends each [`crate::login::LoginEvent`] as a control message.
    /// The shell answers a prompt with [`crate::login::LoginAnswer`].
    /// After the result, farglassd closes the channel.
    Login,
}

/// A D-Bus bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Bus {
    /// The system bus.
    System,
    /// The session bus of the user.
    Session,
}

/// A request of the shell on a dbus channel.
///
/// Values use this JSON form: numbers, strings, and booleans as they are;
/// arrays and structures as JSON arrays; dictionaries as JSON objects; a
/// variant as `{"type": "<signature>", "value": <value>}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum DbusRequest {
    /// Calls a method. The agent answers with [`DbusEvent::Reply`] or [`DbusEvent::Error`].
    Call {
        /// The number of the request, chosen by the shell.
        id: u32,
        /// The bus name of the service.
        destination: String,
        /// The object path.
        path: String,
        /// The interface.
        interface: String,
        /// The method.
        method: String,
        /// The D-Bus signature of the arguments, for example `sb`. Empty for no arguments.
        #[serde(default)]
        signature: String,
        /// The arguments, in the JSON form.
        #[serde(default)]
        #[ts(type = "unknown[]")]
        args: Vec<serde_json::Value>,
        /// Lets the service ask polkit for an interactive authentication.
        #[serde(default)]
        interactive: bool,
    },
    /// Subscribes to signals. The agent sends [`DbusEvent::Signal`] for each one
    /// until [`DbusRequest::Unsubscribe`] or the end of the channel.
    Subscribe {
        /// The number of the subscription, chosen by the shell.
        id: u32,
        /// The bus name of the sender.
        sender: String,
        /// The object path, or all paths.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        path: Option<String>,
        /// The interface, or all interfaces.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        interface: Option<String>,
        /// The signal name, or all signals.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        member: Option<String>,
    },
    /// Ends a subscription.
    Unsubscribe {
        /// The number of the subscription.
        id: u32,
    },
}

/// A message of the agent on a dbus channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum DbusEvent {
    /// The return values of a call, in the JSON form.
    Reply {
        /// The number of the request.
        id: u32,
        /// The values.
        #[ts(type = "unknown[]")]
        values: Vec<serde_json::Value>,
    },
    /// A call or a subscription failed.
    Error {
        /// The number of the request.
        id: u32,
        /// The D-Bus error name, for example `org.freedesktop.DBus.Error.AccessDenied`.
        name: String,
        /// The error message.
        message: String,
    },
    /// A signal of a subscription.
    Signal {
        /// The number of the subscription.
        subscription: u32,
        /// The object path.
        path: String,
        /// The interface.
        interface: String,
        /// The signal name.
        member: String,
        /// The values, in the JSON form.
        #[ts(type = "unknown[]")]
        values: Vec<serde_json::Value>,
    },
}

/// The urgency of a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Urgency {
    /// For example "the download is complete".
    Low,
    /// The usual urgency.
    Normal,
    /// For example "the battery is almost empty". The shell shows it until the user closes it.
    Critical,
}

/// An action button of a notification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NotifyAction {
    /// The key that goes back to the program.
    pub key: String,
    /// The label of the button.
    pub label: String,
}

/// One desktop notification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Notification {
    /// The id that the program got from Notify.
    pub id: u32,
    /// The name of the program.
    pub app: String,
    /// The summary, one line.
    pub summary: String,
    /// The body, plain text.
    pub body: String,
    /// The action buttons. The key `default` is the action of a click on the notification.
    pub actions: Vec<NotifyAction>,
    /// The urgency.
    pub urgency: Urgency,
    /// The time to show the notification in milliseconds. `None` shows it until the user closes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timeout_ms: Option<u32>,
}

/// A message of the agent on a notify channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NotifyEvent {
    /// Shows a new notification, or replaces the one with the same id.
    Show {
        /// The notification.
        notification: Notification,
    },
    /// Removes a notification.
    Close {
        /// The id of the notification.
        id: u32,
    },
}

/// A message of the shell on a notify channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NotifyReply {
    /// The user closed the notification.
    Dismiss {
        /// The id of the notification.
        id: u32,
    },
    /// The time of the notification ended.
    Expire {
        /// The id of the notification.
        id: u32,
    },
    /// The user selected an action.
    Action {
        /// The id of the notification.
        id: u32,
        /// The key of the action.
        key: String,
    },
}

/// A message of the agent on a polkit channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PolkitEvent {
    /// A program asks for an action that needs an authentication. The shell shows a dialog.
    Begin {
        /// The number of the request.
        request: u32,
        /// The polkit action, for example `org.freedesktop.timedate1.set-timezone`.
        action: String,
        /// The text of polkit for the user.
        message: String,
        /// The users who can authenticate.
        identities: Vec<String>,
        /// The user of the first attempt.
        identity: String,
    },
    /// The PAM conversation asks a question.
    Prompt {
        /// The number of the request.
        request: u32,
        /// The question, for example "Password: ".
        text: String,
        /// Reports whether the dialog can show the answer as the user types it.
        echo: bool,
    },
    /// The PAM conversation gives information.
    Info {
        /// The number of the request.
        request: u32,
        /// The text.
        text: String,
    },
    /// The authentication failed, for example after a wrong password. The user can try again.
    Failed {
        /// The number of the request.
        request: u32,
    },
    /// The request ended: with a success, or with a cancel by the user, by polkit, or by a different tab.
    Done {
        /// The number of the request.
        request: u32,
        /// Reports whether the authentication succeeded.
        success: bool,
    },
}

/// A message of the shell on a polkit channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PolkitReply {
    /// Starts an attempt as `identity`, for example after a failure or to change the user.
    Start {
        /// The number of the request.
        request: u32,
        /// One of the identities of the request.
        identity: String,
    },
    /// Answers the last prompt.
    Answer {
        /// The number of the request.
        request: u32,
        /// The text that the user typed.
        #[ts(type = "string")]
        text: zeroize::Zeroizing<String>,
    },
    /// Cancels the request.
    Cancel {
        /// The number of the request.
        request: u32,
    },
}

/// A control message of the shell on a pty channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PtyRequest {
    /// Changes the size of the terminal.
    Resize {
        /// The number of columns.
        cols: u16,
        /// The number of rows.
        rows: u16,
    },
    /// The shell processed `bytes` more bytes of output.
    ///
    /// The agent sends at most [`crate::PTY_WINDOW`] bytes that the shell did not
    /// acknowledge. Then it stops to read the terminal until an acknowledgment comes.
    Ack {
        /// The number of bytes.
        bytes: u32,
    },
    /// Stops the programs of the terminal. The agent then sends [`PtyEvent::Exited`].
    Terminate,
}

/// A control message of the agent on a pty channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PtyEvent {
    /// The channel is attached to the terminal. It is the first message after the ready frame.
    Attached {
        /// Reports whether the agent started a new terminal.
        new: bool,
        /// The number of bytes of scrollback that follow as data frames, before the live output.
        replay: u32,
    },
    /// The program of the terminal stopped. The agent closes the channel after this message.
    Exited {
        /// The exit code, if the program exited.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        code: Option<i32>,
        /// The signal, if a signal stopped the program.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        signal: Option<i32>,
    },
}

/// A request of the shell on a state channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StateRequest {
    /// Asks for the value of `key`. After this request, the channel also gets each change of the key.
    Get {
        /// The key, for example `windows`.
        key: String,
    },
    /// Sets the value of `key`. `null` removes the key.
    ///
    /// Each other state channel that asked for the key gets the new value.
    Set {
        /// The key.
        key: String,
        /// The value, any JSON value.
        #[ts(type = "unknown")]
        value: serde_json::Value,
    },
}

/// A message of the agent on a state channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StateEvent {
    /// The value of `key`: the answer to a get request, or a change from a different channel.
    Value {
        /// The key.
        key: String,
        /// The value, or `null` if the key has no value.
        #[ts(type = "unknown")]
        value: serde_json::Value,
    },
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
    fn link_messages_use_the_type_tag() {
        assert_eq!(
            serde_json::to_value(LinkMessage::Attach { connection: 7 }).unwrap(),
            json!({"type": "attach", "connection": 7})
        );
        let hello: LinkMessage =
            serde_json::from_value(json!({"type": "hello", "version": 1})).unwrap();
        assert_eq!(hello, LinkMessage::Hello { version: 1 });
    }

    #[test]
    fn unknown_variant_is_refused() {
        assert!(serde_json::from_value::<Open>(json!({"kind": "telepathy"})).is_err());
        assert!(serde_json::from_value::<Close>(json!({"reason": "bored"})).is_err());
        assert!(serde_json::from_value::<ConnectionMessage>(json!({"type": "bye"})).is_err());
    }
}
