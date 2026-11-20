//! HTTP/3 and WebTransport on QUIC (ADR-005, F023).
//!
//! The QUIC endpoint uses the UDP port with the number of the TCP port. It
//! serves the same routes as HTTP/1.1 and HTTP/2, and WebTransport sessions at
//! [`WT_PATH`]. A WebTransport session carries the channel protocol: each
//! channel has its own bidirectional stream, thus a late frame of one channel
//! does not stop the frames of the other channels.
//!
//! The browser opens one stream for each channel. The first frame on a stream
//! selects its channel: the hello message for the connection channel, the
//! open frame for each other channel. Each later frame on the stream must have
//! that channel. farglassd sends the frames of a channel on its stream, and
//! ends the stream after its close frame.
//!
//! The browser gets a single-use ticket from `POST /transport` first, because a
//! WebTransport request has no cookie. With a self-signed certificate, the QUIC
//! endpoint uses a separate certificate of at most 14 days, which the browser
//! accepts through its SHA-256 hash (`serverCertificateHashes`).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use axum::Router;
use bytes::{Buf as _, Bytes, BytesMut};
use farglass_proto::codec::{CodecError, FrameCodec};
use farglass_proto::{ChannelId, Frame, FrameType};
use futures_util::{SinkExt as _, StreamExt as _};
use h3::ext::Protocol;
use h3::server::RequestStream;
use h3_webtransport::server::{AcceptedBi, WebTransportSession};
use http::{Method, Request, Response, StatusCode};
use http_body_util::BodyExt as _;
use rcgen::{
    CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, KeyPair, KeyUsagePurpose,
};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use sha2::{Digest as _, Sha256};
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite};
use tower::ServiceExt as _;

use crate::connection::{self, Context, Violation};
use crate::login::Identity;
use crate::tls::CertStore;

/// The path of the WebTransport endpoint.
pub const WT_PATH: &str = "/wt";

/// The validity of the certificate for pinned hashes. Chrome accepts at most 14 days.
pub const PINNED_VALIDITY: time::Duration = time::Duration::days(10);

/// farglassd makes a new pinned certificate when the old one expires in less than this time.
const PINNED_RENEW_BEFORE: time::Duration = time::Duration::days(2);

/// The time in which the browser must use a transport ticket.
pub const TICKET_LIFETIME: Duration = Duration::from_secs(30);

/// The largest body of an HTTP/3 request. The routes of farglassd take small JSON bodies only.
const MAX_BODY: usize = 64 * 1024;

/// The time with no packet before farglassd ends a QUIC connection. The keep-alive keeps an idle tab connected.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const KEEP_ALIVE: Duration = Duration::from_secs(10);

/// An error of the start of the QUIC endpoint.
#[derive(Debug, thiserror::Error)]
pub enum QuicError {
    /// The UDP socket did not open.
    #[error("The UDP port {addr} is not usable: {source}.")]
    Bind {
        /// The address.
        addr: SocketAddr,
        /// The cause.
        source: std::io::Error,
    },
    /// The TLS configuration is not usable for QUIC.
    #[error("The TLS configuration is not usable for QUIC: {0}.")]
    Tls(String),
}

/// The certificate of the QUIC endpoint.
#[derive(Debug)]
pub enum QuicCerts {
    /// The certificate of the administrator, the same as on TCP. The browser checks it as usual.
    Admin(Arc<CertStore>),
    /// A self-signed certificate of at most 14 days that the browser accepts through its hash.
    Pinned(PinnedCert),
}

impl QuicCerts {
    /// Returns the certificates for `store`: the same store for a certificate of
    /// the administrator, a new pinned certificate for a self-signed one.
    ///
    /// # Errors
    ///
    /// [`QuicError::Tls`] if the pinned certificate cannot be made.
    pub fn for_store(store: &Arc<CertStore>) -> Result<Arc<Self>, QuicError> {
        Ok(Arc::new(match store.self_signed() {
            None => Self::Admin(store.clone()),
            Some(names) => Self::Pinned(PinnedCert::new(
                names,
                &store.provider(),
                OffsetDateTime::now_utc(),
            )?),
        }))
    }

    /// Returns the SHA-256 hashes for `serverCertificateHashes`, after a renewal if one is due.
    /// A certificate of the administrator needs no hash.
    pub fn hashes(&self) -> Vec<[u8; 32]> {
        match self {
            Self::Admin(_) => Vec::new(),
            Self::Pinned(pinned) => vec![pinned.current(OffsetDateTime::now_utc()).hash],
        }
    }
}

