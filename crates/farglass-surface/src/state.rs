//! The state of the compositor and its Wayland handlers.

use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixListener;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::backend::input::{Axis, AxisSource, ButtonState, KeyState};
use smithay::delegate_compositor;
use smithay::delegate_cursor_shape;
use smithay::delegate_data_device;
use smithay::delegate_output;
use smithay::delegate_seat;
use smithay::delegate_shm;
use smithay::delegate_xdg_decoration;
use smithay::delegate_xdg_shell;
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorImageStatus, CursorImageSurfaceData, MotionEvent,
};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::output::{Mode as OutputMode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::channel::{self, Channel};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_callback::WlCallback;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, Display, DisplayHandle, Resource as _};
use smithay::utils::{Rectangle, SERIAL_COUNTER, Serial, Transform};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    BufferAssignment, CompositorClientState, CompositorHandler, CompositorState, Damage,
    SurfaceAttributes, with_states,
};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::output::{OutputHandler, OutputManagerState};
use smithay::wayland::seat::WaylandFocus as _;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
    request_data_device_client_selection, set_data_device_focus, set_data_device_selection,
};
use smithay::wayland::selection::{SelectionHandler, SelectionSource, SelectionTarget};
use smithay::wayland::shell::xdg::decoration::{XdgDecorationHandler, XdgDecorationState};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, SurfaceCachedState, ToplevelSurface, XdgShellHandler,
    XdgShellState, XdgToplevelSurfaceData,
};
use smithay::wayland::shm::{ShmHandler, ShmState, with_buffer_contents};
use smithay::wayland::tablet_manager::TabletSeatHandler;
use smithay::xwayland::XWaylandClientData;

use crate::clipboard::{self, Clip, ClipKind};
use crate::pace::{Decision, Pace};

mod x11;

use x11::Focus;

/// The interval of the frame callbacks: a client draws at most this often.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

/// The size of the virtual output until the shell gives the size of its desktop.
const OUTPUT_SIZE: (i32, i32) = (1920, 1080);

/// A rectangle in buffer pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// The left edge.
    pub x: i32,
    /// The top edge.
    pub y: i32,
    /// The width.
    pub width: i32,
    /// The height.
    pub height: i32,
}

/// The pixel format of a commit. Each pixel has 4 bytes: blue, green, red, and alpha or unused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Alpha in the fourth byte.
    Argb8888,
    /// The fourth byte has no meaning: each pixel is opaque.
    Xrgb8888,
}

/// The content of one commit of a surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    /// The surface: a top-level window or a popup.
    pub id: u32,
    /// The width of the buffer in pixels.
    pub width: i32,
    /// The height of the buffer in pixels.
    pub height: i32,
    /// The scale of the buffer: buffer pixels for each logical pixel.
    pub scale: i32,
    /// The pixel format.
    pub format: Format,
    /// The parts of the buffer that changed since the last commit, in buffer pixels.
    /// The first commit of a buffer size has the whole buffer.
    pub damage: Vec<Rect>,
    /// The part of the surface that is the window, with no shadow, in logical pixels. `None` is the whole surface.
    pub geometry: Option<Rect>,
    /// The pixels of the whole buffer, row after row, with `width * 4` bytes in each row.
    pub pixels: Arc<Vec<u8>>,
}

/// A pointer image of a client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorImage {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
    /// The point of the image that the pointer is at, from the left edge.
    pub hotspot_x: i32,
    /// The point of the image that the pointer is at, from the top edge.
    pub hotspot_y: i32,
    /// The pixel format.
    pub format: Format,
    /// The pixels, row after row, with `width * 4` bytes in each row. The alpha is premultiplied.
    pub pixels: Arc<Vec<u8>>,
}

/// An event of the compositor for the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A new top-level window. It has no content until its first commit.
    ToplevelNew {
        /// The id of the window. The compositor uses each id one time only.
        id: u32,
        /// The process of the client, from the credentials of its socket (F045).
        pid: Option<u32>,
    },
    /// The title, the app id, or the parent of a top-level window changed.
    ToplevelChanged {
        /// The window.
        id: u32,
        /// The title.
        title: Option<String>,
        /// The app id, for example `org.gnome.Calculator`.
        app_id: Option<String>,
        /// The window that this window belongs to, for example for a dialog.
        parent: Option<u32>,
    },
    /// A top-level window closed.
    ToplevelClosed {
        /// The window.
        id: u32,
    },
    /// The client asks to move the window, for example after a press on its own title bar.
    MoveRequest {
        /// The window.
        id: u32,
    },
    /// A new popup, for example a menu, with its place relative to its parent surface in logical pixels.
    /// A new popup, or a new place or size of a popup. A subsurface comes as a popup too.
    PopupNew {
        /// The id of the popup. Popups and top-level windows share the ids.
        id: u32,
        /// The surface that the popup belongs to: a top-level window or a popup.
        parent: u32,
        /// The place and the size of the popup, relative to the window geometry of its parent.
        geometry: Rect,
        /// The popup is a subsurface: a part of its parent, for example a bubble of Chromium.
        /// A click outside it does not close it.
        subsurface: bool,
    },
    /// A popup closed.
    PopupClosed {
        /// The popup.
        id: u32,
    },
    /// A surface committed a new buffer.
    Commit(Commit),
    /// A surface removed its buffer: the window is not visible.
    Unmapped {
        /// The surface.
        id: u32,
    },
    /// The client selected the shape of the pointer: a CSS cursor name, for example `text` or `pointer`.
    Cursor(String),
    /// The client selected a pointer that it draws itself, for example each cursor of an X11 app (F084).
    CursorImage(CursorImage),
    /// An app set the clipboard: its text, or its PNG image if it has no text (F030).
    Clipboard(Clip),
}

