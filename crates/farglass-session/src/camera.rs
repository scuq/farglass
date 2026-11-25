//! The camera of the client for the apps of the session (F116, ADR-056, ADR-057).
//!
//! The agent adds the video source [`NODE_NAME`] to the PipeWire server of the
//! user. It has one fixed format: I420 at [`WIDTH`]x[`HEIGHT`] and [`FRAME_RATE`]
//! pictures each second. The node is the driver of its graph, and a timer
//! starts each picture.
//!
//! The camera portal gives each host app a PipeWire connection with no node
//! on it, and WirePlumber does not show the cameras on such a connection on
//! Debian 13. Thus the agent gives each connection of a host app with the
//! media role Camera the permissions rwx on this node, and on no other node.
//!
//! The shell sends H.264 pictures on a camera channel only while a host app
//! reads the node. The agent decodes them with openh264, and the node gives
//! the newest picture. With no stream, the node gives black pictures.
//!
//! The camera must not delay the desktop (F118, ADR-058). The link loop only
//! checks a picture and puts it in a queue: a thread of its own with nice 10
//! decodes it. The agent acks each picture, and the shell sends the next one
//! only after the ack. Thus each queue holds at most [`MAX_UNACKED`] pictures.
//!
//! One channel holds the camera at a time. The channel that arms last gets
//! it, and the agent tells the channel before it with
//! [`CameraEvent::Taken`].

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::thread::JoinHandle;
use std::time::Instant;

use farglass_proto::message::{CameraEvent, CloseReason};
use openh264::formats::YUVSource as _;

/// The node name of the camera.
pub const NODE_NAME: &str = "farglass-camera";

/// The width of the pictures, in pixels.
pub const WIDTH: usize = 1280;

/// The height of the pictures, in pixels.
pub const HEIGHT: usize = 720;

/// The pictures each second.
pub const FRAME_RATE: u32 = 30;

/// The codec of the pictures on the channel.
pub const CODEC: &str = "h264";

/// The largest data frame of one picture. A picture at 1.5 Mbit/s is much smaller.
pub const MAX_PICTURE: usize = 512 * 1024;

/// The bytes of one I420 picture.
const PICTURE_BYTES: usize = WIDTH * HEIGHT * 3 / 2;

/// The pictures that can come at one time after a network delay: 2 s.
const BURST: f64 = 60.0;

/// The bit of the flags byte for a key frame.
const KEY_FRAME: u8 = 1;

/// The pictures that can wait for their ack. The shell waits for each ack, and the first picture of a stream can overlap.
pub const MAX_UNACKED: usize = 2;

/// The nice value of the threads of the camera: the desktop goes first.
const NICE: i32 = 10;

/// A message or a data frame that the agent refuses. The channel closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PictureError {
    /// A format or a picture before the channel armed the camera.
    #[error("The camera channel sent a picture before an arm message.")]
    NotArmed,
    /// A format that is not the format of the node.
    #[error("The camera stream does not have the format of the ready message.")]
    Format,
    /// A data frame with no picture, or with a picture that is too large.
    #[error("The camera picture has a size that is not valid.")]
    Size,
    /// More pictures than the frame rate gives.
    #[error("The camera channel sent more pictures than its frame rate gives.")]
    TooFast,
    /// More pictures with no ack than [`MAX_UNACKED`].
    #[error("The camera channel sent pictures before their acks.")]
    Unacked,
}

impl PictureError {
    /// Returns the reason of the close frame.
    #[must_use]
    pub const fn reason(self) -> CloseReason {
        match self {
            Self::NotArmed | Self::Format | Self::Size => CloseReason::ProtocolError,
            Self::TooFast | Self::Unacked => CloseReason::ResourceLimit,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::NotArmed => "not-armed",
            Self::Format => "format",
            Self::Size => "size",
            Self::TooFast => "too-fast",
            Self::Unacked => "unacked",
        }
    }
}

type Events = Arc<dyn Fn(CameraEvent) + Send + Sync>;

/// The channel that holds the camera.
struct Holder {
    id: u64,
    events: Events,
}

/// The state of the holder and of its stream.
struct State {
    holder: Option<Holder>,
    /// Reports whether a stream of the holder runs: a format came, and no idle since.
    streaming: bool,
    /// The number of the stream. Each format, arm, disarm, and idle counts up, and the decode thread drops older pictures.
    generation: u64,
    /// The pictures of the stream that wait for their ack.
    unacked: usize,
    /// Reports whether the decoder got a picture of a different size: the next picture closes the channel.
    wrong_size: bool,
    /// The pictures that can come now, for the rate limit.
    tokens: f64,
    last: Instant,
}

/// A picture for the decode thread.
struct Job {
    generation: u64,
    key: bool,
    data: Vec<u8>,
}

/// The camera channels and the camera node of the session.
pub struct Camera {
    me: Weak<Camera>,
    state: Mutex<State>,
    /// The queue of the decode thread. The thread starts with the first stream.
    decode: Mutex<Option<SyncSender<Job>>>,
    /// The pictures in the queue or in the decoder.
    busy: AtomicUsize,
    /// The newest picture in I420, for the node.
    picture: Mutex<Vec<u8>>,
    next: AtomicU64,
    running: AtomicBool,
    reading: AtomicBool,
}

