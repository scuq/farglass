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
use crate::http::{self, AppState};
use crate::tls::{CertStore, TlsError};

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
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("local_addr", &self.local_addr)
            .field("certs", &self.certs)
            .field("registry", &self.registry)
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
        let certs = CertStore::open(config.tls_source()?)?;
        let acceptor = TlsAcceptor::from(Arc::new(certs.server_config()?));
        let bind_error = |source| ServerError::Bind {
            addr: config.listen,
            source,
        };
        let listener = TcpListener::bind(config.listen).await.map_err(bind_error)?;
        let local_addr = listener.local_addr().map_err(bind_error)?;
        let agent_listener = agents::bind(&config.agent_socket)?;
        let registry = Registry::new();
        let state = Arc::new(AppState {
            registry: registry.clone(),
            user: None,
        });
        Ok(Self {
            listener,
            local_addr,
            certs,
            acceptor,
            router: http::router(&config.web_root, state),
            agent_listener,
            registry,
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

        tokio::pin!(shutdown);
        loop {
            let accepted = tokio::select! {
                () = &mut shutdown => {
                    agent_loop.abort();
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
            let acceptor = self.acceptor.clone();
            let router = self.router.clone();
            let builder = builder.clone();
            tokio::spawn(async move {
                if let Err(error) = serve(stream, acceptor, router, &builder).await {
                    tracing::debug!(%peer, %error, "the connection failed");
                }
            });
        }
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