impl ResolvesServerCert for QuicCerts {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        match self {
            Self::Admin(store) => store.resolve(hello),
            Self::Pinned(pinned) => Some(pinned.current(OffsetDateTime::now_utc()).key.clone()),
        }
    }
}

/// One pinned certificate.
#[derive(Debug)]
struct Pinned {
    key: Arc<CertifiedKey>,
    hash: [u8; 32],
    not_after: OffsetDateTime,
}

/// The self-signed certificate of the QUIC endpoint. It lives in memory only, and a new one replaces it before it expires.
#[derive(Debug)]
pub struct PinnedCert {
    names: Vec<String>,
    provider: Arc<CryptoProvider>,
    current: ArcSwap<Pinned>,
    renew: Mutex<()>,
}

impl PinnedCert {
    fn new(
        names: Vec<String>,
        provider: &Arc<CryptoProvider>,
        now: OffsetDateTime,
    ) -> Result<Self, QuicError> {
        let first = make_pinned(&names, provider, now)?;
        Ok(Self {
            names,
            provider: provider.clone(),
            current: ArcSwap::from_pointee(first),
            renew: Mutex::new(()),
        })
    }

    /// Returns the current certificate. Makes a new one if the current one expires soon.
    fn current(&self, now: OffsetDateTime) -> Arc<Pinned> {
        let current = self.current.load_full();
        if now + PINNED_RENEW_BEFORE < current.not_after {
            return current;
        }
        let _one = self.renew.lock().unwrap_or_else(PoisonError::into_inner);
        let current = self.current.load_full();
        if now + PINNED_RENEW_BEFORE < current.not_after {
            return current;
        }
        match make_pinned(&self.names, &self.provider, now) {
            Ok(next) => {
                tracing::info!("a new certificate for WebTransport replaces the old one");
                let next = Arc::new(next);
                self.current.store(next.clone());
                next
            }
            Err(error) => {
                tracing::error!(%error, "no new certificate for WebTransport");
                current
            }
        }
    }
}

/// Makes a self-signed ECDSA P-256 certificate for pinned hashes.
fn make_pinned(
    names: &[String],
    provider: &CryptoProvider,
    now: OffsetDateTime,
) -> Result<Pinned, QuicError> {
    let tls = |error: &dyn std::fmt::Display| QuicError::Tls(error.to_string());
    let mut params = CertificateParams::new(names.to_vec()).map_err(|e| tls(&e))?;
    // The browser counts the validity from not_before. The whole time must be at most 14 days.
    params.not_before = now - time::Duration::minutes(5);
    params.not_after = now + PINNED_VALIDITY;
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(
        DnType::CommonName,
        format!(
            "farglass WebTransport ({})",
            crate::tls::hostname().as_deref().unwrap_or("localhost")
        ),
    );
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let key_pair = KeyPair::generate().map_err(|e| tls(&e))?;
    let cert = params.self_signed(&key_pair).map_err(|e| tls(&e))?;
    let der: CertificateDer<'static> = cert.der().clone();
    let hash = Sha256::digest(&der).into();
    let private = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));
    let key = CertifiedKey::from_der(vec![der], private, provider).map_err(|e| tls(&e))?;
    Ok(Pinned {
        key: Arc::new(key),
        hash,
        not_after: params.not_after,
    })
}

/// The user of a ticket and the end of its validity.
type TicketEntry = (Option<Identity>, Instant);

/// The single-use tickets of `POST /transport`. Each names the user of the
/// cookie, or no user for a connection that must log in.
#[derive(Debug, Default)]
pub struct Tickets {
    tickets: Mutex<HashMap<[u8; 32], TicketEntry>>,
}

impl Tickets {
    /// Returns a new ticket for `identity`.
    pub fn issue(&self, identity: Option<Identity>) -> String {
        let ticket = crate::cookie::secret();
        let now = Instant::now();
        let mut tickets = self.tickets.lock().unwrap_or_else(PoisonError::into_inner);
        tickets.retain(|_, (_, expires)| *expires > now);
        tickets.insert(
            crate::cookie::hash(&ticket),
            (identity, now + TICKET_LIFETIME),
        );
        ticket
    }

