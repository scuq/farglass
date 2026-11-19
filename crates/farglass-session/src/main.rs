//! farglass-session is the agent in the logind session of a user.
//!
//! It connects to the agent socket of farglassd and serves the channels of the
//! browser connections of the user. If the link stops, it connects again.
//!
//! Usage:
//!
//!     farglass-session
//!
//! FARGLASS_AGENT_SOCKET names the agent socket. The default is /run/farglass/agent.sock.

use std::path::PathBuf;
use std::time::Duration;

use farglass_session::link::Session;
use farglass_session::{DEFAULT_AGENT_SOCKET, link};
use tracing_subscriber::EnvFilter;

/// The first wait before the agent connects again.
const RETRY_MIN: Duration = Duration::from_millis(250);
/// The longest wait before the agent connects again.
const RETRY_MAX: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let path = std::env::var_os("FARGLASS_AGENT_SOCKET")
        .map_or_else(|| PathBuf::from(DEFAULT_AGENT_SOCKET), PathBuf::from);
    // The state store and the terminals live as long as the agent, also when the link to farglassd breaks.
    let mut session = Session::default();
    let mut wait = RETRY_MIN;
    loop {
        match link::connect(&path, &mut session).await {
            Ok(()) => {
                tracing::info!("farglassd closed the link");
                wait = RETRY_MIN;
            }
            Err(error) => {
                tracing::warn!(socket = %path.display(), %error, "no link to farglassd");
            }
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(RETRY_MAX);
    }
}