/// A command of the shell for the compositor.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Asks the client to draw the window with this size in logical pixels. 0 lets the client select.
    Configure {
        /// The window.
        id: u32,
        /// The width.
        width: i32,
        /// The height.
        height: i32,
    },
    /// Gives the focus to a window, or to no window. The window gets the keyboard and the activated state.
    Activate {
        /// The window, or `None`.
        id: Option<u32>,
    },
    /// Asks the client to close the window. The client can ask the user first.
    Close {
        /// The window.
        id: u32,
    },
    /// Closes a popup, for example after a click outside it.
    PopupDismiss {
        /// The popup.
        id: u32,
    },
    /// The pointer is at (`x`, `y`) in logical pixels of the surface `id`, a window or a popup.
    PointerMotion {
        /// The surface.
        id: u32,
        /// The left distance.
        x: f64,
        /// The top distance.
        y: f64,
    },
    /// The pointer left each surface.
    PointerLeave,
    /// A pointer button: a Linux button code, for example `BTN_LEFT` (0x110).
    PointerButton {
        /// The button code.
        button: u32,
        /// Pressed or released.
        pressed: bool,
    },
    /// A scroll of the wheel in logical pixels.
    PointerAxis {
        /// The horizontal distance.
        dx: f64,
        /// The vertical distance.
        dy: f64,
    },
    /// A key of the keyboard: a Linux key code, for example `KEY_A` (30), in the window `id`.
    /// If the window does not have the focus, it gets it first: a key goes where the user typed it.
    Key {
        /// The window, or `None` for the window with the focus.
        id: Option<u32>,
        /// The key code.
        code: u32,
        /// Pressed or released.
        pressed: bool,
    },
    /// Sets the clipboard of the apps, for example to the clipboard of the client computer.
    /// The compositor owns the selection until an app sets a new one.
    SetClipboard(Clip),
    /// The surface channels of the surface `id` changed (F048). A window with no channel gets no frame callbacks,
    /// and a window with video keeps the full rate.
    Viewers {
        /// The surface: a window, a popup, or a subsurface.
        id: u32,
        /// At least one surface channel shows the surface.
        watched: bool,
        /// A surface channel sends a video region of the surface.
        video: bool,
    },
}

/// The bytes of a selection that a reader thread got: the generation of the selection, the kind, and the content.
type ClipRead = (u64, ClipKind, Vec<u8>);

/// The data of a selection that the compositor offers to the Wayland apps.
#[derive(Clone, Debug)]
pub(crate) enum ClipData {
    /// The clipboard of the shell.
    Shell(Arc<Clip>),
    /// The selection of an X11 app (F084). The compositor reads it from Xwayland for each paste.
    X11,
}

/// The owner of the clipboard.
#[derive(Clone, Debug, Default)]
pub(crate) enum ClipOwner {
    /// No clipboard.
    #[default]
    None,
    /// The shell, with its content.
    Shell(Arc<Clip>),
    /// A Wayland app.
    Wayland,
    /// An X11 app.
    X11,
}

/// The side of a new selection of an app that the compositor reads for the shell.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ClipFrom {
    Wayland,
    X11,
}

/// The last pointer image of a cursor surface, for a pointer that comes back to the surface.
#[derive(Default)]
struct LastCursor(std::sync::Mutex<Option<CursorImage>>);

/// The largest side of a pointer image. Browsers do not show a larger cursor.
const MAX_CURSOR: i32 = 128;