    /// Redeems `ticket` one time. Returns the identity of the ticket, or `None` if the ticket is not valid.
    pub fn redeem(&self, ticket: &str) -> Option<Option<Identity>> {
        let mut tickets = self.tickets.lock().unwrap_or_else(PoisonError::into_inner);
        let (identity, expires) = tickets.remove(&crate::cookie::hash(ticket))?;
        (expires > Instant::now()).then_some(identity)
    }
}

/// What `POST /transport` needs to know about the QUIC endpoint.
#[derive(Debug)]
pub struct TransportInfo {
    /// The UDP port.
    pub port: u16,
    /// The certificates, for the hashes.
    pub certs: Arc<QuicCerts>,
    /// The tickets.
    pub tickets: Tickets,
}

/// Opens the QUIC endpoint on `addr` with `certs`.
///
/// # Errors
///
/// [`QuicError`] if the UDP socket does not open or rustls refuses the configuration.
pub fn bind(
    addr: SocketAddr,
    certs: Arc<QuicCerts>,
    provider: Arc<CryptoProvider>,
) -> Result<quinn::Endpoint, QuicError> {
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| QuicError::Tls(e.to_string()))?
        .with_no_client_auth()
        .with_cert_resolver(certs);
    tls.alpn_protocols = vec![b"h3".to_vec()];
    tls.max_early_data_size = 0;
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|e| QuicError::Tls(e.to_string()))?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    let mut transport = quinn::TransportConfig::default();
    transport
        .max_idle_timeout(Some(IDLE_TIMEOUT.try_into().map_err(
            |e: quinn::VarIntBoundsExceeded| QuicError::Tls(e.to_string()),
        )?))
        .keep_alive_interval(Some(KEEP_ALIVE));
    config.transport_config(Arc::new(transport));
    quinn::Endpoint::server(config, addr).map_err(|source| QuicError::Bind { addr, source })
}

/// Accepts QUIC connections until the endpoint closes. Each connection runs in its own task.
pub async fn accept_loop(
    endpoint: quinn::Endpoint,
    router: Router,
    context: Arc<Context>,
    loopback_only: bool,
) {
    while let Some(incoming) = endpoint.accept().await {
        let peer = incoming.remote_address();
        if loopback_only && !peer.ip().to_canonical().is_loopback() {
            tracing::warn!(%peer, "dev_user mode refused a QUIC connection that is not from loopback");
            incoming.refuse();
            continue;
        }
        let router = router.clone();
        let context = context.clone();
        tokio::spawn(async move {
            if let Err(error) = serve_connection(incoming, router, context).await {
                tracing::info!(%peer, %error, "a QUIC connection ended with an error");
            }
        });
    }
}

type H3Conn = h3::server::Connection<h3_quinn::Connection, Bytes>;
type H3Stream = RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>;

/// Serves one QUIC connection: HTTP/3 requests, and at most one WebTransport session.
async fn serve_connection(
    incoming: quinn::Incoming,
    router: Router,
    context: Arc<Context>,
) -> Result<(), String> {
    let quic = incoming.await.map_err(|e| e.to_string())?;
    let peer = quic.remote_address();
    let mut h3: H3Conn = h3::server::builder()
        .enable_webtransport(true)
        .enable_extended_connect(true)
        .enable_datagram(true)
        .max_webtransport_sessions(1)
        .send_grease(true)
        .build(h3_quinn::Connection::new(quic.clone()))
        .await
        .map_err(|e| e.to_string())?;
    loop {
        let resolver = match h3.accept().await {
            Ok(Some(resolver)) => resolver,
            Ok(None) => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        let (request, mut stream) = match resolver.resolve_request().await {
            Ok(resolved) => resolved,
            Err(error) => {
                tracing::debug!(%peer, %error, "an HTTP/3 request did not resolve");
                continue;
            }
        };
        if request.method() != Method::CONNECT
            || request.extensions().get::<Protocol>() != Some(&Protocol::WEB_TRANSPORT)
        {
            tokio::spawn(serve_request(router.clone(), request, stream));
            continue;
        }
        // A WebTransport session: check the path, the Origin, and the ticket first.
        let identity = match admit(&context, &request) {
            Ok(identity) => identity,
            Err(status) => {
                let _ = stream.send_response(empty(status)).await;
                let _ = stream.finish().await;
                continue;
            }
        };
        let session = WebTransportSession::accept(request, stream, h3)
            .await
            .map_err(|e| e.to_string())?;
        run_session(session, identity, router, context, quic.clone(), peer).await;
        return Ok(());
    }
}

fn empty(status: StatusCode) -> Response<()> {
    let mut response = Response::new(());
    *response.status_mut() = status;
    response
}

/// Checks a WebTransport request. Returns the identity of its ticket.
fn admit(context: &Context, request: &Request<()>) -> Result<Option<Identity>, StatusCode> {
    if request.uri().path() != WT_PATH {
        return Err(StatusCode::NOT_FOUND);
    }
    if !context.origin.allows(request.headers(), request.uri()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let ticket = request
        .uri()
        .query()
        .and_then(|query| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("ticket="))
        })
        .ok_or(StatusCode::FORBIDDEN)?;
    let transport = context.transport.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    transport
        .tickets
        .redeem(ticket)
        .ok_or(StatusCode::FORBIDDEN)
}

