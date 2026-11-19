//! farglass-login: the PAM login of farglass.
//!
//! farglass-login runs as root and has no network access (ADR-003). farglassd
//! connects to its Unix socket for each login attempt. farglass-login sends
//! each PAM prompt to farglassd, which shows it on the login screen.
//!
//! All unsafe code of the crate is in the modules `pam`, `activation`, and `worker`.

#![deny(unsafe_code)]
#![warn(missing_docs)]

#[allow(unsafe_code)]
pub mod activation;
pub mod client;
pub mod host;
pub mod limiter;
#[allow(unsafe_code)]
pub mod pam;
pub mod server;
#[allow(unsafe_code)]
pub mod worker;

/// The login socket that farglassd connects to when its configuration names no other socket.
pub const DEFAULT_LOGIN_SOCKET: &str = "/run/farglass/login.sock";
