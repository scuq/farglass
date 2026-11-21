// Generated from crates/farglass-proto. Do not edit.
// To generate this file again: cargo run -p farglass-proto --example gen-ts

/**
 * A message on the connection channel (channel 0).
 */
export type ConnectionMessage = { "type": "hello", 
/**
 * The protocol version of the sender.
 */
version: number, 
/**
 * From farglassd: the user of the connection, if it has one.
 * With no user, the shell must log in on a channel of kind login.
 */
user?: string, 
/**
 * From farglassd: the session of this user is locked, and the connection has no user.
 * The shell shows the lock screen. A login of this user on a channel of kind login unlocks the session.
 */
locked?: string, 
/**
 * From farglassd: the host has a machine PIN (F059). The connection has no user, and
 * the shell must send [`ConnectionMessage::Pin`] before it opens a login channel.
 */
pin?: boolean, } | { "type": "pin", 
/**
 * The digits that the user typed.
 */
pin: string, } | { "type": "pin-result", 
/**
 * `true` if the PIN is correct. The shell can then open a login channel.
 */
accepted: boolean, 
/**
 * With too many wrong PINs from the address: the time in seconds until the next try.
 */
retry_after?: number, } | { "type": "logged-out" } | { "type": "locked" } | { "type": "unlocked" } | { "type": "lock" } | { "type": "activity" };

/**
 * The payload of an open frame. It selects the kind of the channel.
 */
export type Open = { "kind": "echo" } | { "kind": "pty", 
/**
 * The id of the terminal, chosen by the shell, for example the id of the window.
 */
terminal: string, 
/**
 * The number of columns.
 */
cols: number, 
/**
 * The number of rows.
 */
rows: number, } | { "kind": "dbus", 
/**
 * The bus.
 */
bus: Bus, } | { "kind": "notify" } | { "kind": "surfaces" } | { "kind": "surface", 
/**
 * The id of the surface from the surfaces channel.
 */
id: number, 
/**
 * Reports whether the shell decodes the video messages (F037). A shell before this field sends none and gets tiles only.
 */
video?: boolean, } | { "kind": "surface-input" } | { "kind": "clipboard" } | { "kind": "audio" } | { "kind": "redirect" } | { "kind": "stats" } | { "kind": "apps" } | { "kind": "polkit" } | { "kind": "state" } | { "kind": "login" } | { "kind": "passkeys" };

/**
 * The payload of a ready frame. The receiver of the open frame accepts the channel.
 */
export type Ready = Record<symbol, never>;

/**
 * The payload of a close frame.
 */
export type Close = { 
/**
 * The cause of the close.
 */
reason: CloseReason, 
/**
 * More information for a log or for the user.
 */
message?: string, };

/**
 * The cause of a close frame.
 */
export type CloseReason = "done" | "cancelled" | "not-supported" | "not-found" | "access-denied" | "resource-limit" | "protocol-error" | "internal-error";

/**
 * A D-Bus bus.
 */
export type Bus = "system" | "session";

/**
 * A request of the shell on a dbus channel.
 *
 * Values use this JSON form: numbers, strings, and booleans as they are;
 * arrays and structures as JSON arrays; dictionaries as JSON objects; a
 * variant as `{"type": "<signature>", "value": <value>}`.
 */
export type DbusRequest = { "type": "call", 
/**
 * The number of the request, chosen by the shell.
 */
id: number, 
/**
 * The bus name of the service.
 */
destination: string, 
/**
 * The object path.
 */
path: string, 
/**
 * The interface.
 */
interface: string, 
/**
 * The method.
 */
method: string, 
/**
 * The D-Bus signature of the arguments, for example `sb`. Empty for no arguments.
 */
signature: string, 
/**
 * The arguments, in the JSON form.
 */
args: unknown[], 
/**
 * Lets the service ask polkit for an interactive authentication.
 */
interactive: boolean, } | { "type": "subscribe", 
/**
 * The number of the subscription, chosen by the shell.
 */
id: number, 
/**
 * The bus name of the sender.
 */
sender: string, 
/**
 * The object path, or all paths.
 */
path?: string, 
/**
 * The interface, or all interfaces.
 */
interface?: string, 
/**
 * The signal name, or all signals.
 */
member?: string, } | { "type": "unsubscribe", 
/**
 * The number of the subscription.
 */
id: number, };

