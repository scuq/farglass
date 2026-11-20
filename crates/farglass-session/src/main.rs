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
    // The polkit agent of the logind session: polkit asks it for the authentications of the session.
    if let Ok(session_id) = std::env::var("XDG_SESSION_ID") {
        // The lock signals of logind, for example from `loginctl lock-session`.
        let logind = session.logind.clone();
        let id = session_id.clone();
        tokio::spawn(async move {
            if let Err(error) = logind.watch(&id).await {
                tracing::warn!(%error, "the agent does not follow the lock signals of logind");
            }
        });
        let polkit = session.polkit.clone();
        let buses = session.buses.clone();
        tokio::spawn(async move {
            let result = match buses.connection(farglass_proto::message::Bus::System).await {
                Ok(connection) => {
                    farglass_session::polkit::register(polkit, &connection, &session_id).await
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                tracing::warn!(%error, "the agent is not the polkit agent of the session");
            }
        });
    }
    // The desktop notifications of the session. FARGLASS_NOTIFICATIONS_NAME changes the bus name, for tests.
    {
        let notifications = session.notifications.clone();
        let buses = session.buses.clone();
        let name = std::env::var("FARGLASS_NOTIFICATIONS_NAME")
            .unwrap_or_else(|_| farglass_session::notify::NOTIFY_NAME.to_owned());
        tokio::spawn(async move {
            let result = match buses
                .connection(farglass_proto::message::Bus::Session)
                .await
            {
                Ok(connection) => {
                    farglass_session::notify::serve(notifications, &connection, &name).await
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                tracing::warn!(%error, "the agent does not serve the desktop notifications");
            }
        });
    }
    // The sound sink of the session (F038). It lives as long as the agent.
    let _sink = match session.audio.start_sink() {
        Ok(sink) => Some(sink),
        Err(error) => {
            tracing::warn!(%error, "no sound sink, the apps play no sound in the browser");
            None
        }
    };
    // The Wayland compositor of the session (F024). It lives as long as the agent.
    let _compositor = farglass_session::surfaces::socket_path().and_then(|socket| {
        match session.surfaces.start(&socket) {
            Ok(compositor) => {
                tracing::info!(socket = %socket.display(), "the Wayland compositor listens");
                // A web session: programs that D-Bus starts must find the compositor, not a GNOME that is not there.
                if std::env::var_os("XDG_SESSION_ID").is_some() {
                    let buses = session.buses.clone();
                    let socket = socket.clone();
                    tokio::spawn(async move {
                        match farglass_session::desktop::announce(&buses, &socket).await {
                            Ok(true) => tracing::info!("the user manager and D-Bus activation have the farglass desktop"),
                            Ok(false) => tracing::info!("the user manager has a display already, the agent keeps it"),
                            Err(error) => tracing::warn!(%error, "the desktop variables did not go to the user manager"),
                        }
                    });
                }
                Some(compositor)
            }
            Err(error) => {
                tracing::warn!(%error, "no Wayland compositor");
                None
            }
        }
    });
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
