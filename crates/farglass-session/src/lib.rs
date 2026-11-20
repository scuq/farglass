//! farglass-session: the agent in the logind session of a user.
//!
//! The agent connects to the agent socket of farglassd. The kernel tells
//! farglassd the uid of the agent, and farglassd gives the agent only the
//! browser connections of that user (ADR-003, ADR-015).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dbus;
pub mod desktop;
pub mod launch;
pub mod link;
pub mod logind;
pub mod notify;
pub mod polkit;
pub mod state;
pub mod surface_stream;
pub mod surfaces;
pub mod terminal;

/// The agent socket that the agent connects to when the environment names no other socket.
pub const DEFAULT_AGENT_SOCKET: &str = "/run/farglass/agent.sock";
