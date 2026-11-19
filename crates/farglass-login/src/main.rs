//! farglass-login does the PAM login of farglass.
//!
//! It runs as root and has no network access. farglassd connects to its Unix
//! socket for each login attempt. Normally systemd gives the socket
//! (farglass-login.socket). With no socket from systemd, farglass-login creates
//! the socket at --socket.
//!
//! Usage:
//!
//!     farglass-login [--socket FILE] [--pam-service NAME] [--pam-dir DIR] [--allow-root]

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use farglass_login::server::{Service, Settings};
use farglass_login::{DEFAULT_LOGIN_SOCKET, activation};
use tracing_subscriber::EnvFilter;

/// The PAM login of farglass.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// The socket to create when systemd gives no socket
    #[arg(long, value_name = "FILE", default_value = DEFAULT_LOGIN_SOCKET)]
    socket: PathBuf,

    /// The permissions of the socket that farglass-login creates, in octal
    #[arg(long, value_name = "MODE", default_value = "600", value_parser = parse_mode)]
    socket_mode: u32,

    /// The PAM service
    #[arg(long, value_name = "NAME", default_value = "farglass")]
    pam_service: String,

    /// A directory that replaces /etc/pam.d, for tests with no root
    #[arg(long, value_name = "DIR")]
    pam_dir: Option<PathBuf>,

    /// Let root log in
    #[arg(long)]
    allow_root: bool,

    /// The wait after a failed login in milliseconds
    #[arg(long, value_name = "MS", default_value_t = 2000)]
    fail_delay_ms: u64,

    /// The largest number of logins at the same time
    #[arg(long, value_name = "N", default_value_t = 32)]
    max_attempts: usize,
}

fn parse_mode(text: &str) -> Result<u32, String> {
    u32::from_str_radix(text, 8).map_err(|error| format!("not an octal mode: {error}"))
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let args = Args::parse();

    let listener = match activation::from_systemd() {
        Ok(Some(listener)) => listener,
        Ok(None) => match activation::bind(&args.socket, args.socket_mode) {
            Ok(listener) => {
                tracing::info!(socket = %args.socket.display(), "created the login socket");
                listener
            }
            Err(error) => {
                eprintln!(
                    "farglass-login: The socket {} did not start: {error}.",
                    args.socket.display()
                );
                return ExitCode::FAILURE;
            }
        },
        Err(error) => {
            eprintln!("farglass-login: The socket from systemd is not usable: {error}.");
            return ExitCode::FAILURE;
        }
    };

    let settings = Settings {
        service: args.pam_service,
        pam_dir: args.pam_dir,
        allow_root: args.allow_root,
        fail_delay: Duration::from_millis(args.fail_delay_ms),
        max_attempts: args.max_attempts.max(1),
    };
    tracing::info!(
        service = settings.service,
        allow_root = settings.allow_root,
        "ready"
    );
    tokio::select! {
        () = Service::new(settings).run(listener) => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    ExitCode::SUCCESS
}
