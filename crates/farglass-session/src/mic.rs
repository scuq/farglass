//! The microphone of the client for the apps of the session (F114).
//!
//! The agent adds an audio source with the name [`SOURCE_NAME`] to the
//! PipeWire server of the user. The apps that start in the web session record
//! from it: the desktop variables name it in `PULSE_SOURCE`. The shell sends
//! Opus packets of 20 ms, mono, on a mic channel. The agent decodes them into
//! a small jitter buffer, and the source plays the buffer. With no packets,
//! the source gives silence.
//!
//! One channel holds the microphone at a time. The channel that starts last
//! gets it, and the agent tells the channel before it with
//! [`MicEvent::Taken`](farglass_proto::message::MicEvent::Taken).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use farglass_proto::message::CloseReason;
use opus1::{Channels, OpusDecoder, SampleRate};

/// The node name of the source.
pub const SOURCE_NAME: &str = "farglass-mic";

/// The sample rate of the packets and of the source.
pub const SAMPLE_RATE: u32 = 48_000;

/// The channels of the packets and of the source: mono.
pub const CHANNELS: u8 = 1;

/// The samples in one packet: 20 ms.
pub const FRAME: usize = 960;

/// The largest Opus packet of one frame, as RFC 6716 gives it.
pub const MAX_PACKET: usize = 1275;

/// The sound in the buffer before the source plays it: 40 ms.
const START: usize = FRAME * 2;

/// The largest delay of the buffer: 120 ms. Above it, the oldest sound goes.
const MAX_DELAY: usize = FRAME * 6;

/// The packets that can come at one time after a network delay: 3 s.
const BURST: f64 = 150.0;

/// The packets each second of the format.
const RATE: f64 = SAMPLE_RATE as f64 / FRAME as f64;

/// The largest gap of packet numbers that loss concealment fills. A larger gap starts the sound again.
const MAX_CONCEALED: u32 = 5;

/// A data frame that the agent refuses. The channel closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PacketError {
    /// A data frame before the start, or after the stop of the channel.
    #[error("The microphone channel sent sound before a start message.")]
    NotStarted,
    /// A data frame with no packet number, or with a packet that is too large.
    #[error("The microphone packet has a size that is not valid.")]
    Size,
    /// More packets than the format gives.
    #[error("The microphone channel sent more packets than its format gives.")]
    TooFast,
}

impl PacketError {
    /// Returns the reason of the close frame.
    #[must_use]
    pub const fn reason(self) -> CloseReason {
        match self {
            Self::NotStarted | Self::Size => CloseReason::ProtocolError,
            Self::TooFast => CloseReason::ResourceLimit,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::NotStarted => "not-started",
            Self::Size => "size",
            Self::TooFast => "too-fast",
        }
    }
}

type Taken = Arc<dyn Fn() + Send + Sync>;

/// The channel that holds the microphone.
struct Holder {
    id: u64,
    taken: Taken,
}

/// The state of the decoder and of the buffer.
struct State {
    holder: Option<Holder>,
    opus: Option<OpusDecoder>,
    /// The packet number that comes next.
    expected: Option<u32>,
    buffer: VecDeque<f32>,
    /// Reports whether the source plays the buffer. It plays again after [`START`] samples.
    playing: bool,
    /// The packets that can come now, for the rate limit.
    tokens: f64,
    last: Instant,
}

/// The microphone channels and the source of the session.
pub struct Mic {
    state: Mutex<State>,
    next: AtomicU64,
    running: AtomicBool,
}

impl std::fmt::Debug for Mic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mic")
            .field("running", &self.running.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// One microphone channel. Dropping it stops the microphone if the channel holds it.
pub struct MicGuard {
    mic: Arc<Mic>,
    id: u64,
    /// Reports whether the shell sent a start and no stop.
    started: AtomicBool,
    /// Runs when a different channel starts the microphone.
    taken: Taken,
}

impl std::fmt::Debug for MicGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicGuard").field("id", &self.id).finish()
    }
}

impl Drop for MicGuard {
    fn drop(&mut self) {
        self.mic.release(self.id);
    }
}

/// Logs a microphone event in the form of the security log (ADR-043).
fn security(result: &str, reason: &str) {
    let uid = nix::unistd::getuid();
    if result == "accepted" {
        tracing::info!(
            target: "farglass::security",
            "event=microphone result={result} reason={reason} addr=- user=- uid={uid}"
        );
    } else {
        tracing::warn!(
            target: "farglass::security",
            "event=microphone result={result} reason={reason} addr=- user=- uid={uid}"
        );
    }
}

