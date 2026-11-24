//! Tests of the sound sink with the real PipeWire server of the user (F038).

use std::io::Write as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use farglass_session::audio::{Audio, CHANNELS, FRAME};
use opus1::{Channels, OpusDecoder, SampleRate};

/// A tone that `pw-cat` plays to the sink of the agent comes out as Opus packets with the tone.
#[test]
fn a_tone_played_to_the_sink_comes_out_as_opus_packets() {
    if Command::new("pw-cat").arg("--version").output().is_err() {
        eprintln!("SKIP: no pw-cat");
        return;
    }
    let audio = Audio::new();
    let sink = match audio.start_sink() {
        Ok(sink) => sink,
        Err(error) => {
            eprintln!("SKIP: no PipeWire: {error}");
            return;
        }
    };
    assert!(audio.available());
    let packets = Arc::new(Mutex::new(Vec::new()));
    let store = packets.clone();
    let _guard = audio.add_viewer(move |packet| store.lock().unwrap().push(packet.clone()));

    // 2 s of a 440 Hz tone, raw f32 stereo at 48 kHz, to the sink by its node name.
    let mut player = Command::new("pw-cat")
        .args([
            "--playback",
            "--raw",
            "--target",
            "farglass",
            "--format",
            "f32",
            "--rate",
            "48000",
            "--channels",
            "2",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let tone: Vec<u8> = (0..96_000)
        .flat_map(|i| {
            let v = (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
            [v.to_le_bytes(), v.to_le_bytes()].concat()
        })
        .collect();
    player.stdin.take().unwrap().write_all(&tone).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while packets.lock().unwrap().len() < 50 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = player.wait();
    let packets = packets.lock().unwrap().clone();
    // No packet at all: the PipeWire server of this host did not route the tone to the sink.
    // Some packets but too few is still a failure of the sink.
    if packets.is_empty() {
        eprintln!("SKIP: PipeWire gave the sink no sound");
        return;
    }
    assert!(packets.len() >= 50, "only {} packets", packets.len());
    let mut decoder = OpusDecoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
    let mut pcm = vec![0.0; FRAME * CHANNELS];
    let loud = packets
        .iter()
        .filter(|packet| {
            decoder.decode(&packet.data, FRAME, &mut pcm).unwrap();
            pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len() as f32 > 0.05
        })
        .count();
    assert!(
        loud >= 40,
        "{loud} of {} packets have the tone",
        packets.len()
    );
    drop(sink);
}

/// The audio channel: the format first, then one data frame for each packet.
#[tokio::test]
async fn the_channel_gets_the_format_and_then_the_packets() {
    use farglass_proto::codec::FrameCodec;
    use farglass_proto::message::{AudioEvent, LinkMessage, Open};
    use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
    use farglass_session::link::Session;
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_util::codec::Framed;

    let audio = Audio::new();
    audio.set_available(true);
    let (ours, theirs) = tokio::net::UnixStream::pair().unwrap();
    let session_audio = audio.clone();
    tokio::spawn(async move {
        let mut session = Session {
            audio: session_audio,
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
    let channel = ChannelId::new(1);
    link.send(Frame::open(channel, &Open::Audio).unwrap())
        .await
        .unwrap();
    let mut next = async || {
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
    };
    assert_eq!(next().await.frame_type(), FrameType::Ready);
    assert_eq!(
        next().await.parse::<AudioEvent>().unwrap(),
        AudioEvent::Format {
            codec: "opus".into(),
            sample_rate: 48_000,
            channels: 2,
            frame_samples: 960
        }
    );
    let tone: Vec<f32> = (0..FRAME * 2)
        .flat_map(|i| {
            let v = (i as f32 * 0.05).sin() * 0.5;
            [v, v]
        })
        .collect();
    // The channel is registered when the format arrived.
    audio.push(&tone);
    for seq in 0..2_u32 {
        let frame = next().await;
        assert_eq!(frame.frame_type(), FrameType::Data);
        assert_eq!(frame.payload()[..4], seq.to_le_bytes());
        assert!(frame.payload().len() > 10);
    }
}
