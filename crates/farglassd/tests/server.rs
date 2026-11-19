//! Starts farglassd on a loopback port and uses it as a browser does.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use farglass_proto::message::{Close, CloseReason, ConnectionMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglassd::tls::TlsSource;
use farglassd::{Config, Server};
use futures_util::{SinkExt as _, StreamExt as _};
use http_body_util::{BodyExt as _, Empty};
use hyper::header::{ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE, HOST};
use hyper::{Method, Request, Response, StatusCode, Version};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const INDEX: &str = "<!doctype html><title>farglass</title>";

struct TestServer {
    addr: SocketAddr,
    /// The certificate that the server uses. The client trusts only this certificate.
    cert: CertificateDer<'static>,
    _dir: TempDir,
}

impl TestServer {
    /// Starts a server with a self-signed certificate and a small web root.
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let web_root = dir.path().join("web");
        std::fs::create_dir_all(web_root.join("assets")).unwrap();
        std::fs::write(web_root.join("index.html"), INDEX).unwrap();
        std::fs::write(
            web_root.join("assets/index-BcmqJDP6.js"),
            "console.log('farglass');\n".repeat(200),
        )
        .unwrap();
        std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();

        let config = Config {
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            web_root,
            state_dir: dir.path().join("state"),
            agent_socket: dir.path().join("agent.sock"),
            ..Config::default()
        };
        Self::start_with(config, dir).await
    }

    async fn start_with(config: Config, dir: TempDir) -> Self {
        let cert_path = match config.tls_source().unwrap() {
            TlsSource::SelfSigned { dir, .. } => dir.join("self-signed-cert.pem"),
            TlsSource::Files { cert, .. } => cert,
        };
        let server = Server::bind(&config).await.unwrap();
        let addr = server.local_addr();
        tokio::spawn(server.run(std::future::pending()));
        Self {
            addr,
            cert: CertificateDer::from_pem_file(cert_path).unwrap(),
            _dir: dir,
        }
    }

    /// Connects through TLS and offers the protocols `alpn`.
    async fn connect(&self, alpn: &[&str]) -> TlsStream<TcpStream> {
        let mut roots = RootCertStore::empty();
        roots.add(self.cert.clone()).unwrap();
        let mut config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = alpn.iter().map(|p| p.as_bytes().to_vec()).collect();

        let tcp = TcpStream::connect(self.addr).await.unwrap();
        TlsConnector::from(Arc::new(config))
            .connect(ServerName::try_from("localhost").unwrap(), tcp)
            .await
            .unwrap()
    }

    /// Sends a GET request through HTTP/2.
    async fn get(&self, path: &str) -> Response<Bytes> {
        self.get_with(path, &["h2"], &[]).await
    }

    async fn get_with(
        &self,
        path: &str,
        alpn: &[&str],
        headers: &[(hyper::header::HeaderName, &str)],
    ) -> Response<Bytes> {
        let tls = self.connect(alpn).await;
        let h2 = tls.get_ref().1.alpn_protocol() == Some(b"h2");
        let mut request =
            Request::get(format!("https://localhost{path}")).header(HOST, "localhost");
        for (name, value) in headers {
            request = request.header(name, *value);
        }
        let request = request.body(Empty::<Bytes>::new()).unwrap();

        let response = if h2 {
            let (mut sender, conn) =
                hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(tls))
                    .await
                    .unwrap();
            tokio::spawn(conn);
            sender.send_request(request).await.unwrap()
        } else {
            let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(tls))
                .await
                .unwrap();
            tokio::spawn(conn);
            sender.send_request(request).await.unwrap()
        };
        let (parts, body) = response.into_parts();
        let body = body.collect().await.unwrap().to_bytes();
        Response::from_parts(parts, body)
    }
}

fn header(response: &Response<Bytes>, name: hyper::header::HeaderName) -> &str {
    response
        .headers()
        .get(&name)
        .map_or("", |value| value.to_str().unwrap())
}

#[tokio::test]
async fn index_loads_through_http2_and_the_browser_asks_again_each_time() {
    let server = TestServer::start().await;
    let response = server.get("/").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.version(), Version::HTTP_2);
    assert_eq!(header(&response, CACHE_CONTROL), "no-cache");
    assert_eq!(header(&response, CONTENT_TYPE), "text/html");
    assert_eq!(response.body(), INDEX);
}