/**
 * A message of the agent on a dbus channel.
 */
export type DbusEvent = { "type": "reply", 
/**
 * The number of the request.
 */
id: number, 
/**
 * The values.
 */
values: unknown[], } | { "type": "error", 
/**
 * The number of the request.
 */
id: number, 
/**
 * The D-Bus error name, for example `org.freedesktop.DBus.Error.AccessDenied`.
 */
name: string, 
/**
 * The error message.
 */
message: string, } | { "type": "signal", 
/**
 * The number of the subscription.
 */
subscription: number, 
/**
 * The object path.
 */
path: string, 
/**
 * The interface.
 */
interface: string, 
/**
 * The signal name.
 */
member: string, 
/**
 * The values, in the JSON form.
 */
values: unknown[], };

/**
 * The urgency of a notification.
 */
export type Urgency = "low" | "normal" | "critical";

/**
 * An action button of a notification.
 */
export type NotifyAction = { 
/**
 * The key that goes back to the program.
 */
key: string, 
/**
 * The label of the button.
 */
label: string, };

/**
 * One desktop notification.
 */
export type Notification = { 
/**
 * The id that the program got from Notify.
 */
id: number, 
/**
 * The name of the program.
 */
app: string, 
/**
 * The summary, one line.
 */
summary: string, 
/**
 * The body, plain text.
 */
body: string, 
/**
 * The action buttons. The key `default` is the action of a click on the notification.
 */
actions: Array<NotifyAction>, 
/**
 * The urgency.
 */
urgency: Urgency, 
/**
 * The time to show the notification in milliseconds. `None` shows it until the user closes it.
 */
timeout_ms?: number, };

/**
 * A message of the agent on a notify channel.
 */
export type NotifyEvent = { "type": "show", 
/**
 * The notification.
 */
notification: Notification, } | { "type": "close", 
/**
 * The id of the notification.
 */
id: number, };

/**
 * A message of the shell on a notify channel.
 */
export type NotifyReply = { "type": "dismiss", 
/**
 * The id of the notification.
 */
id: number, } | { "type": "expire", 
/**
 * The id of the notification.
 */
id: number, } | { "type": "action", 
/**
 * The id of the notification.
 */
id: number, 
/**
 * The key of the action.
 */
key: string, };

/**
 * A rectangle in pixels.
 */
export type SurfaceRect = { 
/**
 * The left edge.
 */
x: number, 
/**
 * The top edge.
 */
y: number, 
/**
 * The width.
 */
width: number, 
/**
 * The height.
 */
height: number, };

/**
 * A message of the agent on a surfaces channel.
 *
 * Top-level windows and popups share the ids. The agent uses each id one time only.
 */
export type SurfaceEvent = { "type": "toplevel", 
/**
 * The window.
 */
id: number, 
/**
 * The title.
 */
title?: string, 
/**
 * The app id, for example `org.gnome.Calculator`.
 */
app_id?: string, 
/**
 * The window that this window belongs to, for example for a dialog.
 */
parent?: number, } | { "type": "closed", 
/**
 * The window.
 */
id: number, } | { "type": "popup", 
/**
 * The popup.
 */
id: number, 
/**
 * The surface that the popup belongs to.
 */
parent: number, 
/**
 * The place and the size.
 */
geometry: SurfaceRect, 
/**
 * The popup is a subsurface, for example a bubble of Chromium: a click outside it does not close it.
 */
subsurface?: boolean, } | { "type": "popup-closed", 
/**
 * The popup.
 */
id: number, } | { "type": "commit", 
/**
 * The surface.
 */
id: number, 
/**
 * The width of the buffer in pixels.
 */
width: number, 
/**
 * The height of the buffer in pixels.
 */
height: number, 
/**
 * Buffer pixels for each logical pixel.
 */
scale: number, 
/**
 * The parts of the buffer that changed, in buffer pixels.
 */
damage: Array<SurfaceRect>, 
/**
 * The part of the surface that is the window, with no shadow, in logical pixels.
 */
geometry?: SurfaceRect, } | { "type": "unmapped", 
/**
 * The surface.
 */
id: number, } | { "type": "move-request", 
/**
 * The window.
 */
id: number, } | { "type": "synced" } | { "type": "cursor", 
/**
 * The name.
 */
name: string, };

