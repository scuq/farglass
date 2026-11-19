//! Property tests of the frame codec.

use bytes::{BufMut, Bytes, BytesMut};
use farglass_proto::{ChannelId, Frame, FrameType, HEADER_LEN};
use proptest::prelude::*;

fn frame() -> impl Strategy<Value = Frame> {
    (
        any::<u32>(),
        proptest::sample::select(FrameType::ALL.to_vec()),
        proptest::collection::vec(any::<u8>(), 0..2048),
    )
        .prop_map(|(channel, frame_type, payload)| {
            let channel = ChannelId::new(channel);
            let frame_type = if channel.is_connection() {
                FrameType::Control
            } else {
                frame_type
            };
            Frame::new(channel, frame_type, payload).unwrap()
        })
}

proptest! {
    #[test]
    fn message_decode_inverts_encode(frame in frame()) {
        let bytes = frame.to_bytes();
        prop_assert_eq!(bytes.len(), HEADER_LEN + frame.payload().len());
        prop_assert_eq!(Frame::from_message(bytes).unwrap(), frame);
    }

    #[test]
    fn stream_decode_gives_the_same_frames_for_each_split(
        frames in proptest::collection::vec(frame(), 1..8),
        chunk_len in 1usize..300,
    ) {
        let mut wire = BytesMut::new();
        for frame in &frames {
            frame.encode(&mut wire);
        }

        let mut src = BytesMut::new();
        let mut decoded = Vec::new();
        for chunk in wire.chunks(chunk_len) {
            src.put_slice(chunk);
            while let Some(frame) = Frame::decode(&mut src).unwrap() {
                decoded.push(frame);
            }
        }
        prop_assert!(src.is_empty());
        prop_assert_eq!(decoded, frames);
    }

    #[test]
    fn decode_of_random_bytes_does_not_panic(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
        let _ = Frame::from_message(Bytes::from(bytes.clone()));
        let mut src = BytesMut::from(&bytes[..]);
        while let Ok(Some(_)) = Frame::decode(&mut src) {}
    }
}
