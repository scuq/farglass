//! farglassd is the web server and the channel router of farglass.
//!
//! It serves the shell files and the WebSocket endpoint through TLS.
//! With no certificate in the configuration, it makes a self-signed certificate.
//!
//! Usage:
//!
//!     farglassd [--config FILE] [--listen ADDR] [--web-root DIR] [--state-dir DIR]
//!               [--agent-socket FILE] [--login-socket FILE] [--dev-user USER] [--tls-cert FILE --tls-key FILE]
//!     farglassd [--config FILE] [--state-dir DIR] --set-pin < PIN
//!     farglassd [--config FILE] [--state-dir DIR] --clear-pin
//!
//! SIGHUP loads the certificate files again. SIGTERM and SIGINT stop the server.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use farglassd::tls::CertStore;
use farglassd::{Config, Server};
use tokio::signal::unix::{SignalKind, signal};
use tracing_subscriber::EnvFilter;

/// The interval between two examinations of the certificate.
const REFRESH_INTERVAL: Duration = Duration::from_secs(3600);

/// The web server and the channel router of farglass.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// The configuration file [default: /etc/farglass/farglassd.conf, if it exists]
    #[arg(long, value_name = "FILE", env = "FARGLASSD_CONFIG")]
    config: Option<PathBuf>,

    /// The address and the port to listen on, for example [::]:8443
    #[arg(long, value_name = "ADDR", env = "FARGLASSD_LISTEN")]
    listen: Option<SocketAddr>,

    /// The directory with the shell files
    #[arg(long, value_name = "DIR", env = "FARGLASSD_WEB_ROOT")]
    web_root: Option<PathBuf>,

    /// The directory for the files that farglassd writes
    #[arg(long, value_name = "DIR", env = "FARGLASSD_STATE_DIR")]
    state_dir: Option<PathBuf>,

    /// The Unix socket that the agents connect to
    #[arg(long, value_name = "FILE", env = "FARGLASSD_AGENT_SOCKET")]
    agent_socket: Option<PathBuf>,

    /// The Unix socket of farglass-login
    #[arg(long, value_name = "FILE", env = "FARGLASSD_LOGIN_SOCKET")]
    login_socket: Option<PathBuf>,

    /// Development only: each browser connection becomes this user with no login.
    /// Works only on a loopback address
    #[arg(long, value_name = "USER", env = "FARGLASSD_DEV_USER")]
    dev_user: Option<String>,

    /// The PEM file with the certificate chain. Use it together with --tls-key
    #[arg(
        long,
        value_name = "FILE",
        env = "FARGLASSD_TLS_CERT",
        requires = "tls_key"
    )]
    tls_cert: Option<PathBuf>,

    /// The PEM file with the private key. Use it together with --tls-cert
    #[arg(
        long,
        value_name = "FILE",
        env = "FARGLASSD_TLS_KEY",
        requires = "tls_cert"
    )]
    tls_key: Option<PathBuf>,

    /// Set the machine PIN of the web login (4 to 6 digits, from standard input) and exit
    #[arg(long, conflicts_with = "clear_pin")]
    set_pin: bool,

    /// Development and tests: let an agent connect as its own user, with no session worker
    #[arg(long, env = "FARGLASSD_ALLOW_USER_AGENTS")]
    allow_user_agents: bool,

    /// Remove the machine PIN of the web login and exit
    #[arg(long)]
    clear_pin: bool,
}

impl Args {
    /// Loads the configuration file and applies the command-line options to it.
    fn into_config(self) -> Result<Config, farglassd::config::ConfigError> {
        let mut config = Config::load(self.config.as_deref())?;
        if let Some(listen) = self.listen {
            config.listen = listen;
        }
        if let Some(web_root) = self.web_root {
            config.web_root = web_root;
        }
        if let Some(state_dir) = self.state_dir {
            config.state_dir = state_dir;
        }
        if let Some(agent_socket) = self.agent_socket {
            config.agent_socket = agent_socket;
        }
        if let Some(login_socket) = self.login_socket {
            config.login_socket = login_socket;
        }
        if self.dev_user.is_some() {
            config.dev_user = self.dev_user;
        }
        if self.allow_user_agents {
            config.allow_user_agents = true;
        }
        if self.tls_cert.is_some() {
            config.tls.cert = self.tls_cert;
            config.tls.key = self.tls_key;
        }
        Ok(config)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        // No color codes in the journal: fail2ban and grep match the plain text (ADR-043).
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        // RUST_LOG sets the log level, for example RUST_LOG=farglassd=debug.
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("farglassd: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let (set_pin, clear_pin) = (args.set_pin, args.clear_pin);
    let config = args.into_config()?;
    if set_pin || clear_pin {
        return change_pin(&config.state_dir.join("pin"), set_pin);
    }
    let server = Server::bind(&config).await?;
    let certs = server.certs();
    log_certificate(&certs);
    if !config.web_root.join("index.html").is_file() {
        tracing::warn!(
            web_root = %config.web_root.display(),
            "the web root has no index.html, build the shell with: npm --prefix web run build"
        );
    }
    if let Some(user) = &config.dev_user {
        tracing::warn!(
            user,
            "dev_user mode: each browser connection from loopback becomes this user with no login"
        );
    }
    tracing::info!("listening on https://{}", server.local_addr());

    let mut hangup = signal(SignalKind::hangup())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        interval.tick().await;
        loop {
            let result = tokio::select! {
                _ = interval.tick() => certs.refresh(),
                _ = hangup.recv() => certs.reload().map(|()| true),
            };
            match result {
                Ok(true) => log_certificate(&certs),
                Ok(false) => {}
                Err(error) => {
                    tracing::error!(%error, "the certificate did not load, the old one stays active");
                }
            }
        }
    });

    server
        .run(async {
            tokio::select! {
                _ = terminate.recv() => {}
                _ = interrupt.recv() => {}
            }
        })
        .await;
    tracing::info!("stopped");
    Ok(())
}

/// Sets the machine PIN from standard input, or removes it (F059).
fn change_pin(path: &std::path::Path, set: bool) -> Result<(), Box<dyn std::error::Error>> {
    let event = farglass_login::audit::Event {
        event: "pin-change",
        reason: if set { "set" } else { "clear" },
        ..Default::default()
    };
    if set {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let pin = zeroize::Zeroizing::new(line.trim().to_owned());
        farglassd::pin::write_pin(path, &pin)?;
        event.accepted();
        println!(
            "The machine PIN is set in {}. Each new login asks for it.",
            path.display()
        );
    } else {
        farglassd::pin::clear_pin(path)?;
        event.accepted();
        println!("The machine PIN is removed.");
    }
    Ok(())
}

fn log_certificate(certs: &Arc<CertStore>) {
    let info = certs.info();
    tracing::info!(
        self_signed = info.self_signed,
        not_after = %info.not_after,
        names = %info.names.join(", "),
        sha256 = %info.fingerprint,
        "certificate loaded"
    );
}
