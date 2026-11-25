//! Tests of the camera node (F116): the camera channel, and the real PipeWire server of the user.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{CameraEvent, CameraRequest, Close, CloseReason, LinkMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::camera::{Camera, HEIGHT, NODE_NAME, WIDTH};
use farglass_session::link::Session;
use futures_util::{SinkExt as _, StreamExt as _};
use openh264::encoder::Encoder;
use openh264::formats::YUVBuffer;
use tokio_util::codec::Framed;

/// Returns `count` data frame payloads: a gray picture with a bright square, the first one a key frame.
fn pictures(count: usize) -> Vec<Vec<u8>> {
    let mut encoder = Encoder::new().unwrap();
    (0..count)
        .map(|n| {
            let mut yuv = vec![128_u8; WIDTH * HEIGHT * 3 / 2];
            yuv[..WIDTH * HEIGHT].fill(90);
            for row in 100..300 {
                let at = row * WIDTH + 200;
                yuv[at..at + 200].fill(220);
            }
            let stream = encoder
                .encode(&YUVBuffer::from_vec(yuv, WIDTH, HEIGHT))
                .unwrap();
            let mut payload = vec![u8::from(n == 0)];
            payload.extend_from_slice(&stream.to_vec());
            payload
        })
        .collect()
}

type Link = Framed<tokio::net::UnixStream, FrameCodec>;

/// Returns the next frame of `channel` on the link.
async fn next(link: &mut Link, channel: ChannelId) -> Frame {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let frame = link.next().await.unwrap().unwrap();
            if frame.channel() == channel {
                return frame;
            }
        }
    })
    .await
    .unwrap()
}