/// Serves one HTTP/3 request with the router of the TCP endpoint.
async fn serve_request(router: Router, request: Request<()>, mut stream: H3Stream) {
    let (parts, ()) = request.into_parts();
    let mut body = BytesMut::new();
    loop {
        match stream.recv_data().await {
            Ok(Some(mut chunk)) => {
                if body.len() + chunk.remaining() > MAX_BODY {
                    let _ = stream
                        .send_response(empty(StatusCode::PAYLOAD_TOO_LARGE))
                        .await;
                    let _ = stream.finish().await;
                    return;
                }
                body.extend_from_slice(&chunk.copy_to_bytes(chunk.remaining()));
            }
            Ok(None) => break,
            Err(error) => {
                tracing::debug!(%error, "the body of an HTTP/3 request did not arrive");
                return;
            }
        }
    }
    let request = Request::from_parts(parts, axum::body::Body::from(body.freeze()));
    let Ok(response) = router.oneshot(request).await;
    let (parts, mut body) = response.into_parts();
    if stream
        .send_response(Response::from_parts(parts, ()))
        .await
        .is_err()
    {
        return;
    }
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => {
                if let Ok(data) = frame.into_data()
                    && stream.send_data(data).await.is_err()
                {
                    return;
                }
            }
            Err(_) => return,
        }
    }
    let _ = stream.finish().await;
}

/// The stream of each channel of a session, for the frames of farglassd.
#[derive(Debug, Default)]
struct RouteTable {
    streams: HashMap<ChannelId, mpsc::UnboundedSender<Frame>>,
    /// The frames of the connection channel before the shell opened its stream, for example the hello message.
    waiting: Vec<Frame>,
}

type Routes = Arc<Mutex<RouteTable>>;

/// Runs the channel protocol on a WebTransport session until it ends.
async fn run_session(
    session: WebTransportSession<h3_quinn::Connection, Bytes>,
    identity: Option<Identity>,
    router: Router,
    context: Arc<Context>,
    quic: quinn::Connection,
    peer: SocketAddr,
) {
    tracing::info!(%peer, "WebTransport session started");
    let started = std::time::Instant::now();
    let session = Arc::new(session);
    let routes: Routes = Arc::new(Mutex::new(RouteTable::default()));
    let (to_connection, from_streams) = mpsc::channel::<Result<Frame, Violation>>(64);

    let acceptor = {
        let session = session.clone();
        let routes = routes.clone();
        tokio::spawn(async move {
            loop {
                match session.accept_bi().await {
                    Ok(Some(AcceptedBi::BidiStream(_, stream))) => {
                        tokio::spawn(channel_stream(
                            stream,
                            routes.clone(),
                            to_connection.clone(),
                        ));
                    }
                    Ok(Some(AcceptedBi::Request(request, stream))) => {
                        tokio::spawn(serve_request(router.clone(), request, stream));
                    }
                    Ok(None) | Err(_) => return,
                }
            }
        })
    };

    let mut incoming = tokio_stream_from(from_streams);
    let outgoing = futures_util::sink::unfold(routes.clone(), |routes, frame: Frame| async move {
        route(&routes, frame);
        Ok::<_, std::convert::Infallible>(routes)
    });
    let mut outgoing = std::pin::pin!(outgoing);
    if let Err(violation) = connection::run(
        &context,
        Some(peer.ip()),
        identity,
        &mut incoming,
        &mut outgoing,
    )
    .await
    {
        tracing::warn!(%peer, %violation, "the shell broke the protocol on WebTransport");
    }
    acceptor.abort();
    routes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .streams
        .clear();
    // The cause, for example a timeout of the network or a close by the browser.
    let reason = quic.close_reason().map_or_else(
        || "the connection of the shell ended".to_owned(),
        |reason| reason.to_string(),
    );
    tracing::info!(%peer, seconds = started.elapsed().as_secs(), reason, "WebTransport session ended");
    quic.close(0_u32.into(), b"done");
}

