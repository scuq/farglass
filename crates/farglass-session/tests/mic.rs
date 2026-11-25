//! Tests of the microphone source (F114): the mic channel, and the real PipeWire server of the user.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{Close, CloseReason, LinkMessage, MicEvent, MicRequest, Open};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::link::Session;
use farglass_session::mic::{FRAME, MAX_PACKET, Mic};
use futures_util::{SinkExt as _, StreamExt as _};
use opus1::{Application, Channels, OpusEncoder, SampleRate};
use tokio_util::codec::Framed;

/// Returns `count` data frame payloads with Opus packets of a 440 Hz tone, mono.
fn tone_packets(count: usize) -> Vec<Vec<u8>> {
    let mut opus =
        OpusEncoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip).unwrap();
    (0..count)
        .map(|n| {
            let tone: Vec<f32> = (n * FRAME..(n + 1) * FRAME)
                .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5)
                .collect();
            let mut out = [0_u8; MAX_PACKET];
            let length = opus.encode(&tone, FRAME, &mut out).unwrap();
            let mut payload = u32::try_from(n).unwrap().to_le_bytes().to_vec();
            payload.extend_from_slice(&out[..length]);
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

/// The mic channel: the format first, a start, then the packets fill the source.
/// A packet before the start closes the channel.
#[tokio::test]
async fn the_channel_gets_the_format_and_its_packets_reach_the_source() {
    let mic = Mic::new();
    mic.set_available(true);
    let (ours, theirs) = tokio::net::UnixStream::pair().unwrap();
    let session_mic = mic.clone();
    tokio::spawn(async move {
        let mut session = Session {
            mic: session_mic,
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
        link.send(Frame::open(channel, &Open::Mic).unwrap())
            .await
            .unwrap();
    }
    // Each channel gets its ready frame, then the format. The two channels can mix.
    let mut frames = Vec::new();
    for _ in 0..4 {
        frames.push(link.next().await.unwrap().unwrap());
    }
    for channel in [first, second] {
        let of: Vec<&Frame> = frames.iter().filter(|f| f.channel() == channel).collect();
        assert_eq!(of.len(), 2);
        assert_eq!(of[0].frame_type(), FrameType::Ready);
        assert_eq!(
            of[1].parse::<MicEvent>().unwrap(),
            MicEvent::Ready {
                codec: "opus".into(),
                sample_rate: 48_000,
                channels: 1,
                frame_samples: 960
            }
        );
    }
    let packets = tone_packets(4);
    // The first channel starts, then the second takes the microphone.
    link.send(Frame::json(first, FrameType::Control, &MicRequest::Start).unwrap())
        .await
        .unwrap();
    link.send(Frame::json(second, FrameType::Control, &MicRequest::Start).unwrap())
        .await
        .unwrap();
    assert_eq!(
        next(&mut link, first).await.parse::<MicEvent>().unwrap(),
        MicEvent::Taken
    );
    for packet in &packets {
        link.send(Frame::data(second, packet.clone()).unwrap())
            .await
            .unwrap();
    }
    let end = Instant::now() + Duration::from_secs(10);
    let mut out = vec![0.0; FRAME * 2];
    loop {
        mic.pull(&mut out);
        if out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32 > 0.05 {
            break;
        }
        assert!(Instant::now() < end, "the tone did not reach the source");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    link.send(Frame::json(second, FrameType::Control, &MicRequest::Stop).unwrap())
        .await
        .unwrap();
    link.send(Frame::data(second, packets[0].clone()).unwrap())
        .await
        .unwrap();
    let close = next(&mut link, second).await;
    assert_eq!(close.frame_type(), FrameType::Close);
    assert_eq!(
        close.parse::<Close>().unwrap().reason,
        CloseReason::ProtocolError
    );
    assert!(!mic.in_use());
}

/// `pw-record` records the tone that the packets give, from the source of the agent.
#[test]
fn an_app_records_the_packets_from_the_source() {
    if Command::new("pw-record").arg("--version").output().is_err() {
        eprintln!("SKIP: no pw-record");
        return;
    }
    let mic = Mic::new();
    let source = match mic.start_source() {
        Ok(source) => source,
        Err(error) => {
            eprintln!("SKIP: no PipeWire: {error}");
            return;
        }
    };
    let channel = mic.add_channel(|| {});
    channel.start();
    let mut recorder = Command::new("pw-record")
        .args([
            "--raw",
            "--target",
            "farglass-mic",
            "--format",
            "f32",
            "--rate",
            "48000",
            "--channels",
            "1",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = recorder.stdout.take().unwrap();
    let samples = Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
    let store = samples.clone();
    let reader = std::thread::spawn(move || {
        let mut bytes = [0_u8; 4096];
        while let Ok(n) = stdout.read(&mut bytes) {
            if n == 0 {
                break;
            }
            let mut all = store.lock().unwrap();
            all.extend(
                bytes[..n - n % 4]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| f32::from_le_bytes(*b)),
            );
        }
    });
    // 2 s of the tone at the rate of the format.
    let start = Instant::now();
    for (n, packet) in tone_packets(100).iter().enumerate() {
        channel.packet(packet).unwrap();
        let due = start + Duration::from_millis(20 * (n as u64 + 1));
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
    }
    let _ = recorder.kill();
    let _ = recorder.wait();
    let _ = reader.join();
    drop(source);
    let samples = samples.lock().unwrap().clone();
    if samples.is_empty() {
        eprintln!("SKIP: PipeWire gave the recorder no sound");
        return;
    }
    let loud = samples
        .chunks(FRAME)
        .filter(|block| block.iter().map(|s| s * s).sum::<f32>() / block.len() as f32 > 0.05)
        .count();
    assert!(
        loud >= 50,
        "{loud} of {} blocks of 20 ms have the tone",
        samples.len() / FRAME
    );
}
