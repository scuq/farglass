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
user?: string, };

/**
 * The payload of an open frame. It selects the kind of the channel.
 */
export type Open = { "kind": "echo" } | { "kind": "login" };

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
text: string, } | { "type": "success", 
/**
 * The username that PAM accepted.
 */
user: string, 
/**
 * The uid of the user.
 */
uid: number, } | { "type": "failure", 
/**
 * The text for the user. It does not tell if the user exists.
 */
message: string, } | { "type": "blocked", 
/**
 * The time in seconds until the next attempt can start.
 */
retry_after: number, };

/**
 * The answer of the shell to a prompt, on the login channel.
 */
export type LoginAnswer = { 
/**
 * The number of the prompt.
 */
prompt: number, 
/**
 * The text that the user typed.
 */
text: string, };