/// Writes the clipboard of the shell to a pipe of an app, on a thread: a large content fills the pipe.
pub(crate) fn write_clip(clip: Arc<Clip>, fd: OwnedFd) {
    let spawned = std::thread::Builder::new()
        .name("farglass-clipboard".to_owned())
        .spawn(move || {
            if let Err(error) = std::fs::File::from(fd).write_all(clip.bytes()) {
                tracing::debug!(%error, "an app stopped the paste");
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "no thread for a paste");
    }
}

/// The data of each Wayland client.
#[derive(Debug, Default)]
struct ClientState {
    compositor: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

/// The id of a surface, kept in the data of the surface.
struct SurfaceId(u32);

/// The last buffer size of a surface, for the full damage of a new size.
#[derive(Default)]
struct LastSize(std::sync::Mutex<Option<(i32, i32)>>);

/// The state of the compositor.
pub(crate) struct State {
    display: DisplayHandle,
    compositor: CompositorState,
    shm: ShmState,
    xdg_shell: XdgShellState,
    _decoration: XdgDecorationState,
    _cursor_shape: CursorShapeManagerState,
    data_device: DataDeviceState,
    _outputs: OutputManagerState,
    seat_state: SeatState<Self>,
    seat: Seat<Self>,
    toplevels: HashMap<u32, ToplevelSurface>,
    popups: HashMap<u32, PopupSurface>,
    /// The X11 windows of Xwayland (F082).
    x11: x11::X11,
    /// The subsurfaces with their parent and their last place for the shell.
    subsurfaces: HashMap<u32, (WlSurface, u32, Option<Rect>)>,
    next_id: u32,
    /// The parent of each popup and subsurface, for the window of a frame callback.
    parents: HashMap<u32, u32>,
    /// The frame callbacks that wait, with their surface.
    frame_callbacks: Vec<(Option<u32>, WlCallback)>,
    /// The rate of the frame callbacks of each window (F048).
    pace: Pace,
    /// The surface under the pointer, for the full rate after a click or a scroll.
    pointer_on: Option<u32>,
    /// The window with the keyboard focus.
    focused: Option<u32>,
    /// The mime types of a new selection of an app, and the side of the app. The compositor reads it
    /// after the dispatch, because Smithay stores the selection after it calls the handler.
    selection: Option<(Vec<String>, ClipFrom)>,
    /// The owner of the clipboard, for a paste in an X11 app.
    clip_owner: ClipOwner,
    /// The surface of the pointer image that the client under the pointer selected.
    cursor_surface: Option<WlSurface>,
    /// The last pointer image that went to the shell.
    last_cursor: Option<CursorImage>,
    /// The count of selections. A read of an older selection does not go to the shell.
    clip_generation: u64,
    clip_reads: channel::Sender<ClipRead>,
    started: Instant,
    events: Box<dyn FnMut(Event) + Send>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("toplevels", &self.toplevels.len())
            .field("popups", &self.popups.len())
            .finish_non_exhaustive()
    }
}

/// Runs the compositor until `stop` is set. Gives the signal for the wakeup to `ready` first,
/// and with `x11` the number of the X11 display of Xwayland.
///
/// The loop checks `stop` after each dispatch. A stop through the loop signal alone can
/// come before the loop runs, and then the loop does not see it.
pub(crate) fn run(
    listener: UnixListener,
    commands: Channel<Command>,
    events: Box<dyn FnMut(Event) + Send>,
    x11: bool,
    stop: &std::sync::atomic::AtomicBool,
    ready: &std::sync::mpsc::Sender<Result<(LoopSignal, Option<u32>), String>>,
) {
    match setup(listener, commands, events) {
        Ok((mut event_loop, mut state)) => {
            let display = if x11 {
                x11::start(&mut state, &event_loop.handle())
                    .and_then(|number| x11::wait_ready(&mut event_loop, &mut state, number))
            } else {
                None
            };
            let _ = ready.send(Ok((event_loop.get_signal(), display)));
            while !stop.load(std::sync::atomic::Ordering::Acquire) {
                if let Err(error) = event_loop.dispatch(None, &mut state) {
                    tracing::error!(%error, "the Wayland compositor stopped");
                    break;
                }
                state.read_selection();
                let _ = state.display.flush_clients();
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
}

fn setup(
    listener: UnixListener,
    commands: Channel<Command>,
    events: Box<dyn FnMut(Event) + Send>,
) -> Result<(EventLoop<'static, State>, State), String> {
    let event_loop: EventLoop<State> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let display: Display<State> = Display::new().map_err(|e| e.to_string())?;
    let dh = display.handle();

    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "seat0");
    seat.add_keyboard(Default::default(), 400, 30)
        .map_err(|e| e.to_string())?;
    seat.add_pointer();

    // One output: the desktop of the shell. The shell gives its size later (F029).
    let output = Output::new(
        "farglass-0".to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "farglass".to_owned(),
            model: "browser".to_owned(),
        },
    );
    let mode = OutputMode {
        size: OUTPUT_SIZE.into(),
        refresh: 60_000,
    };
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Integer(1)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    let _global = output.create_global::<State>(&dh);

    let (clip_reads, clip_results) = channel::channel::<ClipRead>();
    let state = State {
        compositor: CompositorState::new::<State>(&dh),
        shm: ShmState::new::<State>(&dh, [wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888]),
        xdg_shell: XdgShellState::new::<State>(&dh),
        _decoration: XdgDecorationState::new::<State>(&dh),
        // GTK 4 selects its cursors by name through cursor-shape-v1.
        _cursor_shape: CursorShapeManagerState::new::<State>(&dh),
        // The clipboard (F030). GTK also needs a data device to start.
        data_device: DataDeviceState::new::<State>(&dh),
        _outputs: OutputManagerState::new_with_xdg_output::<State>(&dh),
        seat_state,
        seat,
        toplevels: HashMap::new(),
        popups: HashMap::new(),
        x11: x11::X11::new(&dh),
        subsurfaces: HashMap::new(),
        next_id: 0,
        parents: HashMap::new(),
        frame_callbacks: Vec::new(),
        pace: Pace::new(Pace::idle_fps_from_env(), FRAME_INTERVAL),
        pointer_on: None,
        focused: None,
        selection: None,
        clip_owner: ClipOwner::None,
        cursor_surface: None,
        last_cursor: None,
        clip_generation: 0,
        clip_reads,
        started: Instant::now(),
        events,
        display: dh,
    };

    let handle = event_loop.handle();
    handle
        .insert_source(
            Generic::new(listener, Interest::READ, Mode::Level),
            |_, listener, state| {
                while let Ok((stream, _)) = listener.accept() {
                    if let Err(error) = state
                        .display
                        .insert_client(stream, Arc::new(ClientState::default()))
                    {
                        tracing::warn!(%error, "a Wayland client did not connect");
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state| {
                // SAFETY: the display stays in this event source. The callback does not drop it.
                #[allow(unsafe_code)]
                let display = unsafe { display.get_mut() };
                display.dispatch_clients(state)?;
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(commands, |event, (), state| {
            if let channel::Event::Msg(command) = event {
                state.command(command);
            }
        })
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(clip_results, |event, (), state| {
            if let channel::Event::Msg((generation, kind, bytes)) = event
                && generation == state.clip_generation
            {
                state.emit(Event::Clipboard(clipboard::clip(kind, bytes)));
            }
        })
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(Timer::from_duration(FRAME_INTERVAL), |_, (), state| {
            state.send_frame_callbacks();
            TimeoutAction::ToDuration(FRAME_INTERVAL)
        })
        .map_err(|e| e.to_string())?;
    Ok((event_loop, state))
}

impl State {
    fn emit(&mut self, event: Event) {
        (self.events)(event);
    }

    fn new_id(&mut self, surface: &WlSurface) -> u32 {
        let id = self.alloc_id();
        Self::tag(surface, id);
        id
    }

    /// Returns an id that no surface had before.
    fn alloc_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Sets the id of a surface. 0 removes it.
    fn tag(surface: &WlSurface, id: u32) {
        with_states(surface, |states| {
            states
                .data_map
                .insert_if_missing_threadsafe(|| std::sync::Mutex::new(SurfaceId(0)));
            if let Some(slot) = states.data_map.get::<std::sync::Mutex<SurfaceId>>() {
                slot.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0 = id;
            }
        });
    }

    fn id_of(surface: &WlSurface) -> Option<u32> {
        with_states(surface, |states| {
            states
                .data_map
                .get::<std::sync::Mutex<SurfaceId>>()
                .map(|slot| {
                    slot.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0
                })
        })
        .filter(|id| *id != 0)
    }

    /// Gives the focus to a window, or to no window: the activated state and the keyboard.
    fn activate(&mut self, id: Option<u32>) {
        self.focused = id;
        for (each, toplevel) in &self.toplevels {
            let active = Some(*each) == id;
            let changed = toplevel.with_pending_state(|state| {
                if active {
                    state.states.set(xdg_toplevel::State::Activated)
                } else {
                    state.states.unset(xdg_toplevel::State::Activated)
                }
            });
            if changed && toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure();
            }
        }
        for (each, window) in &self.x11.windows {
            if let Err(error) = window.set_activated(Some(*each) == id) {
                tracing::debug!(%error, "an X11 window did not get its activated state");
            }
        }
        let focus = id.and_then(|id| {
            self.toplevels
                .get(&id)
                .map(|toplevel| Focus::Wayland(toplevel.wl_surface().clone()))
                .or_else(|| self.x11.windows.get(&id).cloned().map(Focus::X11))
        });
        // An X11 app must see its window above the other X11 windows, for example for its menus.
        if let (Some(Focus::X11(window)), Some(wm)) = (&focus, self.x11.wm.as_mut())
            && let Err(error) = wm.raise_window(window)
        {
            tracing::debug!(%error, "an X11 window did not go to the top");
        }
        let to_x11 = matches!(focus, Some(Focus::X11(_)));
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, focus, SERIAL_COUNTER.next_serial());
        }
        // The X11 focus goes on the X11 connection, the keys go on the Wayland connection.
        // The compositor waits until Xwayland has the focus: then a key right after a click is not lost.
        if to_x11 {
            self.x11.sync();
        }
    }

    /// Reads the new selection of an app on a thread of its own: a slow app does not stop the compositor.
    fn read_selection(&mut self) {
        let Some((offered, from)) = self.selection.take() else {
            return;
        };
        let Some((mime, kind)) = clipboard::pick(&offered) else {
            tracing::debug!(
                ?offered,
                "the clipboard of an app has no text and no PNG image"
            );
            return;
        };
        let (reader, writer) = match std::io::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                tracing::warn!(%error, "no pipe for the clipboard of an app");
                return;
            }
        };
        let requested = match from {
            ClipFrom::Wayland => {
                request_data_device_client_selection(&self.seat, mime, OwnedFd::from(writer))
                    .map_err(|error| error.to_string())
            }
            ClipFrom::X11 => self.x11.read_selection(mime, OwnedFd::from(writer)),
        };
        if let Err(error) = requested {
            tracing::debug!(%error, "the clipboard of an app is not available");
            return;
        }
        let generation = self.clip_generation;
        let results = self.clip_reads.clone();
        let spawned = std::thread::Builder::new()
            .name("farglass-clipboard".to_owned())
            .spawn(move || {
                // One byte more than the limit shows a content that is too large.
                let limit = u64::try_from(clipboard::MAX_CLIP).unwrap_or(u64::MAX) + 1;
                let mut bytes = Vec::new();
                match reader.take(limit).read_to_end(&mut bytes) {
                    Ok(_) if bytes.len() <= clipboard::MAX_CLIP => {
                        let _ = results.send((generation, kind, bytes));
                    }
                    Ok(_) => tracing::info!(
                        "the clipboard of an app is larger than 16 MiB, thus the shell does not get it"
                    ),
                    Err(error) => tracing::debug!(%error, "the clipboard of an app did not arrive"),
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "no thread for the clipboard of an app");
        }
    }

    /// The time in milliseconds for input events.
    fn now(&self) -> u32 {
        u32::try_from(self.started.elapsed().as_millis() % u128::from(u32::MAX)).unwrap_or(0)
    }

    /// Returns the window of the surface `id`: the top of its chain of parents.
    fn root(&self, id: u32) -> u32 {
        let mut root = id;
        // A chain is short: the limit only stops a loop.
        for _ in 0..32 {
            match self.parents.get(&root) {
                Some(&parent) => root = parent,
                None => break,
            }
        }
        root
    }

    /// Gives the full rate to the window of the surface `id` (F048).
    fn touch(&mut self, id: Option<u32>) {
        if let Some(id) = id {
            let root = self.root(id);
            self.pace.touch(root, Instant::now());
        }
    }

    /// Sends the frame callbacks that are due by the pace of their window, and keeps the others.
    fn send_frame_callbacks(&mut self) {
        if self.frame_callbacks.is_empty() {
            return;
        }
        let now = Instant::now();
        let time = self.now();
        let mut decisions: HashMap<u32, Decision> = HashMap::new();
        let callbacks = std::mem::take(&mut self.frame_callbacks);
        for (id, callback) in callbacks {
            if !callback.is_alive() {
                continue;
            }
            // A surface with no id, for example with no role yet, gets its callback at once.
            let decision = id.map_or(Decision::Send, |id| {
                let root = self.root(id);
                *decisions
                    .entry(id)
                    .or_insert_with(|| self.pace.decide(id, root, now))
            });
            match decision {
                Decision::Send => callback.done(time),
                Decision::Hold => self.frame_callbacks.push((id, callback)),
            }
        }
    }

    fn command(&mut self, command: Command) {
        tracing::trace!(?command, "a command of the shell");
        match command {
            Command::Viewers { id, watched, video } => {
                tracing::debug!(id, watched, video, "the channels of a surface changed");
                self.pace.set_viewers(id, watched, video);
            }
            Command::Configure { id, width, height } => {
                self.touch(Some(id));
                if let Some(toplevel) = self.toplevels.get(&id) {
                    toplevel.with_pending_state(|state| {
                        state.size = (width > 0 && height > 0).then(|| (width, height).into());
                    });
                    if toplevel.is_initial_configure_sent() {
                        toplevel.send_pending_configure();
                    }
                } else if let Some(window) = self.x11.windows.get(&id)
                    && width > 0
                    && height > 0
                    && let Err(error) =
                        window.configure(Rectangle::new((0, 0).into(), (width, height).into()))
                {
                    tracing::debug!(%error, "an X11 window did not get its size");
                }
            }
            Command::Activate { id } => {
                self.touch(id);
                self.activate(id);
            }
            Command::SetClipboard(clip) => {
                // A read of the selection before this one must not replace it in the shell.
                self.clip_generation += 1;
                self.selection = None;
                let types = clip.mime_types();
                let clip = Arc::new(clip);
                self.clip_owner = ClipOwner::Shell(clip.clone());
                // The X11 apps get the clipboard of the shell too (F084).
                self.x11.announce(Some(types.clone()));
                set_data_device_selection(&self.display, &self.seat, types, ClipData::Shell(clip));
            }
            Command::Close { id } => {
                if let Some(toplevel) = self.toplevels.get(&id) {
                    toplevel.send_close();
                } else if let Some(window) = self.x11.windows.get(&id)
                    && let Err(error) = window.close()
                {
                    tracing::debug!(%error, "an X11 window did not get the close request");
                }
            }
            Command::PopupDismiss { id } => {
                self.touch(Some(id));
                if let Some(popup) = self.popups.get(&id) {
                    popup.send_popup_done();
                } else if self.x11.is_popup(id) {
                    self.x11_dismiss();
                }
            }
            Command::PointerMotion { id, x, y } => {
                self.pointer_on = Some(id);
                self.touch(Some(id));
                let surface = self
                    .toplevels
                    .get(&id)
                    .map(|t| t.wl_surface().clone())
                    .or_else(|| self.popups.get(&id).map(|p| p.wl_surface().clone()))
                    .or_else(|| self.subsurfaces.get(&id).map(|(s, _, _)| s.clone()))
                    .or_else(|| self.x11.surface(id));
                if let (Some(pointer), Some(surface)) = (self.seat.get_pointer(), surface) {
                    // The location is local to the surface: the focus has the origin (0, 0).
                    let event = MotionEvent {
                        location: (x, y).into(),
                        serial: SERIAL_COUNTER.next_serial(),
                        time: self.now(),
                    };
                    pointer.motion(self, Some((surface, (0.0, 0.0).into())), &event);
                    pointer.frame(self);
                }
            }
            Command::PointerLeave => {
                let left = self.pointer_on.take();
                self.touch(left);
                if let Some(pointer) = self.seat.get_pointer() {
                    let event = MotionEvent {
                        location: (-1.0, -1.0).into(),
                        serial: SERIAL_COUNTER.next_serial(),
                        time: self.now(),
                    };
                    pointer.motion(self, None, &event);
                    pointer.frame(self);
                }
            }
            Command::PointerButton { button, pressed } => {
                self.touch(self.pointer_on);
                if let Some(pointer) = self.seat.get_pointer() {
                    let state = if pressed {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    };
                    let event = ButtonEvent {
                        button,
                        state,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: self.now(),
                    };
                    pointer.button(self, &event);
                    pointer.frame(self);
                }
            }
            Command::PointerAxis { dx, dy } => {
                self.touch(self.pointer_on);
                if let Some(pointer) = self.seat.get_pointer() {
                    let mut frame = AxisFrame::new(self.now()).source(AxisSource::Wheel);
                    if dx != 0.0 {
                        frame = frame.value(Axis::Horizontal, dx);
                    }
                    if dy != 0.0 {
                        frame = frame.value(Axis::Vertical, dy);
                    }
                    pointer.axis(self, frame);
                    pointer.frame(self);
                }
            }
            Command::Key { id, code, pressed } => {
                self.touch(id.or(self.focused));
                if let Some(id) = id
                    && self.focused != Some(id)
                    && (self.toplevels.contains_key(&id) || self.x11.windows.contains_key(&id))
                {
                    self.activate(Some(id));
                }
                if let Some(keyboard) = self.seat.get_keyboard() {
                    let state = if pressed {
                        KeyState::Pressed
                    } else {
                        KeyState::Released
                    };
                    // xkb key codes are the Linux key codes plus 8.
                    let time = self.now();
                    keyboard.input::<(), _>(
                        self,
                        Keycode::new(code + 8),
                        state,
                        SERIAL_COUNTER.next_serial(),
                        time,
                        |_, _, _| FilterResult::Forward,
                    );
                }
            }
        }
    }

    /// Copies the buffer of a commit and releases the buffer.
    fn take_buffer(
        &mut self,
        id: u32,
        surface: &WlSurface,
        buffer: &WlBuffer,
        damage: Vec<Damage>,
        scale: i32,
    ) {
        let copied = with_buffer_contents(buffer, |ptr, len, data| {
            let (width, height, stride, offset) =
                (data.width, data.height, data.stride, data.offset);
            let format = match data.format {
                wl_shm::Format::Argb8888 => Format::Argb8888,
                _ => Format::Xrgb8888,
            };
            let row = usize::try_from(width).ok()? * 4;
            let rows = usize::try_from(height).ok()?;
            let stride = usize::try_from(stride).ok()?;
            let offset = usize::try_from(offset).ok()?;
            if stride < row || offset + stride * rows.saturating_sub(1) + row > len {
                return None;
            }
            // SAFETY: Smithay maps the pool with `len` bytes at `ptr` for the duration of this callback,
            // and the check above keeps each read in that range.
            #[allow(unsafe_code)]
            let pool = unsafe { std::slice::from_raw_parts(ptr, len) };
            let mut pixels = Vec::with_capacity(row * rows);
            for y in 0..rows {
                let start = offset + y * stride;
                pixels.extend_from_slice(&pool[start..start + row]);
            }
            Some((width, height, format, pixels))
        });
        buffer.release();
        let Ok(Some((width, height, format, pixels))) = copied else {
            tracing::debug!(id, "a buffer that is not shm or has a bad size");
            return;
        };
        let full = Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        let new_size = with_states(surface, |states| {
            states
                .data_map
                .insert_if_missing_threadsafe(LastSize::default);
            let last = states.data_map.get::<LastSize>().map(|last| {
                let mut last = last
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let changed = *last != Some((width, height));
                *last = Some((width, height));
                changed
            });
            last.unwrap_or(true)
        });
        let damage = if new_size {
            vec![full]
        } else {
            damage
                .into_iter()
                .filter_map(|damage| {
                    let rect = match damage {
                        Damage::Buffer(r) => Rect {
                            x: r.loc.x,
                            y: r.loc.y,
                            width: r.size.w,
                            height: r.size.h,
                        },
                        Damage::Surface(r) => Rect {
                            x: r.loc.x.saturating_mul(scale),
                            y: r.loc.y.saturating_mul(scale),
                            width: r.size.w.saturating_mul(scale),
                            height: r.size.h.saturating_mul(scale),
                        },
                    };
                    clip(rect, full)
                })
                .collect()
        };
        let geometry = with_states(surface, |states| {
            states
                .cached_state
                .get::<SurfaceCachedState>()
                .current()
                .geometry
        })
        .map(|r| Rect {
            x: r.loc.x,
            y: r.loc.y,
            width: r.size.w,
            height: r.size.h,
        });
        self.emit(Event::Commit(Commit {
            id,
            width,
            height,
            scale,
            format,
            damage,
            geometry,
            pixels: Arc::new(pixels),
        }));
    }

    fn toplevel_changed(&mut self, surface: &ToplevelSurface) {
        let Some(id) = Self::id_of(surface.wl_surface()) else {
            return;
        };
        let (title, app_id, parent) = with_states(surface.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .map_or((None, None, None), |data| {
                    let data = data
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    (data.title.clone(), data.app_id.clone(), data.parent.clone())
                })
        });
        let parent = parent.as_ref().and_then(Self::id_of);
        self.emit(Event::ToplevelChanged {
            id,
            title,
            app_id,
            parent,
        });
    }
}

/// Returns the part of `rect` in `bounds`, or `None` if nothing is in it.
fn clip(rect: Rect, bounds: Rect) -> Option<Rect> {
    let x = rect.x.max(bounds.x);
    let y = rect.y.max(bounds.y);
    let right = (rect.x.saturating_add(rect.width)).min(bounds.x + bounds.width);
    let bottom = (rect.y.saturating_add(rect.height)).min(bounds.y + bounds.height);
    (right > x && bottom > y).then_some(Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        if let Some(xwayland) = client.get_data::<XWaylandClientData>() {
            return &xwayland.compositor_state;
        }
        &client
            .get_data::<ClientState>()
            .expect("each client has a ClientState")
            .compositor
    }

    fn new_subsurface(&mut self, surface: &WlSurface, parent: &WlSurface) {
        let Some(parent) = Self::id_of(parent) else {
            return;
        };
        let id = self.new_id(surface);
        tracing::debug!(id, parent, "a new subsurface");
        self.subsurfaces.insert(id, (surface.clone(), parent, None));
        self.parents.insert(id, parent);
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        self.x11.forget(surface);
        if let Some(id) = Self::id_of(surface)
            && self.subsurfaces.remove(&id).is_some()
        {
            self.parents.remove(&id);
            self.pace.remove(id);
            self.emit(Event::PopupClosed { id });
        }
    }

    fn commit(&mut self, surface: &WlSurface) {
        self.commit_surface(surface);
        // A synchronized subsurface applies its state with the commit of its parent.
        let mut children = smithay::wayland::compositor::get_children(surface);
        while let Some(child) = children.pop() {
            self.commit_surface(&child);
            children.extend(smithay::wayland::compositor::get_children(&child));
        }
    }
}

impl State {
    /// Gives the shell the place and the size of a subsurface when they change. `buffer` is the size
    /// of a new buffer in buffer pixels; with none, the size of the last buffer.
    fn place_subsurface(&mut self, id: u32, surface: &WlSurface, buffer: Option<(i32, i32)>) {
        let Some((_, parent, last)) = self.subsurfaces.get(&id) else {
            return;
        };
        let parent_id = *parent;
        let (location, size) = with_states(surface, |states| {
            let location = states
                .cached_state
                .get::<smithay::wayland::compositor::SubsurfaceCachedState>()
                .current()
                .location;
            let size = states.data_map.get::<LastSize>().and_then(|last| {
                *last
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
            });
            (location, buffer.or(size))
        });
        let scale = with_states(surface, |states| {
            states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .buffer_scale
                .max(1)
        });
        let Some((width, height)) = size else {
            return;
        };
        // The place of a popup is relative to the window geometry of its parent; a subsurface is relative to the surface of its parent.
        let parent_offset = self
            .toplevels
            .get(&parent_id)
            .map(|t| t.wl_surface().clone())
            .or_else(|| self.popups.get(&parent_id).map(|p| p.wl_surface().clone()))
            .and_then(|parent| {
                with_states(&parent, |states| {
                    states
                        .cached_state
                        .get::<SurfaceCachedState>()
                        .current()
                        .geometry
                })
            })
            .map_or((0, 0), |g| (g.loc.x, g.loc.y));
        let place = Rect {
            x: location.x - parent_offset.0,
            y: location.y - parent_offset.1,
            width: width / scale,
            height: height / scale,
        };
        if *last == Some(place) {
            return;
        }
        if let Some(entry) = self.subsurfaces.get_mut(&id) {
            entry.2 = Some(place);
        }
        self.emit(Event::PopupNew {
            id,
            parent: parent_id,
            geometry: place,
            subsurface: true,
        });
    }

    fn commit_surface(&mut self, surface: &WlSurface) {
        // The first commit of an xdg surface asks for its first configure.
        if let Some(toplevel) = self.toplevels.values().find(|t| t.wl_surface() == surface)
            && !toplevel.is_initial_configure_sent()
        {
            toplevel.send_configure();
        }
        if let Some(popup) = self.popups.values().find(|p| p.wl_surface() == surface)
            && !popup.is_initial_configure_sent()
        {
            let _ = popup.send_configure();
        }
        let (buffer, damage, scale, callbacks) = with_states(surface, |states| {
            let mut cache = states.cached_state.get::<SurfaceAttributes>();
            let attributes = cache.current();
            (
                attributes.buffer.take(),
                std::mem::take(&mut attributes.damage),
                attributes.buffer_scale.max(1),
                std::mem::take(&mut attributes.frame_callbacks),
            )
        });
        let id = Self::id_of(surface);
        self.frame_callbacks
            .extend(callbacks.into_iter().map(|callback| (id, callback)));
        let Some(id) = id else {
            // Xwayland can commit the first frame of a window before the window manager knows the window.
            // The frame waits for the window: an app that draws only one time must not stay blank.
            // A surface with no role yet, or a subsurface. Its buffer goes back at once.
            if let Some(BufferAssignment::NewBuffer(buffer)) = buffer {
                let role = smithay::wayland::compositor::get_role(surface);
                if role == Some(smithay::wayland::seat::CURSOR_IMAGE_ROLE) {
                    self.cursor_commit(surface, &buffer);
                    buffer.release();
                    return;
                }
                if role == Some(smithay::wayland::xwayland_shell::XWAYLAND_SHELL_ROLE) {
                    self.x11.keep(surface, buffer, scale);
                    return;
                }
                tracing::debug!(
                    subsurface = smithay::wayland::compositor::is_sync_subsurface(surface)
                        || smithay::wayland::compositor::get_parent(surface).is_some(),
                    "a buffer of a surface with no window"
                );
                buffer.release();
            }
            return;
        };
        match buffer {
            Some(BufferAssignment::NewBuffer(buffer)) => {
                // The shell gets the place of a subsurface before its pixels.
                if self.subsurfaces.contains_key(&id) {
                    let size =
                        with_buffer_contents(&buffer, |_, _, data| (data.width, data.height)).ok();
                    self.place_subsurface(id, surface, size);
                }
                self.take_buffer(id, surface, &buffer, damage, scale);
            }
            Some(BufferAssignment::Removed) => {
                with_states(surface, |states| {
                    if let Some(last) = states.data_map.get::<LastSize>() {
                        *last
                            .0
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                    }
                });
                self.emit(Event::Unmapped { id });
            }
            None => {
                if self.subsurfaces.contains_key(&id) {
                    self.place_subsurface(id, surface, None);
                }
            }
        }
    }
}

impl State {
    /// Reads the pointer image of a commit of a cursor surface, and gives it to the shell if the surface is the pointer now.
    fn cursor_commit(&mut self, surface: &WlSurface, buffer: &WlBuffer) {
        let copied = with_buffer_contents(buffer, |ptr, len, data| {
            let (width, height, stride, offset) =
                (data.width, data.height, data.stride, data.offset);
            if width <= 0 || height <= 0 || width > MAX_CURSOR || height > MAX_CURSOR {
                return None;
            }
            let format = match data.format {
                wl_shm::Format::Argb8888 => Format::Argb8888,
                _ => Format::Xrgb8888,
            };
            let row = usize::try_from(width).ok()? * 4;
            let rows = usize::try_from(height).ok()?;
            let stride = usize::try_from(stride).ok()?;
            let offset = usize::try_from(offset).ok()?;
            if stride < row || offset + stride * rows.saturating_sub(1) + row > len {
                return None;
            }
            // SAFETY: Smithay maps the pool with `len` bytes at `ptr` for the duration of this callback,
            // and the check above keeps each read in that range.
            #[allow(unsafe_code)]
            let pool = unsafe { std::slice::from_raw_parts(ptr, len) };
            let mut pixels = Vec::with_capacity(row * rows);
            for y in 0..rows {
                let start = offset + y * stride;
                pixels.extend_from_slice(&pool[start..start + row]);
            }
            Some((width, height, format, pixels))
        });
        let Ok(Some((width, height, format, pixels))) = copied else {
            tracing::debug!("a pointer image that is not shm, or larger than 128 pixels");
            return;
        };
        let (hotspot_x, hotspot_y) = with_states(surface, |states| {
            states
                .data_map
                .get::<CursorImageSurfaceData>()
                .and_then(|data| {
                    data.lock()
                        .ok()
                        .map(|data| (data.hotspot.x, data.hotspot.y))
                })
                .unwrap_or((0, 0))
        });
        let image = CursorImage {
            width: width.unsigned_abs(),
            height: height.unsigned_abs(),
            hotspot_x,
            hotspot_y,
            format,
            pixels: Arc::new(pixels),
        };
        with_states(surface, |states| {
            states
                .data_map
                .insert_if_missing_threadsafe(LastCursor::default);
            if let Some(last) = states.data_map.get::<LastCursor>()
                && let Ok(mut last) = last.0.lock()
            {
                *last = Some(image.clone());
            }
        });
        if self.cursor_surface.as_ref() == Some(surface) {
            self.emit_cursor(image);
        }
    }

    /// Gives a pointer image to the shell, if it is not the image that the shell has.
    fn emit_cursor(&mut self, image: CursorImage) {
        if self.last_cursor.as_ref() == Some(&image) {
            return;
        }
        tracing::debug!(
            width = image.width,
            height = image.height,
            "the client selected a pointer image"
        );
        self.last_cursor = Some(image.clone());
        self.emit(Event::CursorImage(image));
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let id = self.new_id(surface.wl_surface());
        surface.with_pending_state(|state| {
            state.decoration_mode = Some(zxdg_toplevel_decoration_v1::Mode::ServerSide);
        });
        let pid = surface
            .wl_surface()
            .client()
            .and_then(|client| client.get_credentials(&self.display).ok())
            .and_then(|credentials| u32::try_from(credentials.pid).ok());
        self.toplevels.insert(id, surface);
        self.touch(Some(id));
        self.emit(Event::ToplevelNew { id, pid });
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(id) = Self::id_of(surface.wl_surface()) {
            self.toplevels.remove(&id);
            self.pace.remove(id);
            self.emit(Event::ToplevelClosed { id });
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        self.toplevel_changed(&surface);
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        self.toplevel_changed(&surface);
    }

    fn parent_changed(&mut self, surface: ToplevelSurface) {
        self.toplevel_changed(&surface);
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: WlSeat, _serial: Serial) {
        if let Some(id) = Self::id_of(surface.wl_surface()) {
            self.emit(Event::MoveRequest { id });
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        let geometry = positioner.get_geometry();
        surface.with_pending_state(|state| state.geometry = geometry);
        let parent = surface.get_parent_surface().as_ref().and_then(Self::id_of);
        let id = self.new_id(surface.wl_surface());
        tracing::debug!(id, ?parent, ?geometry, "a new popup");
        self.popups.insert(id, surface);
        let Some(parent) = parent else {
            return;
        };
        self.parents.insert(id, parent);
        self.touch(Some(id));
        self.emit(Event::PopupNew {
            id,
            parent,
            geometry: Rect {
                x: geometry.loc.x,
                y: geometry.loc.y,
                width: geometry.size.w,
                height: geometry.size.h,
            },
            subsurface: false,
        });
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        let geometry = positioner.get_geometry();
        tracing::debug!(id = ?Self::id_of(surface.wl_surface()), ?geometry, "a popup moves");
        surface.with_pending_state(|state| state.geometry = geometry);
        surface.send_repositioned(token);
        let _ = surface.send_pending_configure();
        // The shell gets the new place: a popup that grows or moves, for example a list of suggestions.
        if let (Some(id), Some(parent)) = (
            Self::id_of(surface.wl_surface()),
            surface.get_parent_surface().as_ref().and_then(Self::id_of),
        ) {
            self.emit(Event::PopupNew {
                id,
                parent,
                geometry: Rect {
                    x: geometry.loc.x,
                    y: geometry.loc.y,
                    width: geometry.size.w,
                    height: geometry.size.h,
                },
                subsurface: false,
            });
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {
        // The shell closes a popup after a click outside it (PopupDismiss). Input grabs come with F029.
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        if let Some(id) = Self::id_of(surface.wl_surface()) {
            self.popups.remove(&id);
            self.touch(Some(id));
            self.parents.remove(&id);
            self.pace.remove(id);
            self.emit(Event::PopupClosed { id });
        }
    }
}

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(zxdg_toplevel_decoration_v1::Mode::ServerSide);
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    fn request_mode(
        &mut self,
        toplevel: ToplevelSurface,
        _mode: zxdg_toplevel_decoration_v1::Mode,
    ) {
        // The shell draws the frame of each window, thus the mode is always server-side.
        self.new_decoration(toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.new_decoration(toplevel);
    }
}

impl SeatHandler for State {
    type KeyboardFocus = Focus;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        let name = match image {
            CursorImageStatus::Hidden => "none".to_owned(),
            CursorImageStatus::Named(icon) => icon.name().to_owned(),
            // A cursor that the client draws on a surface, for example each cursor of an X11 app (F084).
            CursorImageStatus::Surface(surface) => {
                self.cursor_surface = Some(surface.clone());
                let last = with_states(&surface, |states| {
                    states
                        .data_map
                        .get::<LastCursor>()
                        .and_then(|last| last.0.lock().ok().and_then(|image| image.clone()))
                });
                // A surface with no image yet sends it with its next commit.
                if let Some(image) = last {
                    self.emit_cursor(image);
                }
                return;
            }
        };
        self.cursor_surface = None;
        self.last_cursor = None;
        tracing::debug!(cursor = name, "the client selected a cursor");
        self.emit(Event::Cursor(name));
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&Focus>) {
        // The app with the keyboard focus gets the selection, and only it can set a new one.
        let client = focused
            .and_then(|focus| focus.wl_surface())
            .and_then(|surface| surface.client());
        set_data_device_focus(&self.display, seat, client);
    }
}

impl OutputHandler for State {}

impl TabletSeatHandler for State {}

impl SelectionHandler for State {
    type SelectionUserData = ClipData;

    fn new_selection(
        &mut self,
        ty: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        if ty == SelectionTarget::Clipboard {
            self.clip_generation += 1;
            let types = source.map(|source| source.mime_types());
            self.clip_owner = if types.is_some() {
                ClipOwner::Wayland
            } else {
                ClipOwner::None
            };
            // The X11 apps get the clipboard of a Wayland app too (F084).
            self.x11.announce(types.clone());
            self.selection = types.map(|types| (types, ClipFrom::Wayland));
        }
    }

    fn send_selection(
        &mut self,
        _ty: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        user_data: &Self::SelectionUserData,
    ) {
        match user_data {
            ClipData::Shell(clip) => {
                tracing::debug!(mime_type, "an app pastes the clipboard of the shell");
                write_clip(clip.clone(), fd);
            }
            ClipData::X11 => {
                tracing::debug!(
                    mime_type,
                    "a Wayland app pastes the clipboard of an X11 app"
                );
                if let Err(error) = self.x11.read_selection(mime_type, fd) {
                    tracing::debug!(%error, "the clipboard of an X11 app is not available");
                }
            }
        }
    }
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device
    }
}

impl ClientDndGrabHandler for State {}
impl ServerDndGrabHandler for State {}

delegate_compositor!(State);
delegate_shm!(State);
delegate_xdg_shell!(State);
delegate_xdg_decoration!(State);
delegate_seat!(State);
delegate_output!(State);
delegate_cursor_shape!(State);
delegate_data_device!(State);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_keeps_the_part_in_the_buffer() {
        let full = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };
        assert_eq!(
            clip(
                Rect {
                    x: 90,
                    y: 40,
                    width: 20,
                    height: 20
                },
                full
            ),
            Some(Rect {
                x: 90,
                y: 40,
                width: 10,
                height: 10
            })
        );
        assert_eq!(
            clip(
                Rect {
                    x: 200,
                    y: 0,
                    width: 5,
                    height: 5
                },
                full
            ),
            None
        );
        assert_eq!(
            clip(
                Rect {
                    x: -5,
                    y: -5,
                    width: 10,
                    height: 10
                },
                full
            ),
            Some(Rect {
                x: 0,
                y: 0,
                width: 5,
                height: 5
            })
        );
    }
}
