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

fn main() {
    // Chromium starts the native messaging host of video redirection with the origin of the extension (F040).
    if std::env::args()
        .nth(1)
        .is_some_and(|arg| arg.starts_with("chrome-extension://"))
    {
        let code = match farglass_session::redirect::socket_path() {
            Some(socket) => match farglass_session::redirect::native_host(&socket) {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("farglass-session: no agent for video redirection: {error}");
                    1
                }
            },
            None => 1,
        };
        std::process::exit(code);
    }
    // The link broker of the session worker, before any other thread (F057).
    if let Some(broker) = farglass_session::broker::Broker::take_from_env() {
        let _ = BROKER.set(broker);
    }
    // Other processes of the user cannot attach a debugger to the agent or read its memory (F057).
    if let Err(error) = farglass_session::broker::protect() {
        eprintln!("farglass-session: the agent stays open to a debugger of the user: {error}");
    }
    agent();
}

/// The link broker of the session worker, in a web session (F057).
static BROKER: std::sync::OnceLock<farglass_session::broker::Broker> = std::sync::OnceLock::new();

/// Returns a new link from the session worker.
async fn brokered_link(
    broker: &'static farglass_session::broker::Broker,
) -> std::io::Result<tokio::net::UnixStream> {
    let stream = tokio::task::spawn_blocking(move || broker.link())
        .await
        .map_err(std::io::Error::other)??;
    stream.set_nonblocking(true)?;
    tokio::net::UnixStream::from_std(stream)
}

#[tokio::main]
async fn agent() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        // No color codes in the journal: fail2ban and grep match the plain text (ADR-043).
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
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
    // Video redirection (F040): the native messaging host of the host browser connects to this socket.
    if let Some(socket) = farglass_session::redirect::socket_path() {
        match session.redirect.listen(&socket) {
            Ok(()) => tracing::info!(socket = %socket.display(), "video redirection listens"),
            Err(error) => tracing::warn!(%error, "no video redirection"),
        }
        // In a web session, the browsers of the user find the native messaging host.
        if std::env::var_os("XDG_SESSION_ID").is_some()
            && let Ok(program) = std::env::current_exe()
            && let Err(error) = farglass_session::redirect::install_manifests(&program)
        {
            tracing::warn!(%error, "the native messaging host of video redirection is not installed");
        }
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
        let result = match BROKER.get() {
            Some(broker) => match brokered_link(broker).await {
                Ok(stream) => link::connect_stream(stream, &mut session).await,
                Err(error) => Err(error.into()),
            },
            None => link::connect(&path, &mut session).await,
        };
        match result {
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