/// The camera channel: the format first, an arm, the reading event, then the pictures reach the node.
/// A different channel takes the camera, and a bad format closes the channel.
#[tokio::test]
async fn the_channel_gets_the_format_and_its_pictures_reach_the_node() {
    let camera = Camera::new();
    camera.set_available(true);
    let (ours, theirs) = tokio::net::UnixStream::pair().unwrap();
    let session_camera = camera.clone();
    tokio::spawn(async move {
        let mut session = Session {
            camera: session_camera,
            ..Session::default()
        };
        let _ = farglass_session::link::serve_with(theirs, &mut session).await;
    });
    let mut link = Framed::new(ours, FrameCodec);
    link.next().await.unwrap().unwrap();
    let hello = LinkMessage::Hello {
        version: PROTOCOL_VERSION,
    };
    link.send(Frame::json(ChannelId::CONNECTION, FrameType::Control, &hello).unwrap())
        .await
        .unwrap();
    let first = ChannelId::new(1);
    let second = ChannelId::new(3);
    for channel in [first, second] {
        link.send(Frame::open(channel, &Open::Camera).unwrap())
            .await
            .unwrap();
    }
    let mut frames = Vec::new();
    for _ in 0..4 {
        frames.push(link.next().await.unwrap().unwrap());
    }
    for channel in [first, second] {
        let of: Vec<&Frame> = frames.iter().filter(|f| f.channel() == channel).collect();
        assert_eq!(of.len(), 2);
        assert_eq!(of[0].frame_type(), FrameType::Ready);
        let event = of[1].parse::<CameraEvent>().unwrap();
        if matches!(event, CameraEvent::Blocked { .. }) {
            eprintln!("SKIP: the privacy settings of this host block the camera");
            return;
        }
        assert_eq!(
            event,
            CameraEvent::Ready {
                codec: "h264".into(),
                width: 1280,
                height: 720,
                frame_rate: 30
            }
        );
    }
    let control = |channel, request: &CameraRequest| {
        Frame::json(channel, FrameType::Control, request).unwrap()
    };
    link.send(control(first, &CameraRequest::Arm))
        .await
        .unwrap();
    link.send(control(second, &CameraRequest::Arm))
        .await
        .unwrap();
    assert_eq!(
        next(&mut link, first).await.parse::<CameraEvent>().unwrap(),
        CameraEvent::Taken
    );
    // A host app opens the node.
    camera.set_reading(true);
    assert_eq!(
        next(&mut link, second)
            .await
            .parse::<CameraEvent>()
            .unwrap(),
        CameraEvent::Reading
    );
    let format = CameraRequest::Format {
        codec: "h264".into(),
        width: 1280,
        height: 720,
    };
    link.send(control(second, &format)).await.unwrap();
    for picture in pictures(3) {
        link.send(Frame::data(second, picture).unwrap())
            .await
            .unwrap();
    }
    let end = Instant::now() + Duration::from_secs(10);
    while camera.picture()[200 * WIDTH + 300] < 200 {
        assert!(Instant::now() < end, "the picture did not reach the node");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    camera.set_reading(false);
    assert_eq!(
        next(&mut link, second)
            .await
            .parse::<CameraEvent>()
            .unwrap(),
        CameraEvent::Idle
    );
    let small = CameraRequest::Format {
        codec: "h264".into(),
        width: 640,
        height: 480,
    };
    link.send(control(second, &small)).await.unwrap();
    let close = next(&mut link, second).await;
    assert_eq!(close.frame_type(), FrameType::Close);
    assert_eq!(
        close.parse::<Close>().unwrap().reason,
        CloseReason::ProtocolError
    );
    assert!(!camera.in_use());
}

/// A PipeWire stream reads the node, as a host app does: the agent sees the reader, and the reader gets the pictures.
#[test]
fn an_app_reads_the_pictures_from_the_node() {
    let camera = Camera::new();
    let node = match camera.start_node() {
        Ok(node) => node,
        Err(error) => {
            eprintln!("SKIP: no PipeWire: {error}");
            return;
        }
    };
    let channel = camera.add_channel(|_| {});
    channel.arm();
    let frames = Arc::new(AtomicUsize::new(0));
    let bright = Arc::new(AtomicU8::new(0));
    let reader = {
        let frames = frames.clone();
        let bright = bright.clone();
        std::thread::spawn(move || read_node(Duration::from_secs(4), &frames, &bright))
    };
    let end = Instant::now() + Duration::from_secs(5);
    while !camera.reading() {
        if Instant::now() > end {
            drop(node);
            let _ = reader.join();
            eprintln!("SKIP: PipeWire did not link a reader to the node");
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    channel.format("h264", 1280, 720).unwrap();
    let start = Instant::now();
    for (n, picture) in pictures(60).iter().enumerate() {
        channel.frame(picture).unwrap();
        let due = start + Duration::from_millis(33 * (n as u64 + 1));
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
    }
    reader.join().unwrap();
    drop(node);
    let frames = frames.load(Ordering::Relaxed);
    assert!(frames >= 60, "the reader got {frames} pictures in 4 s");
    assert!(
        bright.load(Ordering::Relaxed) > 200,
        "the reader got the bright square of the stream"
    );
}

/// Reads the node for `time`: counts the pictures, and keeps the luma of a pixel in the bright square.
fn read_node(time: Duration, frames: &Arc<AtomicUsize>, bright: &Arc<AtomicU8>) {
    use pipewire as pw;
    use pw::properties::properties;
    use pw::spa;
    use spa::pod::{Object, Pod, Property, PropertyFlags, Value};

    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).unwrap();
    let context = pw::context::ContextRc::new(&mainloop, None).unwrap();
    let core = context.connect_rc(None).unwrap();
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Video",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Camera",
        *pw::keys::TARGET_OBJECT => NODE_NAME,
    };
    let stream = pw::stream::StreamRc::new(core, "farglass-camera-test", props).unwrap();
    let count = frames.clone();
    let luma = bright.clone();
    let _listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, ()| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            if let Some(bytes) = buffer.datas_mut().first_mut().and_then(|data| data.data())
                && let Some(value) = bytes.get(200 * WIDTH + 300)
            {
                luma.store(*value, Ordering::Relaxed);
            }
            count.fetch_add(1, Ordering::Relaxed);
        })
        .register()
        .unwrap();
    let property = |key, value| Property {
        key,
        flags: PropertyFlags::empty(),
        value,
    };
    let format = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(Object {
            type_: spa::sys::SPA_TYPE_OBJECT_Format,
            id: spa::sys::SPA_PARAM_EnumFormat,
            properties: vec![
                property(
                    spa::sys::SPA_FORMAT_mediaType,
                    Value::Id(spa::utils::Id(spa::sys::SPA_MEDIA_TYPE_video)),
                ),
                property(
                    spa::sys::SPA_FORMAT_mediaSubtype,
                    Value::Id(spa::utils::Id(spa::sys::SPA_MEDIA_SUBTYPE_raw)),
                ),
                property(
                    spa::sys::SPA_FORMAT_VIDEO_format,
                    Value::Id(spa::utils::Id(spa::sys::SPA_VIDEO_FORMAT_I420)),
                ),
            ],
        }),
    )
    .unwrap()
    .0
    .into_inner();
    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut [Pod::from_bytes(&format).unwrap()],
        )
        .unwrap();
    let stop = mainloop.clone();
    let done = Rc::new(Cell::new(false));
    let flag = done.clone();
    let timer = mainloop.loop_().add_timer(move |_| {
        flag.set(true);
        stop.quit();
    });
    let _ = timer.update_timer(Some(time), None);
    mainloop.run();
    assert!(done.get());
}