impl Mic {
    /// Returns the microphone of a session with no source yet.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                holder: None,
                opus: None,
                expected: None,
                buffer: VecDeque::new(),
                playing: false,
                tokens: BURST,
                last: Instant::now(),
            }),
            next: AtomicU64::new(1),
            running: AtomicBool::new(false),
        })
    }

    /// Reports whether the source runs.
    #[must_use]
    pub fn available(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Marks the source as running, for example in a test that reads the samples itself.
    pub fn set_available(&self, running: bool) {
        self.running.store(running, Ordering::Release);
    }

    /// Adds a microphone channel. `taken` runs when a different channel starts the microphone.
    pub fn add_channel(self: &Arc<Self>, taken: impl Fn() + Send + Sync + 'static) -> MicGuard {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        MicGuard {
            mic: self.clone(),
            id,
            started: AtomicBool::new(false),
            taken: Arc::new(taken),
        }
    }

    /// Reports whether a channel holds the microphone.
    #[must_use]
    pub fn in_use(&self) -> bool {
        self.lock().holder.is_some()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Fills `out` with mono samples for the source: the buffer, or silence.
    pub fn pull(&self, out: &mut [f32]) {
        let mut state = self.lock();
        if !state.playing {
            if state.buffer.len() < START {
                out.fill(0.0);
                return;
            }
            state.playing = true;
        }
        let count = out.len().min(state.buffer.len());
        for (sample, value) in out.iter_mut().zip(state.buffer.drain(..count)) {
            // Opus does not clip its output: a loud packet comes back above full scale.
            *sample = value.clamp(-1.0, 1.0);
        }
        out[count..].fill(0.0);
        if state.buffer.is_empty() {
            // An underrun: wait for 40 ms of sound again.
            state.playing = false;
        }
    }

    /// Makes channel `id` the holder. Tells the channel before it.
    fn take(&self, id: u64, taken: Taken) {
        let before = {
            let mut state = self.lock();
            let before = state.holder.replace(Holder { id, taken });
            state.opus = None;
            state.expected = None;
            state.buffer.clear();
            state.playing = false;
            state.tokens = BURST;
            state.last = Instant::now();
            before
        };
        if let Some(before) = before
            && before.id != id
        {
            (before.taken)();
        }
        security("accepted", "start");
    }

    /// Stops the microphone if channel `id` holds it.
    fn release(&self, id: u64) {
        let stopped = {
            let mut state = self.lock();
            if state.holder.as_ref().is_some_and(|holder| holder.id == id) {
                state.holder = None;
                state.opus = None;
                state.buffer.clear();
                state.playing = false;
                true
            } else {
                false
            }
        };
        if stopped {
            security("accepted", "stop");
        }
    }

    /// Takes the payload of a data frame of channel `id` at the time `now`.
    fn packet(&self, id: u64, payload: &[u8], now: Instant) -> Result<(), PacketError> {
        if payload.len() < 4 || payload.len() > 4 + MAX_PACKET {
            return Err(PacketError::Size);
        }
        let mut state = self.lock();
        if !state.holder.as_ref().is_some_and(|holder| holder.id == id) {
            // A packet that was on the way when a different channel took the microphone.
            return Ok(());
        }
        let elapsed = now.saturating_duration_since(state.last).as_secs_f64();
        state.last = now;
        state.tokens = (state.tokens + elapsed * RATE).min(BURST);
        if state.tokens < 1.0 {
            return Err(PacketError::TooFast);
        }
        state.tokens -= 1.0;
        let seq = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let lost = match state.expected {
            Some(expected) => {
                let gap = seq.wrapping_sub(expected);
                if gap > u32::MAX / 2 {
                    // An old packet: its time is over.
                    return Ok(());
                }
                if gap <= MAX_CONCEALED { gap } else { 0 }
            }
            None => 0,
        };
        state.expected = Some(seq.wrapping_add(1));
        state.decode(lost, &payload[4..]);
        Ok(())
    }
}

impl MicGuard {
    /// Starts the microphone for this channel. A different channel that held it gets
    /// its `taken` callback.
    pub fn start(&self) {
        self.started.store(true, Ordering::Release);
        self.mic.take(self.id, self.taken.clone());
    }

    /// Stops the microphone if this channel holds it.
    pub fn stop(&self) {
        self.started.store(false, Ordering::Release);
        self.mic.release(self.id);
    }

    /// Takes the payload of a data frame: the packet number as u32, little-endian, then one Opus packet.
    ///
    /// # Errors
    ///
    /// [`PacketError`] if the channel did not start, the size is not valid, or the packets come too fast.
    /// The agent then closes the channel.
    pub fn packet(&self, payload: &[u8]) -> Result<(), PacketError> {
        self.packet_at(payload, Instant::now())
    }

    fn packet_at(&self, payload: &[u8], now: Instant) -> Result<(), PacketError> {
        let result = if self.started.load(Ordering::Acquire) {
            self.mic.packet(self.id, payload, now)
        } else {
            Err(PacketError::NotStarted)
        };
        if let Err(error) = result {
            security("refused", error.name());
        }
        result
    }
}