impl std::fmt::Debug for Camera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Camera")
            .field("running", &self.running.load(Ordering::Relaxed))
            .field("reading", &self.reading.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// One camera channel. Dropping it turns the camera off if the channel holds it.
pub struct CameraGuard {
    camera: Arc<Camera>,
    id: u64,
    /// Reports whether the shell sent an arm and no disarm.
    armed: AtomicBool,
    /// Gets the events for this channel.
    events: Events,
}

impl std::fmt::Debug for CameraGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CameraGuard").field("id", &self.id).finish()
    }
}

impl Drop for CameraGuard {
    fn drop(&mut self) {
        self.camera.release(self.id);
    }
}

/// Logs a camera event in the form of the security log (ADR-043).
fn security(result: &str, reason: &str) {
    let uid = nix::unistd::getuid();
    if result == "accepted" {
        tracing::info!(
            target: "farglass::security",
            "event=camera result={result} reason={reason} addr=- user=- uid={uid}"
        );
    } else {
        tracing::warn!(
            target: "farglass::security",
            "event=camera result={result} reason={reason} addr=- user=- uid={uid}"
        );
    }
}

/// Gives the thread that calls it the nice value [`NICE`], so the threads of the desktop go first.
#[allow(unsafe_code)]
fn lower_priority() {
    // SAFETY: setpriority and gettid take plain integers and touch no memory of the program.
    // With the id of this thread, setpriority changes only this thread.
    let result =
        unsafe { nix::libc::setpriority(nix::libc::PRIO_PROCESS, nix::libc::gettid() as _, NICE) };
    if result != 0 {
        tracing::debug!("the camera thread keeps its priority");
    }
}

/// Returns a black picture in I420.
fn black() -> Vec<u8> {
    let mut picture = vec![16; PICTURE_BYTES];
    picture[WIDTH * HEIGHT..].fill(128);
    picture
}

/// Reports whether a setting of the host refuses the camera to all apps.
///
/// The GTK and the GNOME backend of the portal read
/// `org.gnome.desktop.privacy disable-camera`. With no `gsettings`, the camera is not blocked.
#[must_use]
pub fn host_blocks_camera() -> bool {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.privacy", "disable-camera"])
        .stderr(std::process::Stdio::null())
        .output()
        .is_ok_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true"
        })
}

