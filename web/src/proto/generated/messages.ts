// Generated from crates/farglass-proto. Do not edit.
// To generate this file again: cargo run -p farglass-proto --example gen-ts

/**
 * A message on the connection channel (channel 0).
 */
export type ConnectionMessage = { "type": "hello", 
/**
 * The protocol version of the sender.
 */
version: number, };

/**
 * The payload of an open frame. It selects the kind of the channel.
 */
export type Open = { "kind": "echo" };

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
