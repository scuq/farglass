// Generated from crates/farglass-proto. Do not edit.
// To generate this file again: cargo run -p farglass-proto --example gen-ts

/** The version of the channel protocol. */
export const PROTOCOL_VERSION = 1;

/** The length of a frame header in bytes. */
export const HEADER_LEN = 8;

/** The largest payload of one frame in bytes. */
export const MAX_PAYLOAD_LEN = 1048576;

/** The number of the connection channel. */
export const CONNECTION_CHANNEL = 0;

/** The largest number of terminal output bytes that the shell did not acknowledge. */
export const PTY_WINDOW = 262144;

/** The size of the scrollback of a terminal in bytes. */
export const PTY_SCROLLBACK = 1048576;

/** The largest clipboard content in bytes. */
export const MAX_CLIPBOARD_LEN = 16777216;

/** The type of a frame, as the number in the header. */
export const FrameType = {
  Open: 1,
  Ready: 2,
  Data: 3,
  Control: 4,
  Close: 5,
} as const;
export type FrameType = (typeof FrameType)[keyof typeof FrameType];

/** The code of a frame error. The Rust side uses the same codes. */
export type FrameErrorCode =
  | "unknown-frame-type"
  | "payload-too-large"
  | "invalid-connection-frame"
  | "truncated"
  | "trailing-bytes"
  | "invalid-json";