impl Camera {
    /// Returns the camera of a session with no node yet.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            state: Mutex::new(State {
                holder: None,
                streaming: false,
                generation: 0,
                unacked: 0,
                wrong_size: false,
                tokens: BURST,
                last: Instant::now(),
            }),
            decode: Mutex::new(None),
            busy: AtomicUsize::new(0),
            picture: Mutex::new(black()),
            next: AtomicU64::new(1),
            running: AtomicBool::new(false),
            reading: AtomicBool::new(false),
        })
    }

    /// Reports whether the node runs.
    #[must_use]
    pub fn available(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Marks the node as running, for example in a test with no PipeWire.
    pub fn set_available(&self, running: bool) {
        self.running.store(running, Ordering::Release);
    }

    /// Reports whether a host app reads the node.
    #[must_use]
    pub fn reading(&self) -> bool {
        self.reading.load(Ordering::Acquire)
    }

    /// Records if a host app reads the node, and tells the holder when it changes.
    /// The node calls it. A test can call it in place of a host app.
    pub fn set_reading(&self, reading: bool) {
        if self.reading.swap(reading, Ordering::AcqRel) == reading {
            return;
        }
        let (events, stopped) = {
            let mut state = self.lock();
            let stopped = !reading && state.streaming;
            if !reading {
                state.end_stream();
            }
            (
                state.holder.as_ref().map(|holder| holder.events.clone()),
                stopped,
            )
        };
        if !reading {
            *self.picture_lock() = black();
        }
        if stopped {
            security("accepted", "stop");
        }
        if let Some(events) = events {
            events(if reading {
                CameraEvent::Reading
            } else {
                CameraEvent::Idle
            });
        }
    }

    /// Adds a camera channel. `events` gets the events for the channel.
    pub fn add_channel(
        self: &Arc<Self>,
        events: impl Fn(CameraEvent) + Send + Sync + 'static,
    ) -> CameraGuard {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        CameraGuard {
            camera: self.clone(),
            id,
            armed: AtomicBool::new(false),
            events: Arc::new(events),
        }
    }

    /// Reports whether a channel holds the camera.
    #[must_use]
    pub fn in_use(&self) -> bool {
        self.lock().holder.is_some()
    }

    /// Returns the pictures in the queue of the decode thread or in its decoder, for a test that waits for the decode.
    #[must_use]
    pub fn decoding(&self) -> usize {
        self.busy.load(Ordering::Acquire)
    }

    /// Returns the pictures of the stream that wait for their ack.
    #[must_use]
    pub fn unacked(&self) -> usize {
        self.lock().unacked
    }

    /// Returns a copy of the picture that the node gives now, in I420.
    #[must_use]
    pub fn picture(&self) -> Vec<u8> {
        self.picture_lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn picture_lock(&self) -> std::sync::MutexGuard<'_, Vec<u8>> {
        self.picture.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes channel `id` the holder. Tells the channel before it, and tells the new one if a host app reads.
    fn take(&self, id: u64, events: Events) {
        let (before, stopped) = {
            let mut state = self.lock();
            let stopped = state.streaming;
            let before = state.holder.replace(Holder {
                id,
                events: events.clone(),
            });
            state.end_stream();
            before.map(|before| (before, stopped))
        }
        .map_or((None, false), |(before, stopped)| (Some(before), stopped));
        if stopped {
            security("accepted", "stop");
        }
        if let Some(before) = before
            && before.id != id
        {
            (before.events)(CameraEvent::Taken);
        }
        security("accepted", "arm");
        if self.reading() {
            events(CameraEvent::Reading);
        }
    }

    /// Turns the camera off if channel `id` holds it.
    fn release(&self, id: u64) {
        let (released, stopped) = {
            let mut state = self.lock();
            if state.holder.as_ref().is_some_and(|holder| holder.id == id) {
                let stopped = state.streaming;
                state.holder = None;
                state.end_stream();
                (true, stopped)
            } else {
                (false, false)
            }
        };
        if released {
            *self.picture_lock() = black();
            if stopped {
                security("accepted", "stop");
            }
            security("accepted", "disarm");
        }
    }

    /// Starts a new stream of channel `id`.
    fn format(
        &self,
        id: u64,
        codec: &str,
        width: u32,
        height: u32,
        now: Instant,
    ) -> Result<(), PictureError> {
        if codec != CODEC || width as usize != WIDTH || height as usize != HEIGHT {
            return Err(PictureError::Format);
        }
        let mut state = self.lock();
        if !state.holder.as_ref().is_some_and(|holder| holder.id == id) {
            // A format that was on the way when a different channel took the camera.
            return Ok(());
        }
        state.end_stream();
        state.streaming = true;
        state.tokens = BURST;
        state.last = now;
        drop(state);
        security("accepted", "start");
        Ok(())
    }

    /// Takes the payload of a data frame of channel `id` at the time `now`, and gives it to the decode thread.
    fn frame(&self, id: u64, payload: &[u8], now: Instant) -> Result<(), PictureError> {
        if payload.len() < 2 || payload.len() > MAX_PICTURE {
            return Err(PictureError::Size);
        }
        let mut state = self.lock();
        if !state.holder.as_ref().is_some_and(|holder| holder.id == id) || !state.streaming {
            // A picture that was on the way when a different channel took the camera, or when the host app stopped.
            return Ok(());
        }
        if state.wrong_size {
            return Err(PictureError::Format);
        }
        let elapsed = now.saturating_duration_since(state.last).as_secs_f64();
        state.last = now;
        state.tokens = (state.tokens + elapsed * f64::from(FRAME_RATE)).min(BURST);
        if state.tokens < 1.0 {
            return Err(PictureError::TooFast);
        }
        if state.unacked >= MAX_UNACKED {
            return Err(PictureError::Unacked);
        }
        state.tokens -= 1.0;
        let job = Job {
            generation: state.generation,
            key: payload[0] & KEY_FRAME != 0,
            data: payload[1..].to_vec(),
        };
        let Some(queue) = self.decode_queue() else {
            return Ok(());
        };
        match queue.try_send(job) {
            Ok(()) => {
                state.unacked += 1;
                self.busy.fetch_add(1, Ordering::AcqRel);
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(PictureError::Unacked),
            Err(TrySendError::Disconnected(_)) => Ok(()),
        }
    }

    /// Returns the queue of the decode thread, and starts the thread the first time.
    fn decode_queue(&self) -> Option<SyncSender<Job>> {
        let mut decode = self.decode.lock().unwrap_or_else(PoisonError::into_inner);
        if decode.is_none() {
            let (queue, jobs) = std::sync::mpsc::sync_channel(MAX_UNACKED);
            let camera = self.me.clone();
            let started = std::thread::Builder::new()
                .name("farglass-camera-decode".to_owned())
                .spawn(move || decode_loop(&camera, &jobs));
            match started {
                Ok(_) => *decode = Some(queue),
                Err(error) => tracing::warn!(%error, "no decode thread for the camera"),
            }
        }
        decode.clone()
    }

    /// Acks a picture of stream `generation` when the decode thread takes it from the queue.
    ///
    /// The ack comes before the decode: the time of the decode grows with the size of the picture, and the shell
    /// takes its round trip for the time of the link. The decode thread takes the next picture only after the decode,
    /// thus a slow decode still lowers the frame rate. A picture of an old stream gets no ack.
    fn taken(&self, generation: u64) {
        let events = {
            let mut state = self.lock();
            if state.generation != generation {
                return;
            }
            state.unacked = state.unacked.saturating_sub(1);
            state.holder.as_ref().map(|holder| holder.events.clone())
        };
        if let Some(events) = events {
            events(CameraEvent::Ack);
        }
    }

    /// Ends the work of the decode thread on a picture of stream `generation`, and asks for a key frame if `key_frame`.
    /// `picture` is the decoded picture, if any. A picture of an old stream changes nothing.
    fn done(&self, generation: u64, picture: Decoded<'_>, key_frame: bool) {
        // The decoder is free after this call, also for a picture of an old stream.
        struct Free<'a>(&'a AtomicUsize);
        impl Drop for Free<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::AcqRel);
            }
        }
        let _free = Free(&self.busy);
        let events = {
            let mut state = self.lock();
            if state.generation != generation {
                return;
            }
            match picture {
                Decoded::Picture(yuv) => copy_i420(yuv, &mut self.picture_lock()),
                Decoded::WrongSize => state.wrong_size = true,
                Decoded::Nothing => {}
            }
            state.holder.as_ref().map(|holder| holder.events.clone())
        };
        if key_frame && let Some(events) = events {
            events(CameraEvent::KeyFrame);
        }
    }
}