/**
 * A message of the shell on a surfaces channel.
 */
export type SurfaceRequest = { "type": "configure", 
/**
 * The window.
 */
id: number, 
/**
 * The width.
 */
width: number, 
/**
 * The height.
 */
height: number, } | { "type": "activate", 
/**
 * The window, or none.
 */
id?: number, } | { "type": "close", 
/**
 * The window.
 */
id: number, } | { "type": "popup-dismiss", 
/**
 * The popup.
 */
id: number, };

/**
 * A message of the shell on a surface channel.
 */
export type SurfaceAck = { "type": "ack", 
/**
 * The sequence of the end message.
 */
seq: number, } | { "type": "no-video" } | { "type": "key", 
/**
 * The rectangle.
 */
rect?: SurfaceRect, 
/**
 * The host draws the video in the rectangle now (F073): the rectangle goes as video at once, not as tiles.
 */
video?: boolean, };

/**
 * The use of the host by one process or by the processes of one window.
 */
export type Usage = { 
/**
 * The CPU in percent of one core, over the last interval.
 */
cpu: number, 
/**
 * The memory in bytes.
 */
memory: number, 
/**
 * The network in bytes each second, received and sent. None if the agent cannot know it.
 */
net?: number, };

/**
 * The use of the host by the apps of one window (F045).
 */
export type WindowUsage = { 
/**
 * The window of the shell: `surface-ID` for a Linux app, or the id of a terminal window.
 */
window: string, 
/**
 * The use.
 */
usage: Usage, };

/**
 * A message of the agent on a stats channel.
 */
export type StatsEvent = { "type": "sample", 
/**
 * The apps of each window.
 */
windows: Array<WindowUsage>, 
/**
 * The agent.
 */
agent: Usage, 
/**
 * farglassd. None if the agent cannot read it.
 */
server?: Usage, };

/**
 * A passkey of the user, for the list of the settings (F034).
 */
export type PasskeyInfo = { 
/**
 * The credential id in base64url.
 */
id: string, 
/**
 * The name that the user gave.
 */
name: string, 
/**
 * The host name that the passkey belongs to.
 */
host: string, 
/**
 * The time of the registration, in seconds since 1970.
 */
created: number, 
/**
 * The time of the last login with it, in seconds since 1970.
 */
used?: number, };

/**
 * A request of the shell on a passkeys channel (F034).
 */
export type PasskeyRequest = { "type": "list" } | { "type": "register", 
/**
 * The name of the passkey, for example "Phone".
 */
name: string, } | { "type": "finish", 
/**
 * The `PublicKeyCredential` as JSON.
 */
credential: unknown, } | { "type": "remove", 
/**
 * The credential id in base64url.
 */
id: string, } | { "type": "new-codes" } | { "type": "proof", 
/**
 * The `PublicKeyCredential` as JSON.
 */
credential: unknown, } | { "type": "proof-code", 
/**
 * The recovery code.
 */
code: string, } | { "type": "proof-password", 
/**
 * The password. farglassd gives it only to farglass-login.
 */
password: string, };

/**
 * A message of farglassd on a passkeys channel (F034).
 */