fn tokio_stream_from<T: Send + 'static>(
    mut receiver: mpsc::Receiver<T>,
) -> impl futures_util::Stream<Item = T> + Unpin {
    Box::pin(futures_util::stream::poll_fn(move |cx| {
        receiver.poll_recv(cx)
    }))
}

/// Gives a frame of farglassd to the stream of its channel. After a close frame, the stream ends.
fn route(routes: &Routes, frame: Frame) {
    let mut routes = routes.lock().unwrap_or_else(PoisonError::into_inner);
    let channel = frame.channel();
    let is_close = frame.frame_type() == FrameType::Close;
    match routes.streams.get(&channel) {
        Some(stream) => {
            let _ = stream.send(frame);
        }
        None if channel.is_connection() => routes.waiting.push(frame),
        None => tracing::debug!(%channel, "a frame for a channel with no stream"),
    }
    if is_close {
        routes.streams.remove(&channel);
    }
}

/// Reads the frames of one stream and writes the frames of farglassd for its channel.
async fn channel_stream(
    stream: h3_webtransport::stream::BidiStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    routes: Routes,
    to_connection: mpsc::Sender<Result<Frame, Violation>>,
) {
    let (read, write) = tokio::io::split(stream);
    let mut reader = FramedRead::new(read, FrameCodec);
    let first = match reader.next().await {
        Some(Ok(frame)) => frame,
        Some(Err(CodecError::Frame(error))) => {
            let _ = to_connection.send(Err(Violation::Frame(error))).await;
            return;
        }
        Some(Err(CodecError::Io(_))) | None => return,
    };
    let channel = first.channel();
    let (sender, mut frames) = mpsc::unbounded_channel::<Frame>();
    let duplicate = {
        let mut routes = routes.lock().unwrap_or_else(PoisonError::into_inner);
        let RouteTable { streams, waiting } = &mut *routes;
        match streams.entry(channel) {
            std::collections::hash_map::Entry::Occupied(_) => true,
            std::collections::hash_map::Entry::Vacant(entry) => {
                if channel.is_connection() {
                    for frame in waiting.drain(..) {
                        let _ = sender.send(frame);
                    }
                }
                entry.insert(sender);
                false
            }
        }
    };
    if duplicate {
        let _ = to_connection
            .send(Err(Violation::ChannelNumber(channel)))
            .await;
        return;
    }
    tokio::spawn(async move {
        let mut writer = FramedWrite::new(write, FrameCodec);
        while let Some(frame) = frames.recv().await {
            let is_close = frame.frame_type() == FrameType::Close;
            if writer.send(frame).await.is_err() || is_close {
                break;
            }
        }
        let _ = writer.close().await;
    });
    if to_connection.send(Ok(first)).await.is_err() {
        return;
    }
    while let Some(frame) = reader.next().await {
        let frame = match frame {
            Ok(frame) if frame.channel() == channel => Ok(frame),
            Ok(frame) => Err(Violation::NotOpen(frame.channel())),
            Err(CodecError::Frame(error)) => Err(Violation::Frame(error)),
            // The stream stopped, for example with the session. The connection ends with the session.
            Err(CodecError::Io(_)) => return,
        };
        let stop = frame.is_err();
        if to_connection.send(frame).await.is_err() || stop {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_certificate_is_short_and_renews_before_its_end() {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let now = OffsetDateTime::now_utc();
        let pinned = PinnedCert::new(vec!["localhost".to_owned()], &provider, now).unwrap();
        let first = pinned.current(now);
        assert!(first.not_after - now <= time::Duration::days(14));
        assert_eq!(
            pinned.current(now + time::Duration::days(7)).hash,
            first.hash
        );
        let later = pinned.current(now + time::Duration::days(9));
        assert_ne!(
            later.hash, first.hash,
            "a certificate that ends in less than 2 days is replaced"
        );
    }

    #[test]
    fn tickets_are_single_use() {
        let tickets = Tickets::default();
        let ticket = tickets.issue(None);
        assert_eq!(tickets.redeem(&ticket), Some(None));
        assert_eq!(tickets.redeem(&ticket), None);
        assert_eq!(tickets.redeem("guess"), None);
    }
}
