//! The sound of the apps of the session (F038).
//!
//! The agent adds an audio sink with the name [`SINK_NAME`] to the PipeWire
//! server of the user. The apps that start in the web session play to it:
//! the desktop variables name it in `PULSE_SINK` and `PIPEWIRE_NODE`. The
//! agent encodes the sound of the sink as Opus, 20 ms in each packet, and
//! gives the packets to each audio channel. With no channel, or after 1 s of
//! silence, no packet goes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use opus1::{Application, Bitrate, Channels, OpusEncoder, SampleRate};

/// The node name of the sink.
pub const SINK_NAME: &str = "farglass";

/// The sample rate of the sink and of the packets.
pub const SAMPLE_RATE: u32 = 48_000;

/// The channels: left and right.
pub const CHANNELS: usize = 2;

/// The samples of each channel in one packet: 20 ms.
pub const FRAME: usize = 960;

/// The bit rate of the packets: good music quality for stereo.
pub const BITRATE: u32 = 128_000;

/// The silent packets after which no packet goes until the sound comes back: 1 s.
const SILENT_PACKETS: u32 = 50;

/// One Opus packet: 20 ms of sound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioPacket {
    /// The number of the packet. A gap shows packets that did not go, for example in a silence.
    pub seq: u32,
    /// The Opus packet.
    pub data: Vec<u8>,
}

impl AudioPacket {
    /// Returns the payload of a data frame: the sequence as u32, little-endian, then the Opus packet.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(4 + self.data.len());
        bytes.extend_from_slice(&self.seq.to_le_bytes());
        bytes.extend_from_slice(&self.data);
        bytes
    }
}

type PacketSink = Arc<dyn Fn(&AudioPacket) + Send + Sync>;

/// The state of the encoder.
struct Encoding {
    opus: Option<OpusEncoder>,
    /// Interleaved samples that wait for a full packet.
    pending: Vec<f32>,
    seq: u32,
    silent: u32,
}

/// The audio channels and the encoder of the sound of the sink.
pub struct Audio {
    viewers: Mutex<HashMap<u64, PacketSink>>,
    encoding: Mutex<Encoding>,
    next: AtomicU64,
    running: AtomicBool,
}

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("running", &self.running.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// One audio channel. Dropping it removes the channel.
pub struct AudioGuard {
    audio: Arc<Audio>,
    id: u64,
}

impl std::fmt::Debug for AudioGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioGuard").field("id", &self.id).finish()
    }
}

impl Drop for AudioGuard {
    fn drop(&mut self) {
        self.audio
            .viewers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

/// Reports whether each sample of a packet is silent: below -100 dB.
fn silent(samples: &[f32]) -> bool {
    samples.iter().all(|sample| sample.abs() < 1e-5)
}

impl Audio {
    /// Returns the audio of a session with no sink yet.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            viewers: Mutex::new(HashMap::new()),
            encoding: Mutex::new(Encoding {
                opus: None,
                pending: Vec::new(),
                seq: 0,
                silent: 0,
            }),
            next: AtomicU64::new(1),
            running: AtomicBool::new(false),
        })
    }

    /// Reports whether the sink runs.
    #[must_use]
    pub fn available(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Marks the sink as running, for example in a test that gives its own samples.
    pub fn set_available(&self, running: bool) {
        self.running.store(running, Ordering::Release);
    }

    /// Adds an audio channel. `send` gets each packet.
    pub fn add_viewer(
        self: &Arc<Self>,
        send: impl Fn(&AudioPacket) + Send + Sync + 'static,
    ) -> AudioGuard {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.viewers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, Arc::new(send));
        AudioGuard {
            audio: self.clone(),
            id,
        }
    }

    /// Takes interleaved stereo samples of the sink. Encodes each full packet if a channel listens.
    pub fn push(&self, samples: &[f32]) {
        let viewers: Vec<PacketSink> = self
            .viewers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        let mut encoding = self.encoding.lock().unwrap_or_else(PoisonError::into_inner);
        if viewers.is_empty() {
            // No one listens: no work, and the next listener starts with new sound.
            encoding.pending.clear();
            encoding.silent = SILENT_PACKETS;
            return;
        }
        encoding.pending.extend_from_slice(samples);
        let size = FRAME * CHANNELS;
        while encoding.pending.len() >= size {
            let frame: Vec<f32> = encoding.pending.drain(..size).collect();
            let seq = encoding.seq;
            encoding.seq = seq.wrapping_add(1);
            if silent(&frame) {
                encoding.silent = encoding.silent.saturating_add(1);
                if encoding.silent > SILENT_PACKETS {
                    continue;
                }
            } else {
                encoding.silent = 0;
            }
            let Some(opus) = encoding.opus_encoder() else {
                return;
            };
            let mut out = [0_u8; 1500];
            match opus.encode(&frame, FRAME, &mut out) {
                Ok(length) => {
                    let packet = AudioPacket {
                        seq,
                        data: out[..length].to_vec(),
                    };
                    for send in &viewers {
                        send(&packet);
                    }
                }
                Err(error) => tracing::warn!(%error, "a sound packet did not encode"),
            }
        }
    }

    /// Starts the sink in the PipeWire server of the user, on a thread of its own.
    /// The sink stays until the returned value drops.
    ///
    /// # Errors
    ///
    /// A message if PipeWire does not answer or refuses the sink.
    pub fn start_sink(self: &Arc<Self>) -> Result<SinkHandle, String> {
        let (quit, quit_rx) = pipewire::channel::channel::<()>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let audio = self.clone();
        let thread = std::thread::Builder::new()
            .name("farglass-audio".to_owned())
            .spawn(move || {
                if let Err(error) = sink::run(&audio, quit_rx, &ready_tx) {
                    let _ = ready_tx.send(Err(error));
                }
                audio.set_available(false);
            })
            .map_err(|e| e.to_string())?;
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "PipeWire did not answer".to_owned())??;
        self.set_available(true);
        Ok(SinkHandle {
            quit,
            thread: Some(thread),
        })
    }
}

