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
        /// From farglassd: the session of this user is locked, and the connection has no user.
        /// The shell shows the lock screen. A login of this user on a channel of kind login unlocks the session.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        locked: Option<String>,
        /// From farglassd: the host has a machine PIN (F059). The connection has no user, and
        /// the shell must send [`ConnectionMessage::Pin`] before it opens a login channel.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        pin: Option<bool>,
    },
    /// From the shell: the machine PIN (F059). farglassd answers with [`ConnectionMessage::PinResult`].
    Pin {
        /// The digits that the user typed.
        pin: String,
    },
    /// From farglassd: the answer to [`ConnectionMessage::Pin`].
    PinResult {
        /// `true` if the PIN is correct. The shell can then open a login channel.
        accepted: bool,
        /// With too many wrong PINs from the address: the time in seconds until the next try.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "number")]
        retry_after: Option<u64>,
    },
    /// From farglassd: the user logged out, in this tab or in a different one.
    /// farglassd closes the connection after this message.
    LoggedOut,
    /// From farglassd: the session of the user is locked now. farglassd closes the connection after this message.
    Locked,
    /// From farglassd, on a connection with a locked session: a login in a different tab unlocked the session.
    /// farglassd closes the connection after this message.
    Unlocked,
    /// From the shell: lock the session of the user.
    Lock,
    /// From the shell: the user used the keyboard or the pointer. The shell sends it at most each 30 s.
    Activity,
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
    /// The agent to farglassd: logind asks to lock the session, for example after `loginctl lock-session`.
    Lock,
    /// The agent to farglassd: logind asks to unlock the session, for example after `loginctl unlock-session`.
    Unlock,
    /// farglassd to the agent: the session is locked or unlocked. The agent sets LockedHint of its logind session.
    LockedHint {
        /// Reports whether the session is locked.
        locked: bool,
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
    /// The windows of the Wayland compositor of the session: the top-level windows, the popups,
    /// and the commits. The shell gives the size, the focus, and the close of each window.
    Surfaces,
    /// The pixels of one surface of the Wayland compositor: a window or a popup.
    ///
    /// The agent sends the binary tile stream of farglass-surface in data frames.
    /// The message boundaries do not follow the data frames.
    Surface {
        /// The id of the surface from the surfaces channel.
        id: u32,
        /// Reports whether the shell decodes the video messages (F037). A shell before this field sends none and gets tiles only.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        #[ts(as = "Option<bool>", optional)]
        video: bool,
    },
    /// The keyboard and pointer input to the Wayland apps, on its own stream.
    ///
    /// The shell sends [`SurfaceInput`] as control messages.
    SurfaceInput,
    /// The clipboard of the Wayland apps (F030).
    ///
    /// Each side sends a new content as a [`ClipboardMessage::Offer`] control message
    /// and then the bytes in data frames. The agent answers an offer of the shell with
    /// [`ClipboardMessage::Stored`]. The agent sends a content only after an app set the clipboard.
    Clipboard,
    /// The sound of the apps of the session (F038).
    ///
    /// The agent sends [`AudioEvent`] control messages and, after [`AudioEvent::Format`],
    /// one data frame for each packet: the packet number as u32, little-endian, then one Opus packet.
    Audio,
    /// The video players of the host browser, for video redirection (F040).
    ///
    /// The agent sends [`RedirectEvent`] control messages. An [`RedirectEvent::Append`]
    /// is followed by data frames with its `size` bytes. The shell sends [`RedirectRequest`].
    Redirect,
    /// The use of the host by the apps of each window, and by farglass (F045, F046).
    ///
    /// The agent sends a [`StatsEvent`] every 2 s while the channel is open.
    Stats,
    /// The apps of the host for the launcher: the desktop entries (F031).
    ///
    /// The agent sends [`AppsEvent::List`] at once. The shell sends [`AppsRequest`].
    /// An [`AppsEvent::Icon`] with a mime type is followed by one data frame with the bytes of the icon.
    Apps,
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
    /// The passkeys of the user who logged in (F034). farglassd serves it, not the agent.
    ///
    /// The shell sends [`PasskeyRequest`] and gets [`PasskeyEvent`], as control messages.
    Passkeys,
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

/// A rectangle in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SurfaceRect {
    /// The left edge.
    pub x: i32,
    /// The top edge.
    pub y: i32,
    /// The width.
    pub width: i32,
    /// The height.
    pub height: i32,
}

