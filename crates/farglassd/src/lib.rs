//! farglassd: the web server and the channel router on the host.
//!
//! farglassd serves the shell files and the WebSocket endpoint of the channel
//! protocol through TLS. It runs as an unprivileged user.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod agents;
pub mod config;
pub mod connection;
pub mod cookie;
pub mod device;
mod http;
pub mod lock;
pub mod login;
pub mod origin;
pub mod passkeys;
pub mod pin;
pub mod presence;
pub mod quic;
pub mod server;
pub mod tls;
mod ws;

pub use config::Config;
pub use server::Server;
