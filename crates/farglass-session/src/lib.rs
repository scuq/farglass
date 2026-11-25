//! farglass-session: the agent in the logind session of a user.
//!
//! The agent connects to the agent socket of farglassd. The kernel tells
//! farglassd the uid of the agent, and farglassd gives the agent only the
//! browser connections of that user (ADR-003, ADR-015).

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod audio;
pub mod bookmarks;
pub mod broker;
pub mod browsers;
pub mod camera;
pub mod dbus;
pub mod desktop;
pub mod launch;
pub mod link;
pub mod logind;
pub mod mic;
pub mod netcheck;
pub mod notify;
pub mod polkit;
pub mod redirect;
pub mod ssh_agent;
pub mod state;
pub mod stats;
pub mod surface_stream;
pub mod surfaces;
pub mod terminal;
pub mod tray;

/// The agent socket that the agent connects to when the environment names no other socket.
pub const DEFAULT_AGENT_SOCKET: &str = "/run/farglass/agent.sock";
