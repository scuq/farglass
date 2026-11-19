//! Generates the TypeScript side of the protocol from the Rust definitions.
//!
//! `cargo run -p farglass-proto --example gen-ts` writes the files.
//! The test `tests/typescript.rs` fails when the files in the repository are old.

use std::fmt::Write as _;

use ts_rs::{Config, TS};

use crate::login::{LocalSession, LoginEvent, LoginReply};
use crate::message::{
    Bus, Close, CloseReason, ConnectionMessage, DbusEvent, DbusRequest, Notification, NotifyAction,
    NotifyEvent, NotifyReply, Open, PolkitEvent, PolkitReply, PtyEvent, PtyRequest, Ready,
    StateEvent, StateRequest, SurfaceAck, SurfaceEvent, SurfaceInput, SurfaceRect, SurfaceRequest,
    Urgency,
};
use crate::{
    ChannelId, FrameError, FrameType, HEADER_LEN, MAX_PAYLOAD_LEN, PROTOCOL_VERSION,
    PTY_SCROLLBACK, PTY_WINDOW,
};

/// The directory of the generated files, relative to the root of the repository.
pub const OUTPUT_DIR: &str = "web/src/proto/generated";

const BANNER: &str = "\
// Generated from crates/farglass-proto. Do not edit.
// To generate this file again: cargo run -p farglass-proto --example gen-ts
";

/// Returns each generated file as a filename and its contents.
#[must_use]
pub fn files() -> Vec<(&'static str, String)> {
    vec![("wire.ts", wire()), ("messages.ts", messages())]
}

/// Returns `wire.ts`: the constants of the binary frame format.
fn wire() -> String {
    let mut out = String::from(BANNER);
    // A write to a String cannot fail.
    let _ = write!(
        out,
        "
/** The version of the channel protocol. */
export const PROTOCOL_VERSION = {PROTOCOL_VERSION};

/** The length of a frame header in bytes. */
export const HEADER_LEN = {HEADER_LEN};

/** The largest payload of one frame in bytes. */
export const MAX_PAYLOAD_LEN = {MAX_PAYLOAD_LEN};

/** The number of the connection channel. */
export const CONNECTION_CHANNEL = {connection};

/** The largest number of terminal output bytes that the shell did not acknowledge. */
export const PTY_WINDOW = {PTY_WINDOW};

/** The size of the scrollback of a terminal in bytes. */
export const PTY_SCROLLBACK = {PTY_SCROLLBACK};

/** The type of a frame, as the number in the header. */
export const FrameType = {{
",
        connection = ChannelId::CONNECTION.get(),
    );
    for frame_type in FrameType::ALL {
        let _ = writeln!(out, "  {}: {},", frame_type.name(), frame_type as u8);
    }
    out.push_str(
        "} as const;
export type FrameType = (typeof FrameType)[keyof typeof FrameType];

/** The code of a frame error. The Rust side uses the same codes. */
export type FrameErrorCode =",
    );
    for code in FrameError::CODES {
        let _ = write!(out, "\n  | \"{code}\"");
    }
    out.push_str(";\n");
    out
}

/// Returns `messages.ts`: the types of the JSON messages.
fn messages() -> String {
    let cfg = Config::new();
    let mut out = String::from(BANNER);
    declare::<ConnectionMessage>(&cfg, &mut out);
    declare::<Open>(&cfg, &mut out);
    declare::<Ready>(&cfg, &mut out);
    declare::<Close>(&cfg, &mut out);
    declare::<CloseReason>(&cfg, &mut out);
    declare::<Bus>(&cfg, &mut out);
    declare::<DbusRequest>(&cfg, &mut out);
    declare::<DbusEvent>(&cfg, &mut out);
    declare::<Urgency>(&cfg, &mut out);
    declare::<NotifyAction>(&cfg, &mut out);
    declare::<Notification>(&cfg, &mut out);
    declare::<NotifyEvent>(&cfg, &mut out);
    declare::<NotifyReply>(&cfg, &mut out);
    declare::<SurfaceRect>(&cfg, &mut out);
    declare::<SurfaceEvent>(&cfg, &mut out);
    declare::<SurfaceRequest>(&cfg, &mut out);
    declare::<SurfaceAck>(&cfg, &mut out);
    declare::<SurfaceInput>(&cfg, &mut out);
    declare::<PolkitEvent>(&cfg, &mut out);
    declare::<PolkitReply>(&cfg, &mut out);
    declare::<PtyRequest>(&cfg, &mut out);
    declare::<PtyEvent>(&cfg, &mut out);
    declare::<StateRequest>(&cfg, &mut out);
    declare::<StateEvent>(&cfg, &mut out);
    declare::<LoginEvent>(&cfg, &mut out);
    declare::<LocalSession>(&cfg, &mut out);
    declare::<LoginReply>(&cfg, &mut out);
    out
}

/// Appends the exported declaration of `T` with its documentation.
fn declare<T: TS>(cfg: &Config, out: &mut String) {
    out.push('\n');
    if let Some(docs) = T::docs() {
        out.push_str(docs.trim_end());
        out.push('\n');
    }
    out.push_str("export ");
    out.push_str(T::decl(cfg).trim_end());
    out.push('\n');
}
