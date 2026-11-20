//! The state of the compositor and its Wayland handlers.

use std::collections::HashMap;
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
use smithay::input::pointer::{AxisFrame, ButtonEvent, CursorImageStatus, MotionEvent};
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
use smithay::reexports::wayland_server::{Client, Display, DisplayHandle};
use smithay::utils::{SERIAL_COUNTER, Serial, Transform};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    BufferAssignment, CompositorClientState, CompositorHandler, CompositorState, Damage,
    SurfaceAttributes, with_states,
};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::output::{OutputHandler, OutputManagerState};
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::shell::xdg::decoration::{XdgDecorationHandler, XdgDecorationState};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, SurfaceCachedState, ToplevelSurface, XdgShellHandler,
    XdgShellState, XdgToplevelSurfaceData,
};
use smithay::wayland::shm::{ShmHandler, ShmState, with_buffer_contents};
use smithay::wayland::tablet_manager::TabletSeatHandler;

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

/// An event of the compositor for the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A new top-level window. It has no content until its first commit.
    ToplevelNew {
        /// The id of the window. The compositor uses each id one time only.
        id: u32,
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
    PopupNew {
        /// The id of the popup. Popups and top-level windows share the ids.
        id: u32,
        /// The surface that the popup belongs to: a top-level window or a popup.
        parent: u32,
        /// The place and the size of the popup.
        geometry: Rect,
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
        /// The window.
        id: u32,
        /// The key code.
        code: u32,
        /// Pressed or released.
        pressed: bool,
    },
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
    next_id: u32,
    frame_callbacks: Vec<WlCallback>,
    /// The window with the keyboard focus.
    focused: Option<u32>,
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

/// Runs the compositor until `stop` is set. Gives the signal for the wakeup to `ready` first.
///
/// The loop checks `stop` after each dispatch. A stop through the loop signal alone can
/// come before the loop runs, and then the loop does not see it.
pub(crate) fn run(
    listener: UnixListener,
    commands: Channel<Command>,
    events: Box<dyn FnMut(Event) + Send>,
    stop: &std::sync::atomic::AtomicBool,
    ready: &std::sync::mpsc::Sender<Result<LoopSignal, String>>,
) {
    match setup(listener, commands, events) {
        Ok((mut event_loop, mut state)) => {
            let _ = ready.send(Ok(event_loop.get_signal()));
            while !stop.load(std::sync::atomic::Ordering::Acquire) {
                if let Err(error) = event_loop.dispatch(None, &mut state) {
                    tracing::error!(%error, "the Wayland compositor stopped");
                    break;
                }
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

    let state = State {
        compositor: CompositorState::new::<State>(&dh),
        shm: ShmState::new::<State>(&dh, [wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888]),
        xdg_shell: XdgShellState::new::<State>(&dh),
        _decoration: XdgDecorationState::new::<State>(&dh),
        // GTK 4 selects its cursors by name through cursor-shape-v1.
        _cursor_shape: CursorShapeManagerState::new::<State>(&dh),
        // GTK needs a data device. The clipboard to the shell comes with F030.
        data_device: DataDeviceState::new::<State>(&dh),
        _outputs: OutputManagerState::new_with_xdg_output::<State>(&dh),
        seat_state,
        seat,
        toplevels: HashMap::new(),
        popups: HashMap::new(),
        next_id: 0,
        frame_callbacks: Vec::new(),
        focused: None,
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
        self.next_id += 1;
        let id = self.next_id;
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
        id
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
        let focus = id
            .and_then(|id| self.toplevels.get(&id))
            .map(|toplevel| toplevel.wl_surface().clone());
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, focus, SERIAL_COUNTER.next_serial());
        }
    }

    /// The time in milliseconds for input events.
    fn now(&self) -> u32 {
        u32::try_from(self.started.elapsed().as_millis() % u128::from(u32::MAX)).unwrap_or(0)
    }

    fn send_frame_callbacks(&mut self) {
        if self.frame_callbacks.is_empty() {
            return;
        }
        let time =
            u32::try_from(self.started.elapsed().as_millis() % u128::from(u32::MAX)).unwrap_or(0);
        for callback in self.frame_callbacks.drain(..) {
            callback.done(time);
        }
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Configure { id, width, height } => {
                if let Some(toplevel) = self.toplevels.get(&id) {
                    toplevel.with_pending_state(|state| {
                        state.size = (width > 0 && height > 0).then(|| (width, height).into());
                    });
                    if toplevel.is_initial_configure_sent() {
                        toplevel.send_pending_configure();
                    }
                }
            }
            Command::Activate { id } => self.activate(id),
            Command::Close { id } => {
                if let Some(toplevel) = self.toplevels.get(&id) {
                    toplevel.send_close();
                }
            }
            Command::PopupDismiss { id } => {
                if let Some(popup) = self.popups.get(&id) {
                    popup.send_popup_done();
                }
            }
            Command::PointerMotion { id, x, y } => {
                let surface = self
                    .toplevels
                    .get(&id)
                    .map(|t| t.wl_surface().clone())
                    .or_else(|| self.popups.get(&id).map(|p| p.wl_surface().clone()));
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
                if self.focused != Some(id) && self.toplevels.contains_key(&id) {
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
        &client
            .get_data::<ClientState>()
            .expect("each client has a ClientState")
            .compositor
    }

    fn commit(&mut self, surface: &WlSurface) {
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
        self.frame_callbacks.extend(callbacks);
        let Some(id) = Self::id_of(surface) else {
            // A surface with no role yet, or a subsurface. Its buffer goes back at once.
            if let Some(BufferAssignment::NewBuffer(buffer)) = buffer {
                buffer.release();
            }
            return;
        };
        match buffer {
            Some(BufferAssignment::NewBuffer(buffer)) => {
                self.take_buffer(id, surface, &buffer, damage, scale)
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
            None => {}
        }
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
        self.toplevels.insert(id, surface);
        self.emit(Event::ToplevelNew { id });
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(id) = Self::id_of(surface.wl_surface()) {
            self.toplevels.remove(&id);
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
        self.popups.insert(id, surface);
        let Some(parent) = parent else {
            return;
        };
        self.emit(Event::PopupNew {
            id,
            parent,
            geometry: Rect {
                x: geometry.loc.x,
                y: geometry.loc.y,
                width: geometry.size.w,
                height: geometry.size.h,
            },
        });
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        let geometry = positioner.get_geometry();
        surface.with_pending_state(|state| state.geometry = geometry);
        surface.send_repositioned(token);
        let _ = surface.send_pending_configure();
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {
        // The shell closes a popup after a click outside it (PopupDismiss). Input grabs come with F029.
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        if let Some(id) = Self::id_of(surface.wl_surface()) {
            self.popups.remove(&id);
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
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        let name = match image {
            CursorImageStatus::Hidden => "none".to_owned(),
            CursorImageStatus::Named(icon) => icon.name().to_owned(),
            // A cursor that the client draws on a surface: the shell shows its usual arrow.
            CursorImageStatus::Surface(_) => "default".to_owned(),
        };
        tracing::debug!(cursor = name, "the client selected a cursor");
        self.emit(Event::Cursor(name));
    }
}

impl OutputHandler for State {}

impl TabletSeatHandler for State {}

impl SelectionHandler for State {
    type SelectionUserData = ();
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