export type PasskeyEvent = { "type": "list", 
/**
 * The passkeys.
 */
passkeys: Array<PasskeyInfo>, 
/**
 * The count of recovery codes that the user did not use.
 */
codes: number, 
/**
 * The host name of this connection. A passkey works only on its host name.
 */
host: string, } | { "type": "create", 
/**
 * The options.
 */
options: unknown, } | { "type": "confirm", 
/**
 * The `publicKey` options of `navigator.credentials.get`, or `None` if no passkey
 * belongs to this host name: only a recovery code works.
 */
options?: unknown, 
/**
 * The host names of the other passkeys of the user.
 */
elsewhere: Array<string>, } | { "type": "password" } | { "type": "codes", 
/**
 * The codes.
 */
codes: Array<string>, } | { "type": "error", 
/**
 * The text for the user.
 */
message: string, };

/**
 * A control message of the agent on an audio channel.
 */
export type AudioEvent = { "type": "format", 
/**
 * The codec, `opus`.
 */
codec: string, 
/**
 * The sample rate in Hz.
 */
sample_rate: number, 
/**
 * The channels.
 */
channels: number, 
/**
 * The samples of each channel in one packet.
 */
frame_samples: number, } | { "type": "unavailable", 
/**
 * The cause.
 */
message: string, };

/**
 * A rectangle of a redirected video.
 */
export type RedirectRect = { 
/**
 * The left edge.
 */
x: number, 
/**
 * The top edge.
 */
y: number, 
/**
 * The width.
 */
width: number, 
/**
 * The height.
 */
height: number, };

/**
 * A message of the agent on a redirect channel: what a page of the host browser does with a media source.
 *
 * `media` names one MediaSource of one tab. `source` names one SourceBuffer of it.
 */
export type RedirectEvent = { "type": "source", 
/**
 * The media source.
 */
media: string, 
/**
 * The SourceBuffer.
 */
source: string, 
/**
 * The MIME type with its codecs, for example `video/webm; codecs="vp9"`.
 */
mime: string, } | { "type": "append", 
/**
 * The media source.
 */
media: string, 
/**
 * The SourceBuffer.
 */
source: string, 
/**
 * The `timestampOffset` of the SourceBuffer at the append.
 */
offset: number, 
/**
 * The length of the data.
 */
size: number, } | { "type": "change-type", 
/**
 * The media source.
 */
media: string, 
/**
 * The SourceBuffer.
 */
source: string, 
/**
 * The new MIME type with its codecs.
 */
mime: string, } | { "type": "remove", 
/**
 * The media source.
 */
media: string, 
/**
 * The SourceBuffer.
 */
source: string, 
/**
 * The start in seconds.
 */
start: number, 
/**
 * The end in seconds.
 */
end: number, } | { "type": "abort", 
/**
 * The media source.
 */
media: string, 
/**
 * The SourceBuffer.
 */
source: string, } | { "type": "duration", 
/**
 * The media source.
 */
media: string, 
/**
 * The duration in seconds.
 */
value?: number, } | { "type": "end", 
/**
 * The media source.
 */
media: string, } | { "type": "state", 
/**
 * The media source.
 */
media: string, 
/**
 * The current time in seconds.
 */
time: number, 
/**
 * Reports whether the video is paused.
 */
paused: boolean, 
/**
 * The playback rate.
 */
rate: number, 
/**
 * The video element in the viewport, in CSS pixels.
 */
rect: RedirectRect, 
/**
 * The viewport in the window, in logical pixels of the window.
 */
viewport: RedirectRect, 
/**
 * The logical pixels of the window for each CSS pixel: the zoom of the page.
 */
zoom: number, 
/**
 * The title of the page: the window title of the browser starts with it.
 */
title: string, 
/**
 * Reports whether the tab shows and the video is in the viewport.
 */
visible: boolean, 
/**
 * From the agent: the windows of the browser process that sent the media source (F052).
 * The client places the video only on one of these windows.
 */
windows?: Array<number>, } | { "type": "gone", 
/**
 * The media source.
 */
media: string, };

/**
 * A message of the shell on a redirect channel.
 */
export type RedirectRequest = { "type": "showing", 
/**
 * The media source.
 */
media: string, 
/**
 * Reports whether the client shows it.
 */
showing: boolean, };

/**
 * An app of the host: a desktop entry that the launcher shows.
 */
