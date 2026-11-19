//! The WebSocket endpoint of the channel protocol.
//!
//! One binary WebSocket message holds one frame.
//! This version has no agent link, thus it refuses each channel.

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::response::Response;
use farglass_proto::message::{Close, CloseReason, ConnectionMessage};
use farglass_proto::{
    Frame, FrameError, FrameType, HEADER_LEN, MAX_PAYLOAD_LEN, PROTOCOL_VERSION, Side,
};

/// The largest WebSocket message: one frame with the largest payload.
const MAX_MESSAGE_LEN: usize = HEADER_LEN + MAX_PAYLOAD_LEN;

/// Accepts the WebSocket handshake and starts the connection task.
pub async fn upgrade(ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(MAX_MESSAGE_LEN)
        .max_frame_size(MAX_MESSAGE_LEN)
        .on_upgrade(|socket| async move {
            if let Err(error) = serve(socket).await {
                tracing::debug!(%error, "the WebSocket connection failed");
            }
        })
}

/// A cause to close the connection.
#[derive(Debug, thiserror::Error)]
enum Violation {
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("the peer has protocol version {0}")]
    Version(u16),
    #[error("the first frame was not a hello message")]
    NoHello,
    #[error("the message was not binary")]
    NotBinary,
    #[error("the shell cannot open channel {0}")]
    ChannelNumber(farglass_proto::ChannelId),
}

/// Runs one connection until the peer closes it or breaks the protocol.
async fn serve(mut socket: WebSocket) -> Result<(), axum::Error> {
    let hello = ConnectionMessage::Hello {
        version: PROTOCOL_VERSION,
    };
    send(&mut socket, Frame::connection(&hello)).await?;

    let mut greeted = false;
    while let Some(message) = socket.recv().await {
        let result = match message? {
            Message::Binary(bytes) => handle(&mut socket, bytes, &mut greeted).await?,
            Message::Text(_) => Err(Violation::NotBinary),
            Message::Close(_) => return Ok(()),
            // axum answers a ping.
            Message::Ping(_) | Message::Pong(_) => Ok(()),
        };
        if let Err(violation) = result {
            tracing::debug!(%violation, "closing the WebSocket connection");
            let frame = CloseFrame {
                code: close_code::PROTOCOL,
                // A WebSocket close reason has a limit of 123 bytes. The messages are shorter.
                reason: violation.to_string().into(),
            };
            return socket.send(Message::Close(Some(frame))).await;
        }
    }
    Ok(())
}

/// Handles one frame from the shell.
async fn handle(
    socket: &mut WebSocket,
    bytes: bytes::Bytes,
    greeted: &mut bool,
) -> Result<Result<(), Violation>, axum::Error> {
    let frame = match Frame::from_message(bytes) {
        Ok(frame) => frame,
        Err(error) => return Ok(Err(error.into())),
    };

    if frame.channel().is_connection() {
        return Ok(match frame.parse::<ConnectionMessage>() {
            Ok(ConnectionMessage::Hello { version }) if version == PROTOCOL_VERSION => {
                *greeted = true;
                Ok(())
            }
            Ok(ConnectionMessage::Hello { version }) => Err(Violation::Version(version)),
            Err(error) => Err(error.into()),
        });
    }
    if !*greeted {
        return Ok(Err(Violation::NoHello));
    }

    if frame.frame_type() == FrameType::Open {
        if frame.channel().opener() != Some(Side::Shell) {
            return Ok(Err(Violation::ChannelNumber(frame.channel())));
        }
        let close = Close {
            reason: CloseReason::NotSupported,
            message: Some("This server has no channel kinds.".to_owned()),
        };
        send(socket, Frame::close(frame.channel(), &close)).await?;
    }
    // The server refused the channel, thus it drops all other frames of the channel.
    Ok(Ok(()))
}

/// Sends one frame as a binary message.
async fn send(socket: &mut WebSocket, frame: Result<Frame, FrameError>) -> Result<(), axum::Error> {
    let frame = frame.map_err(axum::Error::new)?;
    socket.send(Message::Binary(frame.to_bytes())).await
}