#[tokio::test]
async fn index_loads_through_http1() {
    let server = TestServer::start().await;
    let response = server.get_with("/", &["http/1.1"], &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.version(), Version::HTTP_11);
    assert_eq!(response.body(), INDEX);
}

#[tokio::test]
async fn server_selects_http2_when_the_client_offers_the_two_protocols() {
    let server = TestServer::start().await;
    let tls = server.connect(&["http/1.1", "h2"]).await;
    assert_eq!(tls.get_ref().1.alpn_protocol(), Some(&b"h2"[..]));
}

#[tokio::test]
async fn hashed_file_is_immutable_and_compressed() {
    let server = TestServer::start().await;
    let path = "/assets/index-BcmqJDP6.js";

    let plain = server.get(path).await;
    assert_eq!(plain.status(), StatusCode::OK);
    assert_eq!(
        header(&plain, CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(header(&plain, CONTENT_TYPE), "text/javascript");
    assert_eq!(
        header(&plain, "x-content-type-options".parse().unwrap()),
        "nosniff"
    );

    let compressed = server
        .get_with(path, &["h2"], &[(ACCEPT_ENCODING, "br")])
        .await;
    assert_eq!(header(&compressed, CONTENT_ENCODING), "br");
    assert!(compressed.body().len() < plain.body().len() / 4);
}

#[tokio::test]
async fn missing_file_and_a_path_outside_the_web_root_give_404() {
    let server = TestServer::start().await;
    for path in [
        "/missing.js",
        "/assets/missing.js",
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/assets/../../secret.txt",
    ] {
        let response = server.get_with(path, &["http/1.1"], &[]).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(header(&response, CACHE_CONTROL), "no-store", "{path}");
        assert!(!response.body().starts_with(b"secret"), "{path}");
    }
}

#[tokio::test]
async fn dev_user_refuses_an_address_that_is_not_loopback() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        listen: "0.0.0.0:0".parse().unwrap(),
        state_dir: dir.path().join("state"),
        agent_socket: dir.path().join("agent.sock"),
        dev_user: Some("1000".to_owned()),
        ..Config::default()
    };
    let error = Server::bind(&config).await.unwrap_err();
    assert!(error.to_string().contains("loopback"), "{error}");
}

#[tokio::test]
async fn certificate_files_of_the_administrator_serve_requests() {
    // Make a pair in a separate directory and give it to the server as external files.
    let dir = tempfile::tempdir().unwrap();
    let pair = dir.path().join("pair");
    farglassd::tls::CertStore::open(TlsSource::SelfSigned {
        dir: pair.clone(),
        names: vec![],
    })
    .unwrap();
    let web_root = dir.path().join("web");
    std::fs::create_dir_all(&web_root).unwrap();
    std::fs::write(web_root.join("index.html"), INDEX).unwrap();

    let mut config = Config {
        listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        web_root,
        state_dir: dir.path().join("state"),
        agent_socket: dir.path().join("agent.sock"),
        ..Config::default()
    };
    config.tls.cert = Some(pair.join("self-signed-cert.pem"));
    config.tls.key = Some(pair.join("self-signed-key.pem"));
    let state_dir = config.state_dir.clone();

    let server = TestServer::start_with(config, dir).await;
    assert_eq!(server.get("/").await.status(), StatusCode::OK);
    assert!(!Path::new(&state_dir).join("tls").exists());
}

type Ws<S> = WebSocketStream<S>;

async fn next_frame<S>(ws: &mut Ws<S>) -> Frame
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match ws.next().await.unwrap().unwrap() {
        Message::Binary(bytes) => Frame::from_message(bytes).unwrap(),
        other => panic!("not a binary message: {other:?}"),
    }
}

async fn send_frame<S>(ws: &mut Ws<S>, frame: Frame)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    ws.send(Message::Binary(frame.to_bytes())).await.unwrap();
}

fn hello(version: u16) -> Frame {
    Frame::connection(&ConnectionMessage::Hello { version }).unwrap()
}

