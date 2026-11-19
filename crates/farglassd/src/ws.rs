//! The WebSocket transport of the channel protocol.
//!
//! One binary WebSocket message holds one frame.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::response::Response;
use farglass_proto::{Frame, HEADER_LEN, MAX_PAYLOAD_LEN};
use futures_util::{SinkExt as _, StreamExt as _, future};

use crate::connection::{self, Violation};
use crate::http::AppState;

/// The largest WebSocket message: one frame with the largest payload.
const MAX_MESSAGE_LEN: usize = HEADER_LEN + MAX_PAYLOAD_LEN;

/// Accepts the WebSocket handshake and starts the connection task.
pub async fn upgrade(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(MAX_MESSAGE_LEN)
        .max_frame_size(MAX_MESSAGE_LEN)
        .on_upgrade(move |socket| serve(socket, state))
}

/// Runs one connection until the peer closes it or breaks the protocol.
async fn serve(socket: WebSocket, state: Arc<AppState>) {
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

    let user = state.user;
    if let Err(violation) =
        connection::run(&state.registry, user, &mut incoming, &mut outgoing).await
    {
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