export type HostApp = { 
/**
 * The desktop file id, for example `org.gnome.Calculator`.
 */
id: string, 
/**
 * The name in the language of the user.
 */
name: string, 
/**
 * What the app does.
 */
comment?: string, 
/**
 * More words for the search.
 */
keywords: Array<string>, 
/**
 * Reports whether the agent has an icon for [`AppsRequest::Icon`].
 */
icon: boolean, 
/**
 * The Wayland app ids that a window of this app can have, in lowercase: the id, the `StartupWMClass`, and the program name.
 */
names: Array<string>, };

/**
 * A request of the shell on an apps channel.
 */
export type AppsRequest = { "type": "list" } | { "type": "icon", 
/**
 * The app.
 */
id: string, } | { "type": "launch", 
/**
 * The app.
 */
id: string, };

/**
 * A message of the agent on an apps channel.
 */
export type AppsEvent = { "type": "list", 
/**
 * The apps.
 */
apps: Array<HostApp>, } | { "type": "icon", 
/**
 * The app.
 */
id: string, 
/**
 * `image/svg+xml` or `image/png`.
 */
mime?: string, } | { "type": "launched", 
/**
 * The app.
 */
id: string, } | { "type": "failed", 
/**
 * The app.
 */
id: string, 
/**
 * The cause, for the user.
 */
message: string, };

/**
 * The kind of a clipboard content.
 */
export type ClipboardMime = "text/plain" | "image/png";

/**
 * A message on a clipboard channel.
 */
export type ClipboardMessage = { "type": "offer", 
/**
 * The kind.
 */
mime: ClipboardMime, 
/**
 * The length in bytes.
 */
size: number, } | { "type": "stored" };

/**
 * A message of the shell on a surface input channel.
 *
 * The coordinates are logical pixels of the surface, from its top-left corner, with no window geometry.
 */
export type SurfaceInput = { "type": "pointer", 
/**
 * The surface.
 */
id: number, 
/**
 * The left distance.
 */
x: number, 
/**
 * The top distance.
 */
y: number, } | { "type": "leave" } | { "type": "button", 
/**
 * The button.
 */
button: number, 
/**
 * Pressed or released.
 */
pressed: boolean, } | { "type": "axis", 
/**
 * The horizontal distance.
 */
dx: number, 
/**
 * The vertical distance.
 */
dy: number, } | { "type": "key", 
/**
 * The window where the user typed the key. A shell before this field sends none: the key goes to the window with the focus.
 */
id?: number, 
/**
 * The key code.
 */
code: number, 
/**
 * Pressed or released.
 */
pressed: boolean, };

/**
 * A message of the agent on a polkit channel.
 */
export type PolkitEvent = { "type": "begin", 
/**
 * The number of the request.
 */
request: number, 
/**
 * The polkit action, for example `org.freedesktop.timedate1.set-timezone`.
 */
action: string, 
/**
 * The text of polkit for the user.
 */
message: string, 
/**
 * The users who can authenticate.
 */
identities: Array<string>, 
/**
 * The user of the first attempt.
 */
identity: string, } | { "type": "prompt", 
/**
 * The number of the request.
 */
request: number, 
/**
 * The question, for example "Password: ".
 */
text: string, 
/**
 * Reports whether the dialog can show the answer as the user types it.
 */
echo: boolean, } | { "type": "info", 
/**
 * The number of the request.
 */
request: number, 
/**
 * The text.
 */
text: string, } | { "type": "failed", 
/**
 * The number of the request.
 */
request: number, } | { "type": "done", 
/**
 * The number of the request.
 */
request: number, 
/**
 * Reports whether the authentication succeeded.
 */
success: boolean, };

/**
 * A message of the shell on a polkit channel.
 */
export type PolkitReply = { "type": "start", 
/**
 * The number of the request.
 */
request: number, 
/**
 * One of the identities of the request.
 */
identity: string, } | { "type": "answer", 
/**
 * The number of the request.
 */
request: number, 
/**
 * The text that the user typed.
 */
text: string, } | { "type": "cancel", 
/**
 * The number of the request.
 */
request: number, };

/**
 * A control message of the shell on a pty channel.
 */