impl State {
    /// Ends the stream: the decode thread drops its pictures, and no ack is due.
    fn end_stream(&mut self) {
        self.streaming = false;
        self.generation += 1;
        self.unacked = 0;
        self.wrong_size = false;
    }
}

/// What the decode thread got from one picture.
enum Decoded<'a> {
    /// A picture of [`WIDTH`]x[`HEIGHT`].
    Picture(&'a openh264::decoder::DecodedYUV<'a>),
    /// A picture of a different size.
    WrongSize,
    /// No picture: the decoder needs more data, or dropped the data.
    Nothing,
}

/// Decodes the pictures of `jobs` until the camera goes. A new stream gets a new decoder.
fn decode_loop(camera: &Weak<Camera>, jobs: &Receiver<Job>) {
    lower_priority();
    let mut decoder: Option<openh264::decoder::Decoder> = None;
    let mut generation = u64::MAX;
    let mut need_key = true;
    while let Ok(job) = jobs.recv() {
        let Some(camera) = camera.upgrade() else {
            return;
        };
        camera.taken(job.generation);
        if job.generation != generation {
            generation = job.generation;
            need_key = true;
            decoder = openh264::decoder::Decoder::new()
                .map_err(|error| tracing::warn!(%error, "no H.264 decoder for the camera"))
                .ok();
        }
        if need_key && !job.key {
            // The pictures of a stream before its first key frame do not decode.
            camera.done(job.generation, Decoded::Nothing, false);
            continue;
        }
        let Some(decoder) = decoder.as_mut() else {
            camera.done(job.generation, Decoded::Nothing, false);
            continue;
        };
        match decoder.decode(&job.data) {
            Ok(Some(yuv)) if yuv.dimensions() == (WIDTH, HEIGHT) => {
                need_key = false;
                camera.done(job.generation, Decoded::Picture(&yuv), false);
            }
            Ok(Some(_)) => camera.done(job.generation, Decoded::WrongSize, false),
            Ok(None) => {
                need_key = false;
                camera.done(job.generation, Decoded::Nothing, false);
            }
            Err(error) => {
                tracing::debug!(%error, "a camera picture did not decode");
                need_key = true;
                camera.done(job.generation, Decoded::Nothing, true);
            }
        }
    }
}

/// Copies a decoded picture of [`WIDTH`]x[`HEIGHT`] into `out` as I420 with no padding.
fn copy_i420(yuv: &impl openh264::formats::YUVSource, out: &mut [u8]) {
    let (y_stride, u_stride, v_stride) = yuv.strides();
    let (luma, chroma) = out.split_at_mut(WIDTH * HEIGHT);
    let (u_out, v_out) = chroma.split_at_mut(WIDTH * HEIGHT / 4);
    let planes: [(&[u8], usize, &mut [u8], usize); 3] = [
        (yuv.y(), y_stride, luma, WIDTH),
        (yuv.u(), u_stride, u_out, WIDTH / 2),
        (yuv.v(), v_stride, v_out, WIDTH / 2),
    ];
    for (plane, stride, out, width) in planes {
        for (row, line) in out.chunks_exact_mut(width).enumerate() {
            if let Some(source) = plane.get(row * stride..row * stride + width) {
                line.copy_from_slice(source);
            }
        }
    }
}

impl CameraGuard {
    /// Arms the camera for this channel. A different channel that held it gets [`CameraEvent::Taken`].
    pub fn arm(&self) {
        self.armed.store(true, Ordering::Release);
        self.camera.take(self.id, self.events.clone());
    }

    /// Turns the camera off if this channel holds it.
    pub fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
        self.camera.release(self.id);
    }

    /// Starts a new stream with the format of a format message.
    ///
    /// # Errors
    ///
    /// [`PictureError`] if the channel did not arm, or the format is not the format of the node.
    /// The agent then closes the channel.
    pub fn format(&self, codec: &str, width: u32, height: u32) -> Result<(), PictureError> {
        let result = if self.armed.load(Ordering::Acquire) {
            self.camera
                .format(self.id, codec, width, height, Instant::now())
        } else {
            Err(PictureError::NotArmed)
        };
        if let Err(error) = result {
            security("refused", error.name());
        }
        result
    }

    /// Takes the payload of a data frame: a flags byte, then one H.264 access unit in Annex B form.
    ///
    /// # Errors
    ///
    /// [`PictureError`] if the channel did not arm, the size is not valid, or the pictures come too fast.
    /// The agent then closes the channel.
    pub fn frame(&self, payload: &[u8]) -> Result<(), PictureError> {
        self.frame_at(payload, Instant::now())
    }

    fn frame_at(&self, payload: &[u8], now: Instant) -> Result<(), PictureError> {
        let result = if self.armed.load(Ordering::Acquire) {
            self.camera.frame(self.id, payload, now)
        } else {
            Err(PictureError::NotArmed)
        };
        if let Err(error) = result {
            security("refused", error.name());
        }
        result
    }
}

