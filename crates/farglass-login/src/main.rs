//! farglass-login does the PAM login of farglass and starts the web session.
//!
//! It runs as root and has no network access. farglassd connects to its Unix
//! socket for each login attempt. Normally systemd gives the socket
//! (farglass-login.socket). With no socket from systemd, farglass-login creates
//! the socket at --socket.
//!
//! After a login, farglass-login starts the session worker in a transient
//! systemd unit: `farglass-login --session-worker USER`. The worker opens a
//! PAM session of type web and runs the agent as the user.
//!
//! Usage:
//!
//!     farglass-login [--socket FILE] [--pam-service NAME] [--pam-dir DIR] [--allow-root]
//!                    [--session systemd|none]
//!     farglass-login --session-worker USER [--session-pam-service NAME] [--pam-dir DIR]

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use farglass_login::host::{Host, NoSessions, Systemd, WorkerSettings};
use farglass_login::server::{Service, Settings};
use farglass_login::{DEFAULT_LOGIN_SOCKET, activation, worker};
use tracing_subscriber::EnvFilter;

/// How farglass-login starts the web session after a login.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum SessionMode {
    /// A logind session through systemd-run and pam_systemd. Needs root.
    Systemd,
    /// No session. For development and tests: the agent must already run.
    None,
}

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

    /// The PAM service of the login
    #[arg(long, value_name = "NAME", default_value = "farglass")]
    pam_service: String,

    /// A directory that replaces /etc/pam.d, for tests
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

    /// How to start the web session after a login
    #[arg(long, value_enum, default_value = "systemd")]
    session: SessionMode,

    /// The PAM service of the web session
    #[arg(long, value_name = "NAME", default_value = "farglass-session")]
    session_pam_service: String,

    /// The program of the agent
    #[arg(
        long,
        value_name = "FILE",
        default_value = "/usr/libexec/farglass/farglass-session"
    )]
    agent_program: PathBuf,

    /// The agent socket of farglassd
    #[arg(long, value_name = "FILE", default_value = "/run/farglass/agent.sock")]
    agent_socket: PathBuf,

    /// Run as the session worker of USER. farglass-login uses this mode itself
    #[arg(long, value_name = "USER")]
    session_worker: Option<String>,
}

fn parse_mode(text: &str) -> Result<u32, String> {
    u32::from_str_radix(text, 8).map_err(|error| format!("not an octal mode: {error}"))
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let args = Args::parse();

    if let Some(user) = &args.session_worker {
        return match worker::run(
            user,
            &args.session_pam_service,
            args.pam_dir.as_deref(),
            &args.agent_program,
            &args.agent_socket,
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("farglass-login: The web session of {user} failed: {error}.");
                ExitCode::FAILURE
            }
        };
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("farglass-login: The runtime did not start: {error}.");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(serve(args))
}

async fn serve(args: Args) -> ExitCode {
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

    let host: Arc<dyn Host> = match args.session {
        SessionMode::None => Arc::new(NoSessions),
        SessionMode::Systemd => {
            let exe = match std::env::current_exe() {
                Ok(exe) => exe,
                Err(error) => {
                    eprintln!("farglass-login: The path of the program is unknown: {error}.");
                    return ExitCode::FAILURE;
                }
            };
            Arc::new(Systemd {
                worker: WorkerSettings {
                    exe,
                    pam_service: args.session_pam_service,
                    pam_dir: args.pam_dir.clone(),
                    agent_program: args.agent_program,
                    agent_socket: args.agent_socket,
                },
            })
        }
    };
    let settings = Settings {
        service: args.pam_service,
        pam_dir: args.pam_dir,
        allow_root: args.allow_root,
        fail_delay: Duration::from_millis(args.fail_delay_ms),
        max_attempts: args.max_attempts.max(1),
        host,
    };
    tracing::info!(service = settings.service, allow_root = settings.allow_root, session = ?args.session, "ready");
    tokio::select! {
        () = Service::new(settings).run(listener) => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    ExitCode::SUCCESS
}
