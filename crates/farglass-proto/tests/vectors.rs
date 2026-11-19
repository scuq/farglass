//! Runs the test vectors that the Rust side and the TypeScript side share.

use bytes::{Bytes, BytesMut};
use farglass_proto::message::{Close, ConnectionMessage, Open, Ready};
use farglass_proto::{ChannelId, Frame, FrameType};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("vectors.json")).unwrap()
}

fn hex(value: &Value) -> Bytes {
    let text = value.as_str().unwrap();
    assert!(text.len().is_multiple_of(2), "odd hex length");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// Parses the payload as the message type of the frame and serializes it again.
fn typed_cycle(frame: &Frame) -> Value {
    let value = if frame.channel().is_connection() {
        serde_json::to_value(frame.parse::<ConnectionMessage>().unwrap())
    } else {
        match frame.frame_type() {
            FrameType::Open => serde_json::to_value(frame.parse::<Open>().unwrap()),
            FrameType::Ready => serde_json::to_value(frame.parse::<Ready>().unwrap()),
            FrameType::Close => serde_json::to_value(frame.parse::<Close>().unwrap()),
            FrameType::Control => serde_json::to_value(frame.parse::<Value>().unwrap()),
            FrameType::Data => unreachable!("a data frame has no JSON payload"),
        }
    };
    value.unwrap()
}

#[test]
fn valid_vectors_decode_and_encode() {
    for vector in vectors()["valid"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let bytes = hex(&vector["frame"]);
        let channel = ChannelId::new(vector["channel"].as_u64().unwrap().try_into().unwrap());
        let frame_type = FrameType::try_from(vector["type"].as_u64().unwrap() as u8).unwrap();
        let payload = hex(&vector["payload"]);

        let frame = Frame::from_message(bytes.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(frame.channel(), channel, "{name}");
        assert_eq!(frame.frame_type(), frame_type, "{name}");
        assert_eq!(frame.payload(), &payload, "{name}");

        let mut stream = BytesMut::from(&bytes[..]);
        assert_eq!(
            Frame::decode(&mut stream).unwrap(),
            Some(frame.clone()),
            "{name}"
        );
        assert!(stream.is_empty(), "{name}");

        let encoded = Frame::new(channel, frame_type, payload).unwrap().to_bytes();
        assert_eq!(encoded, bytes, "{name}");

        if let Some(json) = vector.get("json") {
            assert_eq!(&frame.parse::<Value>().unwrap(), json, "{name}");
            assert_eq!(&typed_cycle(&frame), json, "{name}");
        }
    }
}

#[test]
fn invalid_vectors_give_the_error_code() {
    for vector in vectors()["invalid"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let error = Frame::from_message(hex(&vector["frame"])).expect_err(name);
        assert_eq!(error.code(), vector["error"].as_str().unwrap(), "{name}");
    }
}