impl Encoding {
    fn opus_encoder(&mut self) -> Option<&mut OpusEncoder> {
        if self.opus.is_none() {
            match OpusEncoder::new(SampleRate::Hz48000, Channels::Stereo, Application::Audio) {
                Ok(mut opus) => {
                    if let Some(bits) = std::num::NonZeroU32::new(BITRATE) {
                        opus.set_bitrate(Bitrate::BitsPerSecond(bits));
                    }
                    self.opus = Some(opus);
                }
                Err(error) => {
                    tracing::warn!(%error, "no Opus encoder");
                    return None;
                }
            }
        }
        self.opus.as_mut()
    }
}

/// The running sink. Dropping it removes the sink from PipeWire.
pub struct SinkHandle {
    quit: pipewire::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for SinkHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SinkHandle").finish_non_exhaustive()
    }
}

impl Drop for SinkHandle {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Returns the PipeWire format of a stream: 32-bit float samples at `rate` Hz, with one
/// channel for each entry of `positions`, for example `SPA_AUDIO_CHANNEL_MONO`.
pub(crate) fn format_pod(rate: u32, positions: &[u32]) -> Result<Vec<u8>, String> {
    use pipewire::spa;
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(rate);
    info.set_channels(u32::try_from(positions.len()).unwrap_or(1));
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    for (slot, value) in position.iter_mut().zip(positions) {
        *slot = *value;
    }
    info.set_position(position);
    let object = spa::pod::Object {
        type_: spa::sys::SPA_TYPE_OBJECT_Format,
        id: spa::sys::SPA_PARAM_EnumFormat,
        properties: info.into(),
    };
    Ok(spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map_err(|e| format!("{e:?}"))?
    .0
    .into_inner())
}

/// The PipeWire side: a stream that is a sink node.
mod sink {
    use std::sync::Arc;

    use pipewire as pw;
    use pw::properties::properties;
    use pw::spa;
    use spa::pod::Pod;

    use super::{Audio, SAMPLE_RATE, SINK_NAME};

