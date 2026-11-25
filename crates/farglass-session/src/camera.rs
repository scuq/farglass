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
//! One channel holds the camera at a time. The channel that arms last gets
//! it, and the agent tells the channel before it with
//! [`CameraEvent::Taken`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
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
}

impl PictureError {
    /// Returns the reason of the close frame.
    #[must_use]
    pub const fn reason(self) -> CloseReason {
        match self {
            Self::NotArmed | Self::Format | Self::Size => CloseReason::ProtocolError,
            Self::TooFast => CloseReason::ResourceLimit,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::NotArmed => "not-armed",
            Self::Format => "format",
            Self::Size => "size",
            Self::TooFast => "too-fast",
        }
    }
}

type Events = Arc<dyn Fn(CameraEvent) + Send + Sync>;

/// The channel that holds the camera.
struct Holder {
    id: u64,
    events: Events,
}

/// The state of the holder and of the decoder.
struct State {
    holder: Option<Holder>,
    decoder: Option<openh264::decoder::Decoder>,
    /// Reports whether a stream of the holder runs: a format came, and no idle since.
    streaming: bool,
    /// Reports whether the decoder waits for a key frame.
    need_key: bool,
    /// The pictures that can come now, for the rate limit.
    tokens: f64,
    last: Instant,
}

/// The camera channels and the camera node of the session.
pub struct Camera {
    state: Mutex<State>,
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
        Arc::new(Self {
            state: Mutex::new(State {
                holder: None,
                decoder: None,
                streaming: false,
                need_key: true,
                tokens: BURST,
                last: Instant::now(),
            }),
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
                state.streaming = false;
                state.decoder = None;
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
            state.decoder = None;
            state.streaming = false;
            state.need_key = true;
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
                state.decoder = None;
                state.streaming = false;
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
        match openh264::decoder::Decoder::new() {
            Ok(decoder) => state.decoder = Some(decoder),
            Err(error) => {
                tracing::warn!(%error, "no H.264 decoder for the camera");
                return Ok(());
            }
        }
        state.streaming = true;
        state.need_key = true;
        state.tokens = BURST;
        state.last = now;
        drop(state);
        security("accepted", "start");
        Ok(())
    }

    /// Takes the payload of a data frame of channel `id` at the time `now`.
    fn frame(&self, id: u64, payload: &[u8], now: Instant) -> Result<(), PictureError> {
        if payload.len() < 2 || payload.len() > MAX_PICTURE {
            return Err(PictureError::Size);
        }
        let mut state = self.lock();
        if !state.holder.as_ref().is_some_and(|holder| holder.id == id) || !state.streaming {
            // A picture that was on the way when a different channel took the camera, or when the host app stopped.
            return Ok(());
        }
        let elapsed = now.saturating_duration_since(state.last).as_secs_f64();
        state.last = now;
        state.tokens = (state.tokens + elapsed * f64::from(FRAME_RATE)).min(BURST);
        if state.tokens < 1.0 {
            return Err(PictureError::TooFast);
        }
        state.tokens -= 1.0;
        let key = payload[0] & KEY_FRAME != 0;
        if state.need_key && !key {
            return Ok(());
        }
        let Some(decoder) = state.decoder.as_mut() else {
            return Ok(());
        };
        let decoded = match decoder.decode(&payload[1..]) {
            Ok(Some(yuv)) if yuv.dimensions() != (WIDTH, HEIGHT) => {
                return Err(PictureError::Format);
            }
            Ok(Some(yuv)) => {
                let mut picture = self.picture_lock();
                copy_i420(&yuv, &mut picture);
                true
            }
            Ok(None) => true,
            Err(error) => {
                tracing::debug!(%error, "a camera picture did not decode");
                false
            }
        };
        if decoded {
            state.need_key = false;
            return Ok(());
        }
        state.need_key = true;
        let events = state.holder.as_ref().map(|holder| holder.events.clone());
        drop(state);
        if let Some(events) = events {
            events(CameraEvent::KeyFrame);
        }
        Ok(())
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

    fn events() -> (
        Arc<StdMutex<Vec<CameraEvent>>>,
        impl Fn(CameraEvent) + Send + Sync + 'static,
    ) {
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let store = seen.clone();
        (seen, move |event| store.lock().unwrap().push(event))
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
            channel.frame(&picture).unwrap();
        }
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
    fn the_decoder_waits_for_a_key_frame() {
        let camera = Camera::new();
        let channel = camera.add_channel(|_| {});
        channel.arm();
        camera.set_reading(true);
        channel.format("h264", 1280, 720).unwrap();
        let pictures = pictures(3);
        channel.frame(&pictures[1]).unwrap();
        channel.frame(&pictures[2]).unwrap();
        assert_eq!(luma(&camera, 10, 10), 16, "no picture before the key frame");
        channel.frame(&pictures[0]).unwrap();
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
        channel
            .frame(&[KEY_FRAME, 0, 0, 0, 1, 0x65, 0xff, 0x00])
            .unwrap();
        assert_eq!(seen.lock().unwrap().last(), Some(&CameraEvent::KeyFrame));
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
        first.frame(&pictures(1)[0]).unwrap();
        assert_eq!(luma(&camera, 10, 10), 16);
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
        for _ in 0..60 {
            channel.frame_at(&picture, start).unwrap();
        }
        assert_eq!(
            channel.frame_at(&picture, start),
            Err(PictureError::TooFast)
        );
        assert_eq!(PictureError::TooFast.reason(), CloseReason::ResourceLimit);
        // After 1 s, 30 more pictures can come.
        let later = start + Duration::from_secs(1);
        for _ in 0..30 {
            channel.frame_at(&picture, later).unwrap();
        }
        assert_eq!(
            channel.frame_at(&picture, later),
            Err(PictureError::TooFast)
        );
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
