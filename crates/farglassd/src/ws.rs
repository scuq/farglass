//! The WebSocket transport of the channel protocol.
//!
//! One binary WebSocket message holds one frame.

use std::sync::Arc;

use std::net::SocketAddr;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse as _, Response};
use farglass_proto::{Frame, HEADER_LEN, MAX_PAYLOAD_LEN};
use futures_util::{SinkExt as _, StreamExt as _, future};

use crate::connection::{self, Context, Violation};

/// The largest WebSocket message: one frame with the largest payload.
const MAX_MESSAGE_LEN: usize = HEADER_LEN + MAX_PAYLOAD_LEN;

/// Accepts the WebSocket handshake and starts the connection task.
///
/// The handshake must come from a page of this server (the Origin check).
/// The user of the connection comes from `dev_user` or from the session cookie.
pub async fn upgrade(
    State(context): State<Arc<Context>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: Uri,
    ws: WebSocketUpgrade,
) -> Response {
    if !context.origin.allows(&headers, &uri) {
        tracing::warn!(%peer, origin = ?headers.get("origin"), "refused a WebSocket from a different origin");
        crate::http::origin_event(&peer.ip().to_canonical().to_string()).refused();
        return StatusCode::FORBIDDEN.into_response();
    }
    // The Origin check passed: the header names the page of the shell.
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let user = context
        .dev_user
        .clone()
        .or_else(|| context.sessions.user(&headers));
    let client = connection::Client {
        addr: Some(peer.ip()),
        user,
        origin,
        device: context.devices.valid(&headers),
    };
    ws.max_message_size(MAX_MESSAGE_LEN)
        .max_frame_size(MAX_MESSAGE_LEN)
        .on_upgrade(move |socket| serve(socket, context, client))
}

/// Runs one connection until the peer closes it or breaks the protocol.
async fn serve(socket: WebSocket, context: Arc<Context>, client: connection::Client) {
    let (sink, stream) = socket.split();
    let mut outgoing =
        sink.with(|frame: Frame| future::ok::<_, axum::Error>(Message::Binary(frame.to_bytes())));
    let mut incoming = stream
        // A receive error ends the connection, as the end of the stream does.
        .take_while(|message| future::ready(message.is_ok()))
        .filter_map(|message| {
            future::ready(match message {
                Ok(Message::Binary(bytes)) => {
                    Some(Frame::from_message(bytes).map_err(Violation::from))
                }
                Ok(Message::Text(_)) => Some(Err(Violation::NotBinary)),
                // axum answers a ping. A close message ends the stream after it.
                Ok(Message::Ping(_) | Message::Pong(_) | Message::Close(_)) | Err(_) => None,
            })
        });

    if let Err(violation) = connection::run(&context, client, &mut incoming, &mut outgoing).await {
        tracing::debug!(%violation, "closing the WebSocket connection");
        let frame = CloseFrame {
            code: close_code::PROTOCOL,
            // A WebSocket close reason has a limit of 123 bytes. The messages are shorter.
            reason: violation.to_string().into(),
        };
        let _ = outgoing
            .into_inner()
            .send(Message::Close(Some(frame)))
            .await;
    }
}