/// A message of the agent on a surfaces channel.
///
/// Top-level windows and popups share the ids. The agent uses each id one time only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SurfaceEvent {
    /// A new top-level window, or a new title, app id, or parent of a window.
    Toplevel {
        /// The window.
        id: u32,
        /// The title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        title: Option<String>,
        /// The app id, for example `org.gnome.Calculator`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        app_id: Option<String>,
        /// The window that this window belongs to, for example for a dialog.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        parent: Option<u32>,
    },
    /// A top-level window closed.
    Closed {
        /// The window.
        id: u32,
    },
    /// A new popup, with its place relative to its parent surface in logical pixels.
    Popup {
        /// The popup.
        id: u32,
        /// The surface that the popup belongs to.
        parent: u32,
        /// The place and the size.
        geometry: SurfaceRect,
        /// The popup is a subsurface, for example a bubble of Chromium: a click outside it does not close it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        #[ts(as = "Option<bool>", optional)]
        subsurface: bool,
    },
    /// A popup closed.
    PopupClosed {
        /// The popup.
        id: u32,
    },
    /// A surface committed a new buffer.
    Commit {
        /// The surface.
        id: u32,
        /// The width of the buffer in pixels.
        width: i32,
        /// The height of the buffer in pixels.
        height: i32,
        /// Buffer pixels for each logical pixel.
        scale: i32,
        /// The parts of the buffer that changed, in buffer pixels.
        damage: Vec<SurfaceRect>,
        /// The part of the surface that is the window, with no shadow, in logical pixels.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        geometry: Option<SurfaceRect>,
    },
    /// A surface removed its buffer: the window is not visible.
    Unmapped {
        /// The surface.
        id: u32,
    },
    /// The client asks to move the window, for example after a press on its own title bar.
    MoveRequest {
        /// The window.
        id: u32,
    },
    /// The channel has each window and popup that was open at its start. The shell can close the windows of surfaces that are gone.
    Synced,
    /// The app selected the shape of the pointer: a CSS cursor name, for example `text` or `pointer`.
    Cursor {
        /// The name.
        name: String,
    },
}

/// A message of the shell on a surface channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SurfaceAck {
    /// The client shows the frame with this sequence. The agent sends at most 2 frames that the client did not acknowledge.
    Ack {
        /// The sequence of the end message.
        seq: u32,
    },
    /// The client cannot decode the video region (F037). The agent ends it and sends tiles only on this channel.
    NoVideo,
    /// The rectangle in buffer pixels where the client shows a redirected video, or none (F040).
    /// It never goes as video: the client keys its exact black.
    Key {
        /// The rectangle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        rect: Option<SurfaceRect>,
    },
}

/// A message of the shell on a surface input channel.
///
/// The coordinates are logical pixels of the surface, from its top-left corner, with no window geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SurfaceInput {
    /// The pointer moved on the surface `id`, a window or a popup.
    Pointer {
        /// The surface.
        id: u32,
        /// The left distance.
        x: f64,
        /// The top distance.
        y: f64,
    },
    /// The pointer left the surfaces.
    Leave,
    /// A pointer button: 0 primary, 1 middle, 2 secondary, 3 back, 4 forward, as in a DOM event.
    Button {
        /// The button.
        button: u8,
        /// Pressed or released.
        pressed: bool,
    },
    /// A scroll in logical pixels.
    Axis {
        /// The horizontal distance.
        dx: f64,
        /// The vertical distance.
        dy: f64,
    },
    /// A key: a Linux key code, for example 30 for A, in the window `id`. The window gets the keyboard focus first if it does not have it.
    Key {
        /// The window where the user typed the key. A shell before this field sends none: the key goes to the window with the focus.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        id: Option<u32>,
        /// The key code.
        code: u32,
        /// Pressed or released.
        pressed: bool,
    },
}

/// A rectangle of a redirected video.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct RedirectRect {
    /// The left edge.
    pub x: f64,
    /// The top edge.
    pub y: f64,
    /// The width.
    pub width: f64,
    /// The height.
    pub height: f64,
}