impl Camera {
    /// Starts the camera node in the PipeWire server of the user, on a thread of its own.
    /// The node stays until the returned value drops.
    ///
    /// # Errors
    ///
    /// A message if PipeWire does not answer or refuses the node.
    pub fn start_node(self: &Arc<Self>) -> Result<NodeHandle, String> {
        let (quit, quit_rx) = pipewire::channel::channel::<()>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let camera = self.clone();
        let thread = std::thread::Builder::new()
            .name("farglass-camera".to_owned())
            .spawn(move || {
                lower_priority();
                if let Err(error) = node::run(&camera, quit_rx, &ready_tx) {
                    let _ = ready_tx.send(Err(error));
                }
                camera.set_available(false);
                camera.set_reading(false);
            })
            .map_err(|e| e.to_string())?;
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "PipeWire did not answer".to_owned())??;
        self.set_available(true);
        Ok(NodeHandle {
            quit,
            thread: Some(thread),
        })
    }
}

/// The running node. Dropping it removes the node from PipeWire.
pub struct NodeHandle {
    quit: pipewire::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for NodeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeHandle").finish_non_exhaustive()
    }
}

impl Drop for NodeHandle {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The PipeWire side: a stream that is a video source node, and the permissions of the portal connections.
mod node {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Duration;

    use pipewire as pw;
    use pw::permissions::{Permission, PermissionFlags};
    use pw::properties::properties;
    use pw::spa;
    use spa::pod::{ChoiceValue, Object, Pod, Property, PropertyFlags, Value};
    use spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle};

    use super::{Camera, FRAME_RATE, HEIGHT, NODE_NAME, PICTURE_BYTES, WIDTH};

    fn property(key: u32, value: Value) -> Property {
        Property {
            key,
            flags: PropertyFlags::empty(),
            value,
        }
    }

