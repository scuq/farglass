//! The TLS listener of farglassd.

use std::fmt;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto;
use hyper_util::service::TowerToHyperService;
use tokio::net::{TcpListener, TcpStream, UnixListener};
use tokio_rustls::TlsAcceptor;

use crate::agents::{self, BindError, Registry};
use crate::config::{Config, ConfigError};
use crate::connection::Context;
use crate::http;
use crate::login::Identity;
use crate::origin::OriginPolicy;
use crate::tls::{CertStore, TlsError, TlsSource};

/// A client must complete the TLS handshake in this time.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// A client must send the headers of an HTTP/1.1 request in this time.
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

/// An error from the start of the server.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// The configuration is not valid.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// The certificate did not load.
    #[error(transparent)]
    Tls(#[from] TlsError),

    /// The agent socket did not start.
    #[error(transparent)]
    AgentSocket(#[from] BindError),

    /// The listener did not start.
    #[error("The listener on {addr} did not start: {source}.")]
    Bind {
        /// The address from the configuration.
        addr: SocketAddr,
        /// The cause.
        source: io::Error,
    },
}

/// A server that has a certificate and a listener and is ready to run.
pub struct Server {
    listener: TcpListener,
    local_addr: SocketAddr,
    certs: Arc<CertStore>,
    acceptor: TlsAcceptor,
    router: Router,
    agent_listener: UnixListener,
    registry: Arc<Registry>,
    context: Arc<Context>,
    /// Reports whether `dev_user` is active. Then each connection must come from loopback.
    dev_mode: bool,
    /// The QUIC endpoint for HTTP/3 and WebTransport.
    quic: Option<quinn::Endpoint>,
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("local_addr", &self.local_addr)
            .field("certs", &self.certs)
            .field("registry", &self.registry)
            .field("dev_mode", &self.dev_mode)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// Loads the certificate, creates the agent socket, and starts the listener.
    ///
    /// # Errors
    ///
    /// [`ServerError`] if the configuration, the certificate, or the listener is not usable.
    pub async fn bind(config: &Config) -> Result<Self, ServerError> {
        let dev_user = config.dev_uid()?.map(Identity::from_uid);
        let source = config.tls_source()?;
        let hsts = matches!(source, TlsSource::Files { .. });
        let certs = CertStore::open(source)?;
        let acceptor = TlsAcceptor::from(Arc::new(certs.server_config()?));
        let bind_error = |source| ServerError::Bind {
            addr: config.listen,
            source,
        };
        let listener = TcpListener::bind(config.listen).await.map_err(bind_error)?;
        let local_addr = listener.local_addr().map_err(bind_error)?;
        let agent_listener = agents::bind(&config.agent_socket)?;
        let registry = Registry::new();
        let dev_mode = dev_user.is_some();
        // HTTP/3 is optional: if the UDP port does not open, the WebSocket stays.
        let quic = if config.http3 {
            match crate::quic::QuicCerts::for_store(&certs).and_then(|quic_certs| {
                Ok((
                    crate::quic::bind(local_addr, quic_certs.clone(), certs.provider())?,
                    quic_certs,
                ))
            }) {
                Ok(quic) => Some(quic),
                Err(error) => {
                    tracing::warn!(%error, "no HTTP/3 and no WebTransport");
                    None
                }
            }
        } else {
            None
        };
        let transport = quic.as_ref().map(|(_, quic_certs)| {
            Arc::new(crate::quic::TransportInfo {
                port: local_addr.port(),
                certs: quic_certs.clone(),
                tickets: crate::quic::Tickets::default(),
            })
        });
        // Alt-Svc only with a certificate of the administrator: a browser uses HTTP/3 for pages only with a trusted certificate.
        let alt_svc = (hsts && quic.is_some())
            .then(|| {
                axum::http::HeaderValue::from_str(&format!(
                    "h3=\":{}\"; ma=86400",
                    local_addr.port()
                ))
                .ok()
            })
            .flatten();
        let context = Arc::new(Context {
            dev_user,
            origin: OriginPolicy {
                extra: config.allowed_origins.clone(),
                loopback_only: dev_mode,
            },
            hsts,
            transport,
            alt_svc,
            devices: Arc::new(crate::device::DeviceKeys::load(
                &config.state_dir.join("device.key"),
            )),
            passkeys: Some(Arc::new(crate::passkeys::PasskeyStore::new(
                config.state_dir.join("passkeys"),
            ))),
            idle_logout: (config.idle_logout_minutes > 0)
                .then(|| Duration::from_secs(config.idle_logout_minutes * 60)),
            idle_lock: (config.idle_lock_minutes > 0)
                .then(|| Duration::from_secs(config.idle_lock_minutes * 60)),
            ..Context::new(registry.clone(), config.login_socket.clone())
        });
        Ok(Self {
            listener,
            local_addr,
            certs,
            acceptor,
            router: http::router(&config.web_root, context.clone()),
            context,
            agent_listener,
            registry,
            dev_mode,
            quic: quic.map(|(endpoint, _)| endpoint),
        })
    }

    /// Returns the address of the listener.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Returns the certificate store of the server.
    #[must_use]
    pub fn certs(&self) -> Arc<CertStore> {
        self.certs.clone()
    }

    /// Returns the sessions of the cookies.
    #[must_use]
    pub fn sessions(&self) -> Arc<crate::cookie::SessionStore> {
        self.context.sessions.clone()
    }

    /// Returns the registry of the connected agents.
    #[must_use]
    pub fn registry(&self) -> Arc<Registry> {
        self.registry.clone()
    }

    /// Accepts connections until `shutdown` completes.
    ///
    /// Each connection runs in its own task. A connection error stops only that connection.
    pub async fn run(self, shutdown: impl Future<Output = ()>) {
        let mut builder = auto::Builder::new(TokioExecutor::new());
        builder
            .http1()
            .timer(TokioTimer::new())
            .header_read_timeout(HEADER_TIMEOUT);
        builder
            .http2()
            .timer(TokioTimer::new())
            // WebSocket through HTTP/2 uses the extended CONNECT method (RFC 8441).
            .enable_connect_protocol();
        let builder = Arc::new(builder);
        let agent_loop = tokio::spawn(agents::accept_loop(
            self.agent_listener,
            self.registry.clone(),
        ));
        let idle_loop = tokio::spawn(idle_loop(self.context.clone()));
        let lock_loop = tokio::spawn(crate::lock::agent_loop(
            self.context.presence.clone(),
            self.context.registry.clone(),
        ));
        let quic_loop = self.quic.clone().map(|endpoint| {
            tracing::info!(
                port = self.local_addr.port(),
                "HTTP/3 and WebTransport on UDP"
            );
            tokio::spawn(crate::quic::accept_loop(
                endpoint,
                self.router.clone(),
                self.context.clone(),
                self.dev_mode,
            ))
        });

        tokio::pin!(shutdown);
        loop {
            let accepted = tokio::select! {
                () = &mut shutdown => {
                    agent_loop.abort();
                    idle_loop.abort();
                    lock_loop.abort();
                    if let Some(endpoint) = &self.quic {
                        endpoint.close(0_u32.into(), b"shutdown");
                    }
                    if let Some(quic_loop) = quic_loop {
                        quic_loop.abort();
                    }
                    return;
                }
                accepted = self.listener.accept() => accepted,
            };
            let (stream, peer) = match accepted {
                Ok(accepted) => accepted,
                Err(error) => {
                    // For example, the process has no more file descriptors.
                    tracing::warn!(%error, "the listener did not accept a connection");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            if self.dev_mode && !peer.ip().to_canonical().is_loopback() {
                tracing::warn!(%peer, "dev_user mode refused a connection that is not from loopback");
                continue;
            }
            let acceptor = self.acceptor.clone();
            // The handlers read the address of the browser from this extension.
            let router = self
                .router
                .clone()
                .layer(axum::Extension(axum::extract::ConnectInfo(peer)));
            let builder = builder.clone();
            tokio::spawn(async move {
                if let Err(error) = serve(stream, acceptor, router, &builder).await {
                    tracing::debug!(%peer, %error, "the connection failed");
                }
            });
        }
    }
}

/// The interval between two examinations for the idle logout and the idle lock.
const IDLE_INTERVAL: Duration = Duration::from_secs(15);

/// Logs out each user who had no browser connection for the idle time, and
/// locks each session with no keyboard or pointer activity for the lock time.
async fn idle_loop(context: Arc<Context>) {
    if context.idle_logout.is_none() && context.idle_lock.is_none() {
        return;
    }
    let mut interval = tokio::time::interval(IDLE_INTERVAL);
    loop {
        interval.tick().await;
        if let Some(timeout) = context.idle_logout {
            logout_idle(&context, timeout).await;
        }
        if let Some(timeout) = context.idle_lock {
            lock_idle(&context, timeout);
        }
    }
}

/// Logs out each user with an agent who had no browser connection for `timeout`.
pub async fn logout_idle(context: &Context, timeout: Duration) {
    let uids = context.registry.uids();
    for uid in context
        .presence
        .idle_users(&uids, timeout, std::time::Instant::now())
    {
        crate::login::logout(context, uid, "idle").await;
    }
}

/// Locks the session of each user with an agent who had no keyboard or pointer activity for `timeout`.
pub fn lock_idle(context: &Context, timeout: Duration) {
    let uids = context.registry.uids();
    for uid in context
        .presence
        .inactive_users(&uids, timeout, std::time::Instant::now())
    {
        crate::lock::lock(&context.presence, &context.registry, uid, "idle");
    }
}

/// Does the TLS handshake and serves HTTP on one connection.
async fn serve(
    stream: TcpStream,
    acceptor: TlsAcceptor,
    router: Router,
    builder: &auto::Builder<TokioExecutor>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Send small frames immediately. Terminal input must not wait.
    stream.set_nodelay(true)?;
    let tls = tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await??;
    builder
        .serve_connection_with_upgrades(TokioIo::new(tls), TowerToHyperService::new(router))
        .await
}
