//! farglassd: the web server and the channel router on the host.
//!
//! farglassd serves the shell files and the WebSocket endpoint of the channel
//! protocol through TLS. It runs as an unprivileged user.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
mod http;
pub mod server;
pub mod tls;
mod ws;

pub use config::Config;
pub use server::Server;