    fn serialize(value: &Value) -> Result<Vec<u8>, String> {
        Ok(
            spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), value)
                .map_err(|e| format!("{e:?}"))?
                .0
                .into_inner(),
        )
    }

    /// Returns the EnumFormat pod: I420 at the size and frame rate of the camera, as plain values (ADR-057).
    fn enum_format() -> Result<Vec<u8>, String> {
        serialize(&Value::Object(Object {
            type_: spa::sys::SPA_TYPE_OBJECT_Format,
            id: spa::sys::SPA_PARAM_EnumFormat,
            properties: vec![
                property(
                    spa::sys::SPA_FORMAT_mediaType,
                    Value::Id(Id(spa::sys::SPA_MEDIA_TYPE_video)),
                ),
                property(
                    spa::sys::SPA_FORMAT_mediaSubtype,
                    Value::Id(Id(spa::sys::SPA_MEDIA_SUBTYPE_raw)),
                ),
                property(
                    spa::sys::SPA_FORMAT_VIDEO_format,
                    Value::Id(Id(spa::sys::SPA_VIDEO_FORMAT_I420)),
                ),
                property(
                    spa::sys::SPA_FORMAT_VIDEO_size,
                    Value::Rectangle(Rectangle {
                        width: WIDTH as u32,
                        height: HEIGHT as u32,
                    }),
                ),
                property(
                    spa::sys::SPA_FORMAT_VIDEO_framerate,
                    Value::Fraction(Fraction {
                        num: FRAME_RATE,
                        denom: 1,
                    }),
                ),
            ],
        }))
    }

    /// Returns the Buffers pod: 2 to 16 buffers with one block of one picture each.
    fn buffers() -> Result<Vec<u8>, String> {
        serialize(&Value::Object(Object {
            type_: spa::sys::SPA_TYPE_OBJECT_ParamBuffers,
            id: spa::sys::SPA_PARAM_Buffers,
            properties: vec![
                property(
                    spa::sys::SPA_PARAM_BUFFERS_buffers,
                    Value::Choice(ChoiceValue::Int(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Range {
                            default: 4,
                            min: 2,
                            max: 16,
                        },
                    ))),
                ),
                property(spa::sys::SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
                property(
                    spa::sys::SPA_PARAM_BUFFERS_size,
                    Value::Int(PICTURE_BYTES as i32),
                ),
                property(spa::sys::SPA_PARAM_BUFFERS_stride, Value::Int(WIDTH as i32)),
                property(
                    spa::sys::SPA_PARAM_BUFFERS_dataType,
                    Value::Choice(ChoiceValue::Int(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Flags {
                            default: (1 << spa::sys::SPA_DATA_MemFd)
                                | (1 << spa::sys::SPA_DATA_MemPtr),
                            flags: vec![],
                        },
                    ))),
                ),
            ],
        }))
    }

    /// Reports whether the properties are the ones of a portal connection of a host app for the camera.
    pub(super) fn host_camera_client(props: &spa::utils::dict::DictRef) -> bool {
        props.get("pipewire.access.portal.app_id") == Some("")
            && props
                .get("pipewire.access.portal.media_roles")
                .is_some_and(|roles| roles.split(',').any(|role| role.trim() == "Camera"))
    }

    /// A client of the user: its proxy, and if it is a portal connection of a host app for the camera.
    struct Client {
        id: u32,
        proxy: Rc<pw::client::Client>,
        _listener: pw::client::ClientListener,
        camera: Rc<Cell<bool>>,
    }

    /// Gives the client rwx on the node.
    fn grant(proxy: &pw::client::Client, node: u32) {
        proxy.update_permissions(&[Permission::new(
            node,
            PermissionFlags::R | PermissionFlags::W | PermissionFlags::X,
        )]);
        tracing::debug!(
            node,
            "the camera node is visible to a portal connection of a host app"
        );
    }

    /// Runs the node until a message comes on `quit`. Tells `ready` when the node is there.
    pub(super) fn run(
        camera: &Arc<Camera>,
        quit: pw::channel::Receiver<()>,
        ready: &std::sync::mpsc::Sender<Result<(), String>>,
    ) -> Result<(), String> {
        let error = |e: pw::Error| e.to_string();
        pw::init();
        let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(error)?;
        let context = pw::context::ContextRc::new(&mainloop, None).map_err(error)?;
        let core = context.connect_rc(None).map_err(error)?;
        let props = properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_CLASS => "Video/Source",
            *pw::keys::MEDIA_ROLE => "Camera",
            *pw::keys::NODE_NAME => NODE_NAME,
            *pw::keys::NODE_DESCRIPTION => "farglass browser camera",
        };
        let stream = pw::stream::StreamRc::new(core.clone(), NODE_NAME, props).map_err(error)?;
        let node_id = Rc::new(Cell::new(u32::MAX));
        let clients: Rc<RefCell<Vec<Client>>> = Rc::default();
        let streaming = Rc::new(Cell::new(false));

        let reading_of = camera.clone();
        let picture_of = camera.clone();
        let on = streaming.clone();
        let id_of = node_id.clone();
        let clients_of = clients.clone();
        let buffers = buffers()?;
        let _listener = stream
            .add_local_listener_with_user_data(())
            .state_changed(move |stream, (), _, new| {
                let live = matches!(new, pw::stream::StreamState::Streaming);
                on.set(live);
                reading_of.set_reading(live);
                let id = stream.node_id();
                if id != u32::MAX && id_of.get() != id {
                    id_of.set(id);
                    for client in clients_of.borrow().iter().filter(|c| c.camera.get()) {
                        grant(&client.proxy, id);
                    }
                }
            })
            .param_changed(move |stream, (), id, param| {
                if id != spa::sys::SPA_PARAM_Format || param.is_none() {
                    return;
                }
                let Some(pod) = Pod::from_bytes(&buffers) else {
                    return;
                };
                if let Err(error) = stream.update_params(&mut [pod]) {
                    tracing::warn!(%error, "the camera node did not take its buffer size");
                }
            })
            .process(move |stream, ()| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let Some(data) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let written = data.data().map_or(0, |bytes| {
                    let picture = picture_of.picture_lock();
                    let count = bytes.len().min(picture.len());
                    bytes[..count].copy_from_slice(&picture[..count]);
                    count
                });
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = WIDTH as i32;
                *chunk.size_mut() = u32::try_from(written).unwrap_or(0);
            })
            .register()
            .map_err(error)?;

        // The portal connections of host apps: each gets rwx on the node (ADR-056).
        let registry = core.get_registry_rc().map_err(error)?;
        let registry_of = registry.clone();
        let added = clients.clone();
        let node_of = node_id.clone();
        let removed = clients.clone();
        let _registry_listener = registry
            .add_listener_local()
            .global(move |global| {
                if global.type_ != pw::types::ObjectType::Client {
                    return;
                }
                let Ok(proxy) = registry_of.bind::<pw::client::Client, _>(global) else {
                    return;
                };
                let proxy = Rc::new(proxy);
                let camera = Rc::new(Cell::new(false));
                let flag = camera.clone();
                let target = proxy.clone();
                let node = node_of.clone();
                let listener = proxy
                    .add_listener_local()
                    .info(move |info| {
                        if flag.get() || !info.props().is_some_and(host_camera_client) {
                            return;
                        }
                        flag.set(true);
                        if node.get() != u32::MAX {
                            grant(&target, node.get());
                        }
                    })
                    .register();
                added.borrow_mut().push(Client {
                    id: global.id,
                    proxy,
                    _listener: listener,
                    camera,
                });
            })
            .global_remove(move |id| removed.borrow_mut().retain(|client| client.id != id))
            .register();

        let values = enum_format()?;
        let mut params = [Pod::from_bytes(&values).ok_or("the camera format is not valid")?];
        stream
            .connect(
                spa::utils::Direction::Output,
                None,
                pw::stream::StreamFlags::DRIVER | pw::stream::StreamFlags::MAP_BUFFERS,
                &mut params,
            )
            .map_err(error)?;

        // The node is the driver: a timer starts each picture while a host app reads.
        let tick = mainloop.loop_().add_timer({
            let stream = stream.clone();
            move |_| {
                if streaming.get() {
                    let _ = stream.trigger_process();
                }
            }
        });
        let interval = Duration::from_micros(1_000_000 / u64::from(FRAME_RATE));
        let _ = tick.update_timer(Some(interval), Some(interval));

        let stop = mainloop.clone();
        let _quit = quit.attach(mainloop.loop_(), move |()| stop.quit());
        let _ = ready.send(Ok(()));
        tracing::info!(
            node = NODE_NAME,
            "the camera node of the session is in PipeWire"
        );
        mainloop.run();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use openh264::encoder::Encoder;
    use openh264::formats::YUVBuffer;

    use super::*;

    /// Returns `count` data frame payloads: a gray picture with a bright square, H.264, the first one a key frame.
    pub(crate) fn pictures(count: usize) -> Vec<Vec<u8>> {
        let mut encoder = Encoder::new().unwrap();
        (0..count)
            .map(|n| {
                let mut yuv = vec![128_u8; PICTURE_BYTES];
                yuv[..WIDTH * HEIGHT].fill(90);
                for row in 100..300 {
                    let at = row * WIDTH + 200 + n * 4;
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

    type Seen = Arc<StdMutex<Vec<CameraEvent>>>;

    fn events() -> (Seen, impl Fn(CameraEvent) + Send + Sync + 'static) {
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let store = seen.clone();
        (seen, move |event| store.lock().unwrap().push(event))
    }

    /// Returns the events with no ack.
    fn without_acks(seen: &Seen) -> Vec<CameraEvent> {
        let all = seen.lock().unwrap();
        all.iter()
            .filter(|e| **e != CameraEvent::Ack)
            .cloned()
            .collect()
    }

    /// Waits until the decode thread acked and decoded each picture.
    fn settle(camera: &Camera) {
        let end = Instant::now() + Duration::from_secs(10);
        while camera.unacked() > 0 || camera.decoding() > 0 {
            assert!(Instant::now() < end, "the decode thread did not ack");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Sends `payload` as the shell does: after the ack of the picture before it.
    fn send(camera: &Camera, channel: &CameraGuard, payload: &[u8]) -> Result<(), PictureError> {
        settle(camera);
        let result = channel.frame(payload);
        settle(camera);
        result
    }

    fn luma(camera: &Camera, x: usize, y: usize) -> u8 {
        camera.picture()[y * WIDTH + x]
    }

    #[test]
    fn a_stream_of_the_holder_reaches_the_picture_of_the_node() {
        let camera = Camera::new();
        let (seen, sink) = events();
        let channel = camera.add_channel(sink);
        assert_eq!(luma(&camera, 0, 0), 16, "black with no stream");
        assert_eq!(channel.frame(&pictures(1)[0]), Err(PictureError::NotArmed));
        channel.arm();
        assert!(seen.lock().unwrap().is_empty(), "no host app reads yet");
        camera.set_reading(true);
        assert_eq!(*seen.lock().unwrap(), [CameraEvent::Reading]);
        assert_eq!(channel.format("h264", 640, 480), Err(PictureError::Format));
        channel.format("h264", 1280, 720).unwrap();
        for picture in pictures(3) {
            send(&camera, &channel, &picture).unwrap();
        }
        assert_eq!(
            seen.lock()
                .unwrap()
                .iter()
                .filter(|e| **e == CameraEvent::Ack)
                .count(),
            3,
            "one ack for each picture"
        );
        assert!(
            (80..100).contains(&luma(&camera, 10, 10)),
            "the gray of the picture"
        );
        assert!(luma(&camera, 300, 200) > 200, "the bright square");
        camera.set_reading(false);
        assert_eq!(seen.lock().unwrap().last(), Some(&CameraEvent::Idle));
        assert_eq!(
            luma(&camera, 10, 10),
            16,
            "black again when no host app reads"
        );
        // A picture on the way after the idle does not close the channel.
        channel.frame(&pictures(1)[0]).unwrap();
        channel.disarm();
        assert!(!camera.in_use());
    }

    #[test]
    fn the_link_loop_does_not_wait_for_the_decode() {
        let camera = Camera::new();
        let channel = camera.add_channel(|_| {});
        channel.arm();
        camera.set_reading(true);
        channel.format("h264", 1280, 720).unwrap();
        let pictures = pictures(2);
        // The first picture starts the thread. The second one goes in the queue while the first decodes.
        send(&camera, &channel, &pictures[0]).unwrap();
        let start = Instant::now();
        channel.frame(&pictures[1]).unwrap();
        let took = start.elapsed();
        assert!(took < Duration::from_millis(5), "frame took {took:?}");
        settle(&camera);
    }

    #[test]
    fn the_shell_waits_for_the_ack_of_each_picture() {
        let camera = Camera::new();
        let channel = camera.add_channel(|_| {});
        channel.arm();
        camera.set_reading(true);
        channel.format("h264", 1280, 720).unwrap();
        // Pictures that are slow to decode: the queue of the thread is full.
        let pictures = pictures(MAX_UNACKED + 1);
        let mut result = Ok(());
        for picture in &pictures {
            result = channel.frame(picture);
            if result.is_err() {
                break;
            }
        }
        if camera.unacked() < MAX_UNACKED {
            // The decode thread was faster than this loop. The window holds then too.
            settle(&camera);
            return;
        }
        assert_eq!(result, Err(PictureError::Unacked));
        assert_eq!(PictureError::Unacked.reason(), CloseReason::ResourceLimit);
    }

    #[test]
    fn the_decoder_waits_for_a_key_frame() {
        let camera = Camera::new();
        let channel = camera.add_channel(|_| {});
        channel.arm();
        camera.set_reading(true);
        channel.format("h264", 1280, 720).unwrap();
        let pictures = pictures(3);
        send(&camera, &channel, &pictures[1]).unwrap();
        send(&camera, &channel, &pictures[2]).unwrap();
        assert_eq!(luma(&camera, 10, 10), 16, "no picture before the key frame");
        send(&camera, &channel, &pictures[0]).unwrap();
        assert!((80..100).contains(&luma(&camera, 10, 10)));
    }

    #[test]
    fn a_picture_that_does_not_decode_asks_for_a_key_frame() {
        let camera = Camera::new();
        let (seen, sink) = events();
        let channel = camera.add_channel(sink);
        channel.arm();
        camera.set_reading(true);
        channel.format("h264", 1280, 720).unwrap();
        send(
            &camera,
            &channel,
            &[KEY_FRAME, 0, 0, 0, 1, 0x65, 0xff, 0x00],
        )
        .unwrap();
        let seen = seen.lock().unwrap();
        // The ack comes when the decode thread takes the picture, the request for a key frame after the decode.
        assert_eq!(
            seen[seen.len() - 2..],
            [CameraEvent::Ack, CameraEvent::KeyFrame]
        );
    }

    #[test]
    fn the_last_channel_that_arms_holds_the_camera() {
        let camera = Camera::new();
        let (first_seen, first_sink) = events();
        let (second_seen, second_sink) = events();
        let first = camera.add_channel(first_sink);
        let second = camera.add_channel(second_sink);
        camera.set_reading(true);
        first.arm();
        assert_eq!(*first_seen.lock().unwrap(), [CameraEvent::Reading]);
        second.arm();
        assert_eq!(
            *first_seen.lock().unwrap(),
            [CameraEvent::Reading, CameraEvent::Taken]
        );
        assert_eq!(*second_seen.lock().unwrap(), [CameraEvent::Reading]);
        // The pictures of the first channel go nowhere.
        first.format("h264", 1280, 720).unwrap();
        send(&camera, &first, &pictures(1)[0]).unwrap();
        assert_eq!(luma(&camera, 10, 10), 16);
        assert_eq!(without_acks(&second_seen), [CameraEvent::Reading]);
        drop(first);
        assert!(
            camera.in_use(),
            "the drop of the first channel does not release the second"
        );
        drop(second);
        assert!(!camera.in_use());
    }

    #[test]
    fn the_agent_limits_the_size_and_the_rate_of_the_pictures() {
        let camera = Camera::new();
        let channel = camera.add_channel(|_| {});
        channel.arm();
        camera.set_reading(true);
        channel.format("h264", 1280, 720).unwrap();
        assert_eq!(channel.frame(&[1]), Err(PictureError::Size));
        assert_eq!(
            channel.frame(&vec![0; MAX_PICTURE + 1]),
            Err(PictureError::Size)
        );
        let start = Instant::now();
        let picture = [0_u8, 0, 0, 0, 1, 9, 0x10];
        let at = |time| {
            settle(&camera);
            let result = channel.frame_at(&picture, time);
            settle(&camera);
            result
        };
        for _ in 0..60 {
            at(start).unwrap();
        }
        assert_eq!(at(start), Err(PictureError::TooFast));
        assert_eq!(PictureError::TooFast.reason(), CloseReason::ResourceLimit);
        // After 1 s, 30 more pictures can come.
        let later = start + Duration::from_secs(1);
        for _ in 0..30 {
            at(later).unwrap();
        }
        assert_eq!(at(later), Err(PictureError::TooFast));
    }

    #[test]
    fn only_a_portal_connection_of_a_host_app_for_the_camera_gets_the_node() {
        use pipewire::properties::properties;
        let host = properties! {
            "pipewire.access.portal.app_id" => "",
            "pipewire.access.portal.media_roles" => "Camera",
        };
        assert!(node::host_camera_client(host.as_ref()));
        let sandboxed = properties! {
            "pipewire.access.portal.app_id" => "org.example.App",
            "pipewire.access.portal.media_roles" => "Camera",
        };
        assert!(!node::host_camera_client(sandboxed.as_ref()));
        let screen = properties! {
            "pipewire.access.portal.app_id" => "",
            "pipewire.access.portal.media_roles" => "Screen",
        };
        assert!(!node::host_camera_client(screen.as_ref()));
        let plain = properties! { "application.name" => "pw-record" };
        assert!(!node::host_camera_client(plain.as_ref()));
    }
}