/// A message of the agent on a redirect channel: what a page of the host browser does with a media source.
///
/// `media` names one MediaSource of one tab. `source` names one SourceBuffer of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RedirectEvent {
    /// The page added a SourceBuffer.
    Source {
        /// The media source.
        media: String,
        /// The SourceBuffer.
        source: String,
        /// The MIME type with its codecs, for example `video/webm; codecs="vp9"`.
        mime: String,
    },
    /// The page appended `size` bytes to a SourceBuffer. Data frames with the bytes follow.
    Append {
        /// The media source.
        media: String,
        /// The SourceBuffer.
        source: String,
        /// The `timestampOffset` of the SourceBuffer at the append.
        offset: f64,
        /// The length of the data.
        size: u32,
    },
    /// The page changed the codec of a SourceBuffer with changeType.
    ChangeType {
        /// The media source.
        media: String,
        /// The SourceBuffer.
        source: String,
        /// The new MIME type with its codecs.
        mime: String,
    },
    /// The page removed a time range of a SourceBuffer.
    Remove {
        /// The media source.
        media: String,
        /// The SourceBuffer.
        source: String,
        /// The start in seconds.
        start: f64,
        /// The end in seconds.
        end: f64,
    },
    /// The page aborted the current segment of a SourceBuffer.
    Abort {
        /// The media source.
        media: String,
        /// The SourceBuffer.
        source: String,
    },
    /// The page set the duration. None is infinite.
    Duration {
        /// The media source.
        media: String,
        /// The duration in seconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        value: Option<f64>,
    },
    /// The page called endOfStream.
    End {
        /// The media source.
        media: String,
    },
    /// The state of the video element of a media source.
    State {
        /// The media source.
        media: String,
        /// The current time in seconds.
        time: f64,
        /// Reports whether the video is paused.
        paused: bool,
        /// The playback rate.
        rate: f64,
        /// The video element in the viewport, in CSS pixels.
        rect: RedirectRect,
        /// The viewport in the window, in logical pixels of the window.
        viewport: RedirectRect,
        /// The logical pixels of the window for each CSS pixel: the zoom of the page.
        zoom: f64,
        /// The title of the page: the window title of the browser starts with it.
        title: String,
        /// Reports whether the tab shows and the video is in the viewport.
        visible: bool,
        /// From the agent: the windows of the browser process that sent the media source (F052).
        /// The client places the video only on one of these windows.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<u32>>")]
        windows: Vec<u32>,
    },
    /// The media source is gone: its video element changed its source or left the page.
    Gone {
        /// The media source.
        media: String,
    },
}

/// A message of the shell on a redirect channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RedirectRequest {
    /// The client shows the video of a media source, or no longer shows it. While it shows it, the host draws the video black.
    Showing {
        /// The media source.
        media: String,
        /// Reports whether the client shows it.
        showing: bool,
    },
}

/// The use of the host by one process or by the processes of one window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Usage {
    /// The CPU in percent of one core, over the last interval.
    pub cpu: f64,
    /// The memory in bytes.
    #[ts(type = "number")]
    pub memory: u64,
    /// The network in bytes each second, received and sent. None if the agent cannot know it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub net: Option<f64>,
}

/// The use of the host by the apps of one window (F045).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct WindowUsage {
    /// The window of the shell: `surface-ID` for a Linux app, or the id of a terminal window.
    pub window: String,
    /// The use.
    pub usage: Usage,
}

/// A message of the agent on a stats channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StatsEvent {
    /// The use over the last interval.
    Sample {
        /// The apps of each window.
        windows: Vec<WindowUsage>,
        /// The agent.
        agent: Usage,
        /// farglassd. None if the agent cannot read it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        server: Option<Usage>,
    },
}

/// A control message of the agent on an audio channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AudioEvent {
    /// The format of the packets that follow.
    Format {
        /// The codec, `opus`.
        codec: String,
        /// The sample rate in Hz.
        sample_rate: u32,
        /// The channels.
        channels: u8,
        /// The samples of each channel in one packet.
        frame_samples: u32,
    },
    /// The session has no sound, for example with no PipeWire server.
    Unavailable {
        /// The cause.
        message: String,
    },
}

/// An app of the host: a desktop entry that the launcher shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HostApp {
    /// The desktop file id, for example `org.gnome.Calculator`.
    pub id: String,
    /// The name in the language of the user.
    pub name: String,
    /// What the app does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub comment: Option<String>,
    /// More words for the search.
    pub keywords: Vec<String>,
    /// Reports whether the agent has an icon for [`AppsRequest::Icon`].
    pub icon: bool,
    /// The Wayland app ids that a window of this app can have, in lowercase: the id, the `StartupWMClass`, and the program name.
    pub names: Vec<String>,
}

/// A request of the shell on an apps channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AppsRequest {
    /// Reads the desktop entries again, for example when the launcher opens.
    List,
    /// Asks for the icon of an app.
    Icon {
        /// The app.
        id: String,
    },
    /// Starts an app.
    Launch {
        /// The app.
        id: String,
    },
}