impl State {
    /// Conceals `lost` packets, then decodes `packet` into the buffer.
    fn decode(&mut self, lost: u32, packet: &[u8]) {
        if self.opus.is_none() {
            match OpusDecoder::new(SampleRate::Hz48000, Channels::Mono) {
                Ok(opus) => self.opus = Some(opus),
                Err(error) => {
                    tracing::warn!(%error, "no Opus decoder for the microphone");
                    return;
                }
            }
        }
        let Some(opus) = self.opus.as_mut() else {
            return;
        };
        let mut pcm = [0.0_f32; FRAME];
        for _ in 0..lost {
            // An empty packet conceals one frame.
            if let Ok(samples) = opus.decode(&[], FRAME, &mut pcm) {
                self.buffer.extend(&pcm[..samples]);
            }
        }
        match opus.decode(packet, FRAME, &mut pcm) {
            Ok(samples) => self.buffer.extend(&pcm[..samples]),
            Err(error) => tracing::warn!(%error, "a microphone packet did not decode"),
        }
        if self.buffer.len() > MAX_DELAY {
            // Too much delay: keep the newest 40 ms.
            let extra = self.buffer.len() - START;
            self.buffer.drain(..extra);
        }
    }
}

impl Mic {
    /// Starts the source in the PipeWire server of the user, on a thread of its own.
    /// The source stays until the returned value drops.
    ///
    /// # Errors
    ///
    /// A message if PipeWire does not answer or refuses the source.
    pub fn start_source(self: &Arc<Self>) -> Result<SourceHandle, String> {
        let (quit, quit_rx) = pipewire::channel::channel::<()>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let mic = self.clone();
        let thread = std::thread::Builder::new()
            .name("farglass-mic".to_owned())
            .spawn(move || {
                if let Err(error) = source::run(&mic, quit_rx, &ready_tx) {
                    let _ = ready_tx.send(Err(error));
                }
                mic.set_available(false);
            })
            .map_err(|e| e.to_string())?;
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "PipeWire did not answer".to_owned())??;
        self.set_available(true);
        Ok(SourceHandle {
            quit,
            thread: Some(thread),
        })
    }
}

/// The running source. Dropping it removes the source from PipeWire.
pub struct SourceHandle {
    quit: pipewire::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for SourceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceHandle").finish_non_exhaustive()
    }
}

impl Drop for SourceHandle {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The PipeWire side: a stream that is a source node.
mod source {
    use std::sync::Arc;

    use pipewire as pw;
    use pw::properties::properties;
    use pw::spa;
    use spa::pod::Pod;

    use super::{Mic, SAMPLE_RATE, SOURCE_NAME};

    /// Runs the source until a message comes on `quit`. Tells `ready` when the source is there.
    pub(super) fn run(
        mic: &Arc<Mic>,
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
            *pw::keys::MEDIA_CLASS => "Audio/Source",
            *pw::keys::NODE_NAME => SOURCE_NAME,
            *pw::keys::NODE_DESCRIPTION => "farglass browser microphone",
            *pw::keys::NODE_LATENCY => "960/48000",
            "audio.position" => "MONO",
        };
        let stream = pw::stream::StreamBox::new(&core, "farglass-mic", props).map_err(error)?;
        let samples_of = mic.clone();
        let _listener = stream
            .add_local_listener_with_user_data(Vec::<f32>::new())
            .process(move |stream, samples| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let requested = usize::try_from(buffer.requested()).unwrap_or(0);
                let datas = buffer.datas_mut();
                let Some(data) = datas.first_mut() else {
                    return;
                };
                let written = {
                    let Some(bytes) = data.data() else {
                        return;
                    };
                    let mut frames = bytes.len() / 4;
                    if requested > 0 {
                        frames = frames.min(requested);
                    }
                    samples.clear();
                    samples.resize(frames, 0.0);
                    samples_of.pull(samples);
                    for (chunk, sample) in
                        bytes.as_chunks_mut::<4>().0.iter_mut().zip(samples.iter())
                    {
                        chunk.copy_from_slice(&sample.to_le_bytes());
                    }
                    frames * 4
                };
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = 4;
                *chunk.size_mut() = u32::try_from(written).unwrap_or(0);
            })
            .register()
            .map_err(error)?;

        let values = crate::audio::format_pod(SAMPLE_RATE, &[spa::sys::SPA_AUDIO_CHANNEL_MONO])?;
        let mut params = [Pod::from_bytes(&values).ok_or("the audio format is not valid")?];
        stream
            .connect(
                spa::utils::Direction::Output,
                None,
                pw::stream::StreamFlags::MAP_BUFFERS,
                &mut params,
            )
            .map_err(error)?;
        let stop = mainloop.clone();
        let _quit = quit.attach(mainloop.loop_(), move |()| stop.quit());
        let _ = ready.send(Ok(()));
        tracing::info!(
            source = SOURCE_NAME,
            "the microphone source of the session is in PipeWire"
        );
        mainloop.run();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use opus1::{Application, OpusEncoder};