/// Does the protocol exchange that each transport must give.
async fn hello_then_refused_channel<S>(ws: &mut Ws<S>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let first = next_frame(ws).await;
    assert!(first.channel().is_connection());
    assert_eq!(
        first.parse::<ConnectionMessage>().unwrap(),
        ConnectionMessage::Hello {
            version: PROTOCOL_VERSION
        }
    );

    send_frame(ws, hello(PROTOCOL_VERSION)).await;
    let channel = ChannelId::new(1);
    send_frame(ws, Frame::open(channel, &Open::Echo).unwrap()).await;
    // The opener can send data before the answer. The server drops it.
    // The connection has no login, thus the server refuses the channel.
    send_frame(ws, Frame::data(channel, &b"early"[..]).unwrap()).await;

    let answer = next_frame(ws).await;
    assert_eq!(answer.channel(), channel);
    assert_eq!(answer.frame_type(), FrameType::Close);
    assert_eq!(
        answer.parse::<Close>().unwrap().reason,
        CloseReason::AccessDenied
    );

    ws.close(None).await.unwrap();
}

async fn websocket_http1(server: &TestServer) -> Ws<TlsStream<TcpStream>> {
    let tls = server.connect(&["http/1.1"]).await;
    let (ws, response) = tokio_tungstenite::client_async("wss://localhost/ws", tls)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    ws
}

#[tokio::test]
async fn websocket_works_through_http1() {
    let server = TestServer::start().await;
    let mut ws = websocket_http1(&server).await;
    hello_then_refused_channel(&mut ws).await;
}

#[tokio::test]
async fn websocket_works_through_http2_extended_connect() {
    let server = TestServer::start().await;
    let tls = server.connect(&["h2"]).await;
    let (mut sender, conn) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(tls))
            .await
            .unwrap();
    tokio::spawn(conn);

    // The server sends SETTINGS_ENABLE_CONNECT_PROTOCOL. Wait until the client has it.
    sender.ready().await.unwrap();
    let request = Request::builder()
        .method(Method::CONNECT)
        .uri("https://localhost/ws")
        .extension(hyper::ext::Protocol::from_static("websocket"))
        .header("sec-websocket-version", "13")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let upgraded = hyper::upgrade::on(response).await.unwrap();
    let mut ws = WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Client, None).await;
    hello_then_refused_channel(&mut ws).await;
}

/// Sends `messages` after the hello of the server and returns the close code of the server.
async fn close_code_after(messages: Vec<Message>) -> CloseCode {
    let server = TestServer::start().await;
    let mut ws = websocket_http1(&server).await;
    next_frame(&mut ws).await;
    for message in messages {
        ws.send(message).await.unwrap();
    }
    loop {
        match ws.next().await {
            Some(Ok(Message::Close(Some(frame)))) => return frame.code,
            Some(Ok(_)) => {}
            other => panic!("the server did not send a close frame: {other:?}"),
        }
    }
}

fn binary(frame: Frame) -> Message {
    Message::Binary(frame.to_bytes())
}

#[tokio::test]
async fn protocol_violations_close_the_connection() {
    let open = |channel| binary(Frame::open(ChannelId::new(channel), &Open::Echo).unwrap());
    let cases: Vec<(&str, Vec<Message>)> = vec![
        (
            "a different version",
            vec![binary(hello(PROTOCOL_VERSION + 1))],
        ),
        ("no hello before open", vec![open(1)]),
        ("a text message", vec![Message::Text("hello".into())]),
        (
            "an unknown frame type",
            vec![Message::Binary(Bytes::from_static(&[
                1, 0, 0, 0, 9, 0, 0, 0,
            ]))],
        ),
        (
            "a channel number of the host",
            vec![binary(hello(PROTOCOL_VERSION)), open(2)],
        ),
        (
            "a connection message that is not valid",
            vec![binary(
                Frame::new(ChannelId::CONNECTION, FrameType::Control, &b"{}"[..]).unwrap(),
            )],
        ),
    ];
    for (name, messages) in cases {
        assert_eq!(
            close_code_after(messages).await,
            CloseCode::Protocol,
            "{name}"
        );
    }
}
