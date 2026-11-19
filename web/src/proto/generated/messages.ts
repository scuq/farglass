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
user?: string, } | { "type": "logged-out" };

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
rows: number, } | { "kind": "state" } | { "kind": "login" };

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
sessions: Array<LocalSession>, } | { "type": "success", 
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
sessions: number, } | { "type": "blocked", 
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
accept: boolean, };