    use super::*;

    /// Returns `count` Opus packets of a 440 Hz tone, numbered from `first`.
    fn packets(first: u32, count: usize) -> Vec<Vec<u8>> {
        let mut opus =
            OpusEncoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip).unwrap();
        (0..count)
            .map(|n| {
                let tone: Vec<f32> = (n * FRAME..(n + 1) * FRAME)
                    .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5)
                    .collect();
                let mut out = [0_u8; MAX_PACKET];
                let length = opus.encode(&tone, FRAME, &mut out).unwrap();
                let mut payload = (first + n as u32).to_le_bytes().to_vec();
                payload.extend_from_slice(&out[..length]);
                payload
            })
            .collect()
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    #[test]
    fn the_source_waits_for_40_ms_and_then_plays_the_tone() {
        let mic = Mic::new();
        let channel = mic.add_channel(|| {});
        channel.start();
        let packets = packets(0, 3);
        let mut out = vec![1.0; FRAME];
        channel.packet(&packets[0]).unwrap();
        mic.pull(&mut out);
        assert!(out.iter().all(|s| *s == 0.0), "20 ms is not enough");
        channel.packet(&packets[1]).unwrap();
        channel.packet(&packets[2]).unwrap();
        mic.pull(&mut out);
        mic.pull(&mut out);
        assert!(energy(&out) > 0.05, "the tone comes back");
        mic.pull(&mut out);
        mic.pull(&mut out);
        assert!(out.iter().all(|s| *s == 0.0), "an underrun gives silence");
    }

    #[test]
    fn a_gap_is_concealed_an_old_packet_goes_and_the_delay_stays_below_120_ms() {
        let mic = Mic::new();
        let channel = mic.add_channel(|| {});
        channel.start();
        let packets = packets(0, 20);
        channel.packet(&packets[0]).unwrap();
        // Packets 1 and 2 are lost: 60 ms in the buffer.
        channel.packet(&packets[3]).unwrap();
        assert_eq!(mic.lock().buffer.len(), FRAME * 4);
        channel.packet(&packets[1]).unwrap();
        assert_eq!(mic.lock().buffer.len(), FRAME * 4, "an old packet goes");
        for packet in &packets[4..] {
            channel.packet(packet).unwrap();
            assert!(mic.lock().buffer.len() <= MAX_DELAY);
        }
    }

    #[test]
    fn the_last_channel_that_starts_holds_the_microphone() {
        let mic = Mic::new();
        let taken = Arc::new(AtomicBool::new(false));
        let flag = taken.clone();
        let first = mic.add_channel(move || flag.store(true, Ordering::SeqCst));
        let second = mic.add_channel(|| {});
        first.start();
        second.start();
        assert!(taken.load(Ordering::SeqCst), "the first channel learns it");
        let packets = packets(0, 2);
        // A packet of the first channel that was on the way goes nowhere.
        first.packet(&packets[0]).unwrap();
        assert!(mic.lock().buffer.is_empty());
        second.packet(&packets[0]).unwrap();
        assert_eq!(mic.lock().buffer.len(), FRAME);
        drop(first);
        assert!(mic.in_use());
        drop(second);
        assert!(!mic.in_use());
        assert!(mic.lock().buffer.is_empty());
    }

    #[test]
    fn a_packet_before_the_start_a_wrong_size_and_a_flood_are_refused() {
        let mic = Mic::new();
        let channel = mic.add_channel(|| {});
        let packets = packets(0, 1);
        assert_eq!(channel.packet(&packets[0]), Err(PacketError::NotStarted));
        channel.start();
        assert_eq!(channel.packet(&[1, 2, 3]), Err(PacketError::Size));
        assert_eq!(
            channel.packet(&vec![0; 4 + MAX_PACKET + 1]),
            Err(PacketError::Size)
        );
        let now = Instant::now();
        // 3 s of packets at one time are permitted after a network delay.
        for n in 0..150 {
            channel
                .packet_at(&packets[0], now)
                .unwrap_or_else(|e| panic!("{n}: {e}"));
        }
        assert_eq!(
            channel.packet_at(&packets[0], now),
            Err(PacketError::TooFast)
        );
        assert_eq!(PacketError::TooFast.reason(), CloseReason::ResourceLimit);
        // At the rate of the format, the packets go again.
        let later = now + Duration::from_millis(20);
        assert_eq!(channel.packet_at(&packets[0], later), Ok(()));
        channel.stop();
        assert_eq!(channel.packet(&packets[0]), Err(PacketError::NotStarted));
        assert!(!mic.in_use());
    }
}