export type PtyRequest = { "type": "resize", 
/**
 * The number of columns.
 */
cols: number, 
/**
 * The number of rows.
 */
rows: number, } | { "type": "ack", 
/**
 * The number of bytes.
 */
bytes: number, } | { "type": "terminate" };

/**
 * A control message of the agent on a pty channel.
 */
export type PtyEvent = { "type": "attached", 
/**
 * Reports whether the agent started a new terminal.
 */
new: boolean, 
/**
 * The number of bytes of scrollback that follow as data frames, before the live output.
 */
replay: number, } | { "type": "exited", 
/**
 * The exit code, if the program exited.
 */
code?: number, 
/**
 * The signal, if a signal stopped the program.
 */
signal?: number, };

/**
 * A request of the shell on a state channel.
 */
export type StateRequest = { "type": "get", 
/**
 * The key, for example `windows`.
 */
key: string, } | { "type": "set", 
/**
 * The key.
 */
key: string, 
/**
 * The value, any JSON value.
 */
value: unknown, };

/**
 * A message of the agent on a state channel.
 */
export type StateEvent = { "type": "value", 
/**
 * The key.
 */
key: string, 
/**
 * The value, or `null` if the key has no value.
 */
value: unknown, };

/**
 * A message from farglass-login to farglassd. farglassd gives it to the shell on the login channel.
 */
export type LoginEvent = { "type": "prompt", 
/**
 * The number of the prompt. The answer must have the same number.
 */
prompt: number, 
/**
 * The question, for example "Password: ".
 */
text: string, 
/**
 * Reports whether the login screen can show the answer as the user types it.
 */
echo: boolean, } | { "type": "info", 
/**
 * The text for the user.
 */
text: string, } | { "type": "error", 
/**
 * The text for the user.
 */
text: string, } | { "type": "confirm", 
/**
 * The local graphical sessions.
 */
sessions: Array<LocalSession>, } | { "type": "verified", 
/**
 * The username that PAM accepted.
 */
user: string, 
/**
 * The uid of the user.
 */
uid: number, } | { "type": "passkey", 
/**
 * The `publicKey` options of `navigator.credentials.get`, with each binary value in base64url.
 * `None` if no passkey of the user belongs to this host name: only a recovery code works.
 */
options?: unknown, 
/**
 * The host names of the passkeys of the user that are not this host name.
 */
elsewhere: Array<string>, } | { "type": "success", 
/**
 * The username that PAM accepted.
 */
user: string, 
/**
 * The uid of the user.
 */
uid: number, 
/**
 * From farglassd to the shell: a one-time ticket. The shell gives it to
 * `POST /session`, and the answer sets the session cookie.
 */
ticket?: string, } | { "type": "failure", 
/**
 * The text for the user. It does not tell if the user exists.
 */
message: string, } | { "type": "logged-out", 
/**
 * The number of web sessions that ended.
 */
sessions: number, } | { "type": "member", 
/**
 * `true` if the user can use farglass. A fault of the host gives `false` (ADR-042).
 */
allowed: boolean, } | { "type": "blocked", 
/**
 * The time in seconds until the next attempt can start.
 */
retry_after: number, };

/**
 * A local graphical session of the user, which a web login ends (ADR-015).
 */
export type LocalSession = { 
/**
 * The logind session id.
 */
id: string, 
/**
 * The session type: `x11`, `wayland`, or `mir`.
 */
kind: string, 
/**
 * The seat, for example `seat0`, if the session has one.
 */
seat?: string, };

/**
 * A message of the shell on the login channel.
 */
export type LoginReply = { "type": "answer", 
/**
 * The number of the prompt.
 */
prompt: number, 
/**
 * The text that the user typed.
 */
text: string, } | { "type": "confirm", 
/**
 * `true` ends the local graphical sessions. `false` stops the login.
 */
accept: boolean, } | { "type": "passkey", 
/**
 * The `PublicKeyCredential` as JSON.
 */
credential: unknown, } | { "type": "recovery-code", 
/**
 * The code. farglassd uses each code one time.
 */
code: string, };