/// A message of the agent on an apps channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AppsEvent {
    /// The apps, sorted by name.
    List {
        /// The apps.
        apps: Vec<HostApp>,
    },
    /// The icon of an app. With a mime type, one data frame with the bytes follows. With none, the app has no icon.
    Icon {
        /// The app.
        id: String,
        /// `image/svg+xml` or `image/png`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        mime: Option<String>,
    },
    /// The app started.
    Launched {
        /// The app.
        id: String,
    },
    /// The app did not start.
    Failed {
        /// The app.
        id: String,
        /// The cause, for the user.
        message: String,
    },
}

/// The largest clipboard content in bytes.
pub const MAX_CLIPBOARD_LEN: u32 = 16 << 20;

/// The kind of a clipboard content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ClipboardMime {
    /// UTF-8 text.
    #[serde(rename = "text/plain")]
    Text,
    /// A PNG image.
    #[serde(rename = "image/png")]
    Png,
}

/// A message on a clipboard channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClipboardMessage {
    /// A new content: `size` bytes follow in data frames. `size` is at most [`MAX_CLIPBOARD_LEN`].
    Offer {
        /// The kind.
        mime: ClipboardMime,
        /// The length in bytes.
        size: u32,
    },
    /// From the agent: the last content of the shell is the clipboard of the apps now. A paste in an app gets it.
    Stored,
}

/// A message of the shell on a surfaces channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SurfaceRequest {
    /// Asks the client to draw the window with this size in logical pixels. 0 lets the client select.
    Configure {
        /// The window.
        id: u32,
        /// The width.
        width: i32,
        /// The height.
        height: i32,
    },
    /// Gives the focus to a window, or to no window.
    Activate {
        /// The window, or none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        id: Option<u32>,
    },
    /// Asks the client to close the window.
    Close {
        /// The window.
        id: u32,
    },
    /// Closes a popup, for example after a click outside it.
    PopupDismiss {
        /// The popup.
        id: u32,
    },
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

/// A passkey of the user, for the list of the settings (F034).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PasskeyInfo {
    /// The credential id in base64url.
    pub id: String,
    /// The name that the user gave.
    pub name: String,
    /// The host name that the passkey belongs to.
    pub host: String,
    /// The time of the registration, in seconds since 1970.
    #[ts(type = "number")]
    pub created: u64,
    /// The time of the last login with it, in seconds since 1970.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub used: Option<u64>,
}

/// A request of the shell on a passkeys channel (F034).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PasskeyRequest {
    /// Asks for the list.
    List,
    /// Starts the registration of a new passkey. farglassd answers with [`PasskeyEvent::Create`].
    Register {
        /// The name of the passkey, for example "Phone".
        name: String,
    },
    /// Ends the registration with the credential of `navigator.credentials.create`, each binary value in base64url.
    Finish {
        /// The `PublicKeyCredential` as JSON.
        #[ts(type = "unknown")]
        credential: serde_json::Value,
    },
    /// Removes a passkey.
    Remove {
        /// The credential id in base64url.
        id: String,
    },
    /// Makes new recovery codes. The old codes stop working.
    NewCodes,
    /// Answers [`PasskeyEvent::Confirm`] with the credential of `navigator.credentials.get` (F053).
    Proof {
        /// The `PublicKeyCredential` as JSON.
        #[ts(type = "unknown")]
        credential: serde_json::Value,
    },
    /// Answers [`PasskeyEvent::Confirm`] with a recovery code (F053). The code then stops working.
    ProofCode {
        /// The recovery code.
        code: String,
    },
}

/// A message of farglassd on a passkeys channel (F034).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PasskeyEvent {
    /// The passkeys of the user, after each change too.
    List {
        /// The passkeys.
        passkeys: Vec<PasskeyInfo>,
        /// The count of recovery codes that the user did not use.
        codes: u32,
        /// The host name of this connection. A passkey works only on its host name.
        host: String,
    },
    /// The `publicKey` options of `navigator.credentials.create`, each binary value in base64url.
    Create {
        /// The options.
        #[ts(type = "unknown")]
        options: serde_json::Value,
    },
    /// A change of the passkeys needs a fresh passkey or a recovery code first (F053).
    /// The shell answers with [`PasskeyRequest::Proof`] or [`PasskeyRequest::ProofCode`].
    /// After a valid proof, farglassd does the request that waited.
    Confirm {
        /// The `publicKey` options of `navigator.credentials.get`, or `None` if no passkey
        /// belongs to this host name: only a recovery code works.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "unknown")]
        options: Option<serde_json::Value>,
        /// The host names of the other passkeys of the user.
        elsewhere: Vec<String>,
    },
    /// New recovery codes. farglassd keeps only their hashes: the shell shows them one time.
    Codes {
        /// The codes.
        codes: Vec<String>,
    },
    /// A request failed.
    Error {
        /// The text for the user.
        message: String,
    },
}