    /// Runs the sink until a message comes on `quit`. Tells `ready` when the sink is there.
    pub(super) fn run(
        audio: &Arc<Audio>,
        quit: pw::channel::Receiver<()>,
        ready: &std::sync::mpsc::Sender<Result<(), String>>,
    ) -> Result<(), String> {
        let error = |e: pw::Error| e.to_string();
        pw::init();
        let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(error)?;
        let context = pw::context::ContextRc::new(&mainloop, None).map_err(error)?;
        let core = context.connect_rc(None).map_err(error)?;
        let props = properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CLASS => "Audio/Sink",
            *pw::keys::NODE_NAME => SINK_NAME,
            *pw::keys::NODE_DESCRIPTION => "farglass browser",
            *pw::keys::NODE_LATENCY => "960/48000",
            "audio.position" => "FL,FR",
        };
        let stream = pw::stream::StreamBox::new(&core, "farglass-sink", props).map_err(error)?;
        let samples_of = audio.clone();
        let _listener = stream
            .add_local_listener_with_user_data(Vec::<f32>::new())
            .process(move |stream, samples| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let datas = buffer.datas_mut();
                let Some(data) = datas.first_mut() else {
                    return;
                };
                let (offset, size) = (data.chunk().offset() as usize, data.chunk().size() as usize);
                let Some(bytes) = data.data() else {
                    return;
                };
                let Some(bytes) = bytes.get(offset..offset + size) else {
                    return;
                };
                samples.clear();
                samples.extend(
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| f32::from_le_bytes(*b)),
                );
                samples_of.push(samples);
            })
            .register()
            .map_err(error)?;

        let values = super::format_pod(
            SAMPLE_RATE,
            &[
                spa::sys::SPA_AUDIO_CHANNEL_FL,
                spa::sys::SPA_AUDIO_CHANNEL_FR,
            ],
        )?;
        let mut params = [Pod::from_bytes(&values).ok_or("the audio format is not valid")?];
        stream
            .connect(
                spa::utils::Direction::Input,
                None,
                pw::stream::StreamFlags::MAP_BUFFERS,
                &mut params,
            )
            .map_err(error)?;
        let stop = mainloop.clone();
        let _quit = quit.attach(mainloop.loop_(), move |()| stop.quit());
        let _ = ready.send(Ok(()));
        tracing::info!(
            sink = SINK_NAME,
            "the sound sink of the session is in PipeWire"
        );
        mainloop.run();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use opus1::OpusDecoder;

    use super::*;

    fn sine(frames: usize, start: usize) -> Vec<f32> {
        (start..start + frames)
            .flat_map(|i| {
                let v = (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn sound_goes_in_packets_of_20_ms_and_decodes_again() {
        let audio = Audio::new();
        let packets = Arc::new(Mutex::new(Vec::new()));
        let sink = packets.clone();
        let _guard = audio.add_viewer(move |packet| sink.lock().unwrap().push(packet.clone()));
        // Odd buffer sizes, as PipeWire gives them.
        let mut at = 0;
        for size in [256, 1024, 700, 2000, 820] {
            audio.push(&sine(size, at));
            at += size;
        }
        let packets = packets.lock().unwrap().clone();
        assert_eq!(packets.len(), at / FRAME);
        assert_eq!(
            packets.iter().map(|p| p.seq).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        let mut decoder = OpusDecoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
        let mut pcm = vec![0.0; FRAME * CHANNELS];
        let mut energy = 0.0;
        for packet in &packets {
            assert_eq!(
                decoder.decode(&packet.data, FRAME, &mut pcm).unwrap(),
                FRAME
            );
            energy += pcm.iter().map(|s| s * s).sum::<f32>();
        }
        assert!(
            energy / (packets.len() * FRAME * CHANNELS) as f32 > 0.05,
            "the tone comes back"
        );
        assert_eq!(packets[1].to_bytes()[..4], 1_u32.to_le_bytes());
    }

    #[test]
    fn a_long_silence_and_no_listener_send_no_packets() {
        let audio = Audio::new();
        audio.push(&sine(FRAME * 3, 0));
        let packets = Arc::new(Mutex::new(Vec::new()));
        let sink = packets.clone();
        let guard = audio.add_viewer(move |packet| sink.lock().unwrap().push(packet.seq));
        // A tone, then 2 s of silence, then the tone again.
        audio.push(&sine(FRAME * 2, 0));
        audio.push(&vec![0.0; FRAME * CHANNELS * 100]);
        audio.push(&sine(FRAME, 0));
        let seqs = packets.lock().unwrap().clone();
        assert_eq!(
            seqs.len(),
            2 + SILENT_PACKETS as usize + 1,
            "the silence stops after 1 s"
        );
        assert_eq!(
            *seqs.last().unwrap(),
            102,
            "the gap shows the packets that did not go"
        );
        drop(guard);
        audio.push(&sine(FRAME * 4, 0));
        assert_eq!(packets.lock().unwrap().len(), seqs.len());
    }
}
