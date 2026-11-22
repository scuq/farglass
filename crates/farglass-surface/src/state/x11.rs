//! X11 apps through Xwayland (F082).
//!
//! The compositor starts Xwayland and is its X11 window manager. Each X11
//! top-level window is one surface for the shell, with the events and the
//! commands of a Wayland window. An override-redirect window, for example a
//! menu or a tooltip, is a popup of the window that it belongs to.
//!
//! The shell places each window, and the compositor does not know the place.
//! Thus each X11 top-level window is at the origin of the X11 screen, and the
//! place of a menu on that screen is its place relative to its window.

use std::borrow::Cow;
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::process::Stdio;
use std::time::Duration;

use smithay::backend::input::KeyState;
use smithay::delegate_xwayland_shell;
use smithay::input::Seat;
use smithay::input::keyboard::{KeyboardTarget, KeysymHandle, ModifiersState};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{DisplayHandle, Resource as _};
use smithay::utils::{IsAlive, Logical, Rectangle, SERIAL_COUNTER, Serial};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::selection::SelectionTarget;
use smithay::wayland::selection::data_device::{
    clear_data_device_selection, request_data_device_client_selection, set_data_device_selection,
};
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::xwm::{Reorder, ResizeEdge, WmWindowProperty, X11Window, XwmId};
use smithay::xwayland::{X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler};

use super::{ClipData, ClipFrom, ClipOwner, Event, Rect, State, write_clip};

/// The longest wait for the X11 window manager at the start of the compositor.
const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// The wait before the compositor closes an X11 menu: the app can close it first, for example after a click on its own window.
const DISMISS_DELAY: Duration = Duration::from_millis(50);

/// The X11 windows of the compositor.
pub(super) struct X11 {
    shell: XWaylandShellState,
    /// The window manager, after Xwayland is ready.
    pub(super) wm: Option<X11Wm>,
    /// The top-level windows by the id of their surface.
    pub(super) windows: HashMap<u32, X11Surface>,
    /// The override-redirect windows that show as popups.
    popups: HashMap<u32, X11Surface>,
    /// The id of the surface of each X11 window.
    ids: HashMap<X11Window, u32>,
    /// The last buffer and its scale of each Xwayland surface that has no window yet.
    pending: HashMap<ObjectId, (WlBuffer, i32)>,
    /// The event loop, for the transfers of the X11 clipboard.
    handle: Option<LoopHandle<'static, State>>,
    /// The number of the X11 display.
    display: Option<u32>,
    /// Xwayland or its window manager did not start.
    failed: bool,
}

impl X11 {
    pub(super) fn new(display: &DisplayHandle) -> Self {
        Self {
            shell: XWaylandShellState::new::<State>(display),
            wm: None,
            windows: HashMap::new(),
            popups: HashMap::new(),
            ids: HashMap::new(),
            pending: HashMap::new(),
            handle: None,
            display: None,
            failed: false,
        }
    }

    /// Returns `true` if `id` is an X11 menu or tooltip.
    pub(super) fn is_popup(&self, id: u32) -> bool {
        self.popups.contains_key(&id)
    }

    /// Waits until Xwayland did each request of the window manager, for example a new X11 focus.
    pub(super) fn sync(&self) {
        // Each request with a reply is a round trip: Xwayland answers after the requests before it.
        if let Some(wm) = &self.wm
            && let Err(error) = wm.get_randr_primary_output()
        {
            tracing::debug!(%error, "Xwayland did not answer the sync");
        }
    }

    /// Gives a new clipboard of the shell or of a Wayland app to the X11 apps. `None` clears it.
    pub(super) fn announce(&mut self, types: Option<Vec<String>>) {
        if let Some(wm) = self.wm.as_mut()
            && let Err(error) = wm.new_selection(SelectionTarget::Clipboard, types)
        {
            tracing::debug!(%error, "the X11 apps did not get the clipboard");
        }
    }

    /// Asks the X11 app that owns the clipboard to write it as `mime` to `fd`.
    pub(super) fn read_selection(&mut self, mime: String, fd: OwnedFd) -> Result<(), String> {
        let (Some(wm), Some(handle)) = (self.wm.as_mut(), self.handle.as_ref()) else {
            return Err("Xwayland stopped".to_owned());
        };
        wm.send_selection(SelectionTarget::Clipboard, mime, fd, handle.clone())
            .map_err(|error| error.to_string())
    }

    /// Keeps the last buffer of an Xwayland surface that has no window yet. An older buffer goes back.
    pub(super) fn keep(&mut self, surface: &WlSurface, buffer: WlBuffer, scale: i32) {
        if let Some((old, _)) = self.pending.insert(surface.id(), (buffer, scale)) {
            old.release();
        }
    }

    /// Gives back the kept buffer of a surface that is gone.
    pub(super) fn forget(&mut self, surface: &WlSurface) {
        if let Some((buffer, _)) = self.pending.remove(&surface.id()) {
            buffer.release();
        }
    }

    /// Returns the Wayland surface of the window or the popup `id`.
    pub(super) fn surface(&self, id: u32) -> Option<WlSurface> {
        self.windows
            .get(&id)
            .or_else(|| self.popups.get(&id))
            .and_then(X11Surface::wl_surface)
    }
}

/// The window with the keyboard: a Wayland surface or an X11 window.
/// An X11 window also needs the input focus of the X11 server.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Focus {
    Wayland(WlSurface),
    X11(X11Surface),
}

impl IsAlive for Focus {
    fn alive(&self) -> bool {
        match self {
            Self::Wayland(surface) => surface.alive(),
            Self::X11(window) => window.alive(),
        }
    }
}

impl WaylandFocus for Focus {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            Self::Wayland(surface) => Some(Cow::Borrowed(surface)),
            Self::X11(window) => window.wl_surface().map(Cow::Owned),
        }
    }
}

impl KeyboardTarget<State> for Focus {
    fn enter(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
        match self {
            Self::Wayland(surface) => KeyboardTarget::enter(surface, seat, data, keys, serial),
            Self::X11(window) => KeyboardTarget::enter(window, seat, data, keys, serial),
        }
    }

    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial) {
        match self {
            Self::Wayland(surface) => KeyboardTarget::leave(surface, seat, data, serial),
            Self::X11(window) => KeyboardTarget::leave(window, seat, data, serial),
        }
    }

    fn key(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: u32,
    ) {
        match self {
            Self::Wayland(surface) => {
                KeyboardTarget::key(surface, seat, data, key, state, serial, time);
            }
            Self::X11(window) => KeyboardTarget::key(window, seat, data, key, state, serial, time),
        }
    }

    fn modifiers(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        match self {
            Self::Wayland(surface) => {
                KeyboardTarget::modifiers(surface, seat, data, modifiers, serial);
            }
            Self::X11(window) => KeyboardTarget::modifiers(window, seat, data, modifiers, serial),
        }
    }
}

/// Starts Xwayland. Returns the number of its X11 display, the `N` of `DISPLAY=:N`.
///
/// The sockets of the display listen at once, thus an app can connect before Xwayland is ready.
pub(super) fn start(state: &mut State, handle: &LoopHandle<'static, State>) -> Option<u32> {
    state.x11.handle = Some(handle.clone());
    let spawned = XWayland::spawn(
        &state.display,
        None,
        std::iter::empty::<(String, String)>(),
        false,
        Stdio::null(),
        Stdio::null(),
        |_| (),
    );
    let (xwayland, client) = match spawned {
        Ok(spawned) => spawned,
        Err(error) => {
            tracing::warn!(%error, "Xwayland did not start, X11 apps do not show");
            return None;
        }
    };
    let number = xwayland.display_number();
    let wm_handle = handle.clone();
    let inserted = handle.insert_source(xwayland, move |event, (), state| match event {
        XWaylandEvent::Ready { x11_socket, .. } => {
            match X11Wm::start_wm(wm_handle.clone(), x11_socket, client.clone()) {
                Ok(wm) => {
                    tracing::info!(display = number, "Xwayland is ready");
                    state.x11.wm = Some(wm);
                }
                Err(error) => {
                    tracing::warn!(%error, "the X11 window manager did not start, X11 apps do not show");
                    state.x11.failed = true;
                }
            }
        }
        XWaylandEvent::Error => {
            tracing::warn!("Xwayland stopped during its start");
            state.x11.failed = true;
        }
    });
    if let Err(error) = inserted {
        tracing::warn!(%error, "Xwayland has no event source, X11 apps do not show");
        return None;
    }
    state.x11.display = Some(number);
    Some(number)
}

/// Runs the event loop until the X11 window manager of display `number` runs, and returns the number.
///
/// The sockets of the display accept clients before the window manager runs. A window that an app
/// maps before, or a clipboard that it sets before, does not reach the window manager.
/// Thus nobody gets the display before the window manager runs. `None` if it does not start in time.
pub(super) fn wait_ready(
    event_loop: &mut smithay::reexports::calloop::EventLoop<'static, State>,
    state: &mut State,
    number: u32,
) -> Option<u32> {
    let start = std::time::Instant::now();
    while state.x11.wm.is_none() && !state.x11.failed && start.elapsed() < READY_TIMEOUT {
        if let Err(error) = event_loop.dispatch(Some(Duration::from_millis(20)), state) {
            tracing::warn!(%error, "the start of Xwayland stopped");
            return None;
        }
        let _ = state.display.flush_clients();
    }
    if state.x11.wm.is_none() {
        tracing::warn!(
            display = number,
            "the X11 window manager did not start in time, X11 apps do not show"
        );
        return None;
    }
    tracing::debug!(display = number, elapsed = ?start.elapsed(), "the X11 window manager runs");
    Some(number)
}

impl State {
    /// Returns the id of the surface of an X11 window.
    fn x11_id(&self, window: &X11Surface) -> Option<u32> {
        self.x11.ids.get(&window.window_id()).copied()
    }

    /// Gives a new id to an X11 window that shows now.
    fn x11_new_id(&mut self, window: &X11Surface) -> u32 {
        self.x11_remove(window);
        let id = self.alloc_id();
        self.x11.ids.insert(window.window_id(), id);
        // Xwayland can give the Wayland surface before or after the X11 window shows.
        if let Some(surface) = window.wl_surface() {
            Self::tag(&surface, id);
        }
        id
    }

    /// Adds an X11 window as a top-level window of the shell.
    fn x11_add_window(&mut self, id: u32, window: X11Surface) {
        self.x11.windows.insert(id, window.clone());
        self.touch(Some(id));
        // The X server knows the process of each client (X-Resource). `_NET_WM_PID` comes from
        // the app itself, and some apps set it late or not at all.
        let pid = window.get_client_pid().ok().or_else(|| window.pid());
        self.emit(Event::ToplevelNew { id, pid });
        self.x11_changed(id, &window);
        self.x11_flush(id, &window);
    }

    /// Gives the shell the frame that the surface of the window committed before the shell knew the window.
    fn x11_flush(&mut self, id: u32, window: &X11Surface) {
        let Some(surface) = window.wl_surface() else {
            return;
        };
        if let Some((buffer, scale)) = self.x11.pending.remove(&surface.id()) {
            // The surface has no size yet, thus the commit has the whole buffer as damage.
            self.take_buffer(id, &surface, &buffer, Vec::new(), scale);
        }
    }

    /// Removes an X11 window that does not show now, and tells the shell.
    fn x11_remove(&mut self, window: &X11Surface) {
        let Some(id) = self.x11.ids.remove(&window.window_id()) else {
            return;
        };
        // A later commit of the surface must not go to the shell with the old id.
        if let Some(surface) = window.wl_surface() {
            Self::tag(&surface, 0);
        }
        self.pace.remove(id);
        if self.x11.windows.remove(&id).is_some() {
            if self.focused == Some(id) {
                self.focused = None;
            }
            self.emit(Event::ToplevelClosed { id });
        } else if self.x11.popups.remove(&id).is_some() {
            self.parents.remove(&id);
            self.emit(Event::PopupClosed { id });
        }
    }

    /// Gives the shell the title, the class, and the parent of an X11 window.
    fn x11_changed(&mut self, id: u32, window: &X11Surface) {
        let text = |text: String| (!text.is_empty()).then_some(text);
        let parent = window
            .is_transient_for()
            .and_then(|parent| self.x11.ids.get(&parent).copied())
            .filter(|parent| self.x11.windows.contains_key(parent));
        self.emit(Event::ToplevelChanged {
            id,
            title: text(window.title()),
            app_id: text(window.class()),
            parent,
        });
    }

    /// Returns the window that an override-redirect window belongs to: the window that it names,
    /// or the X11 window with the keyboard, or the X11 window under the pointer.
    fn x11_parent(&self, window: &X11Surface) -> Option<u32> {
        let is_window = |id: &u32| self.x11.windows.contains_key(id);
        window
            .is_transient_for()
            .and_then(|parent| self.x11.ids.get(&parent).copied())
            .map(|id| self.root(id))
            .filter(is_window)
            .or_else(|| self.focused.filter(is_window))
            .or_else(|| self.pointer_on.map(|id| self.root(id)).filter(is_window))
    }

    /// Returns the place of an override-redirect window relative to its window.
    fn x11_place(&self, geometry: Rectangle<i32, Logical>, parent: u32) -> Rect {
        let origin = self.x11.windows.get(&parent).map_or((0, 0), |parent| {
            let loc = parent.geometry().loc;
            (loc.x, loc.y)
        });
        Rect {
            x: geometry.loc.x - origin.0,
            y: geometry.loc.y - origin.1,
            width: geometry.size.w,
            height: geometry.size.h,
        }
    }

    /// Closes the open X11 menu: the app gets a click outside each of its windows (F084).
    ///
    /// An X11 app closes its menu after a click outside it, but a click on the desktop of the
    /// shell does not go to Xwayland. Thus a thread clicks with XTEST at a place outside each
    /// X11 window, and only if an app holds the pointer, as an app with an open menu does.
    pub(super) fn x11_dismiss(&self) {
        let Some(display) = self.x11.display else {
            return;
        };
        // Each X11 window is at the origin: a place right or below each window is outside each one.
        let (mut right, mut bottom) = (0, 0);
        for window in self.x11.windows.values().chain(self.x11.popups.values()) {
            let geometry = window.geometry();
            right = right.max(geometry.loc.x + geometry.size.w);
            bottom = bottom.max(geometry.loc.y + geometry.size.h);
        }
        let spawned = std::thread::Builder::new()
            .name("farglass-x11-dismiss".to_owned())
            .spawn(move || {
                std::thread::sleep(DISMISS_DELAY);
                if let Err(error) = click_outside(display, right, bottom) {
                    tracing::debug!(%error, "the X11 menu did not get the click outside it");
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "no thread to close an X11 menu");
        }
    }

    /// Gives the keyboard to the window with the focus again, for example after its Wayland surface came.
    fn x11_refocus(&mut self, window: &X11Surface) {
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
            keyboard.set_focus(
                self,
                Some(Focus::X11(window.clone())),
                SERIAL_COUNTER.next_serial(),
            );
        }
    }
}

impl XWaylandShellHandler for State {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.x11.shell
    }

    fn surface_associated(&mut self, _xwm: XwmId, surface: WlSurface, window: X11Surface) {
        let Some(id) = self.x11_id(&window) else {
            return;
        };
        Self::tag(&surface, id);
        let shown = self
            .x11
            .windows
            .get(&id)
            .or_else(|| self.x11.popups.get(&id))
            .cloned();
        if let Some(shown) = shown {
            self.x11_flush(id, &shown);
        }
        // The window got the focus of the X11 server before it had a surface for the keys.
        if self.focused == Some(id) {
            self.x11_refocus(&window);
        }
    }
}

impl XwmHandler for State {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.x11
            .wm
            .as_mut()
            .expect("an X11 event comes only from the window manager")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(error) = window.set_mapped(true) {
            tracing::debug!(%error, "an X11 window did not map");
            return;
        }
        // Each window is at the origin: see the module text.
        let size = window.geometry().size;
        if let Err(error) = window.configure(Rectangle::new((0, 0).into(), size)) {
            tracing::debug!(%error, "an X11 window did not take its place");
        }
        let id = self.x11_new_id(&window);
        tracing::debug!(id, title = window.title(), "a new X11 window");
        self.x11_add_window(id, window);
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let parent = self.x11_parent(&window);
        let id = self.x11_new_id(&window);
        tracing::debug!(id, ?parent, geometry = ?window.geometry(), "a new X11 override-redirect window");
        match parent {
            Some(parent) => {
                let geometry = self.x11_place(window.geometry(), parent);
                self.x11.popups.insert(id, window);
                self.parents.insert(id, parent);
                self.touch(Some(id));
                self.emit(Event::PopupNew {
                    id,
                    parent,
                    geometry,
                    subsurface: false,
                });
                if let Some(popup) = self.x11.popups.get(&id).cloned() {
                    self.x11_flush(id, &popup);
                }
            }
            // A window with no parent, for example a splash screen, is a window of its own.
            None => self.x11_add_window(id, window),
        }
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        if !window.is_override_redirect()
            && let Err(error) = window.set_mapped(false)
        {
            tracing::debug!(%error, "an X11 window did not unmap");
        }
        self.x11_remove(&window);
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.x11_remove(&window);
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        _x: Option<i32>,
        _y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        // The app selects its size. The place stays the origin.
        let mut geometry = window.geometry();
        geometry.loc = (0, 0).into();
        if let Some(w) = w.and_then(|w| i32::try_from(w).ok()) {
            geometry.size.w = w;
        }
        if let Some(h) = h.and_then(|h| i32::try_from(h).ok()) {
            geometry.size.h = h;
        }
        if let Err(error) = window.configure(geometry) {
            tracing::debug!(%error, "an X11 window did not get its size");
        }
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        geometry: Rectangle<i32, Logical>,
        _above: Option<X11Window>,
    ) {
        // A menu that moves or grows.
        if let Some(id) = self.x11_id(&window)
            && self.x11.popups.contains_key(&id)
            && let Some(&parent) = self.parents.get(&id)
        {
            let geometry = self.x11_place(geometry, parent);
            self.emit(Event::PopupNew {
                id,
                parent,
                geometry,
                subsurface: false,
            });
        }
    }

    fn property_notify(&mut self, _xwm: XwmId, window: X11Surface, _property: WmWindowProperty) {
        if let Some(id) = self.x11_id(&window)
            && self.x11.windows.contains_key(&id)
        {
            self.x11_changed(id, &window);
        }
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _resize_edge: ResizeEdge,
    ) {
        // The shell draws the frame of each window, and its edges change the size.
    }

    fn move_request(&mut self, _xwm: XwmId, window: X11Surface, _button: u32) {
        if let Some(id) = self.x11_id(&window) {
            self.emit(Event::MoveRequest { id });
        }
    }

    fn allow_selection_access(&mut self, _xwm: XwmId, selection: SelectionTarget) -> bool {
        // The clipboard only. The primary selection of X11 stays with the X11 apps.
        selection == SelectionTarget::Clipboard
    }

    fn send_selection(
        &mut self,
        _xwm: XwmId,
        selection: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        if selection != SelectionTarget::Clipboard {
            return;
        }
        match &self.clip_owner {
            ClipOwner::Shell(clip) => {
                tracing::debug!(mime_type, "an X11 app pastes the clipboard of the shell");
                write_clip(clip.clone(), fd);
            }
            ClipOwner::Wayland => {
                tracing::debug!(
                    mime_type,
                    "an X11 app pastes the clipboard of a Wayland app"
                );
                if let Err(error) = request_data_device_client_selection(&self.seat, mime_type, fd)
                {
                    tracing::debug!(%error, "the clipboard of a Wayland app is not available");
                }
            }
            ClipOwner::None | ClipOwner::X11 => {}
        }
    }

    fn new_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        if selection != SelectionTarget::Clipboard {
            return;
        }
        tracing::debug!(?mime_types, "an X11 app set the clipboard");
        self.clip_generation += 1;
        self.clip_owner = ClipOwner::X11;
        // The Wayland apps can paste it, and the shell gets it.
        set_data_device_selection(&self.display, &self.seat, mime_types.clone(), ClipData::X11);
        self.selection = Some((mime_types, ClipFrom::X11));
    }

    fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
        if selection == SelectionTarget::Clipboard && matches!(self.clip_owner, ClipOwner::X11) {
            self.clip_owner = ClipOwner::None;
            clear_data_device_selection(&self.display, &self.seat);
        }
    }

    fn disconnected(&mut self, _xwm: XwmId) {
        tracing::warn!("Xwayland stopped, X11 apps do not show");
        self.x11.wm = None;
    }
}

/// Clicks with XTEST at a place right of `right` or below `bottom`, if an X11 app holds the pointer.
fn click_outside(display: u32, right: i32, bottom: i32) -> Result<(), String> {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{
        BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ConnectionExt as _, EventMask, GrabMode,
        GrabStatus, MOTION_NOTIFY_EVENT,
    };
    use x11rb::protocol::xtest::ConnectionExt as _;
    use x11rb::wrapper::ConnectionExt as _;

    let error = |error: &dyn std::fmt::Display| error.to_string();
    let (conn, screen) = x11rb::connect(Some(&format!(":{display}"))).map_err(|e| error(&e))?;
    let root = conn.setup().roots[screen].clone();
    // A grab of the pointer fails if an app holds it. With no such app, there is no menu to close.
    let grab = conn
        .grab_pointer(
            false,
            root.root,
            EventMask::NO_EVENT,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            x11rb::CURRENT_TIME,
        )
        .map_err(|e| error(&e))?
        .reply()
        .map_err(|e| error(&e))?;
    if grab.status == GrabStatus::SUCCESS {
        conn.ungrab_pointer(x11rb::CURRENT_TIME)
            .map_err(|e| error(&e))?;
        conn.flush().map_err(|e| error(&e))?;
        return Ok(());
    }
    if grab.status != GrabStatus::ALREADY_GRABBED {
        return Ok(());
    }
    let width = i32::from(root.width_in_pixels);
    let height = i32::from(root.height_in_pixels);
    // The corner right and below each window, or the last pixel of the screen if a window covers it.
    let x = i16::try_from((right).min(width - 1).max(0)).unwrap_or(i16::MAX);
    let y = i16::try_from((bottom).min(height - 1).max(0)).unwrap_or(i16::MAX);
    for (kind, detail) in [
        (MOTION_NOTIFY_EVENT, 0),
        (BUTTON_PRESS_EVENT, 1),
        (BUTTON_RELEASE_EVENT, 1),
    ] {
        conn.xtest_fake_input(kind, detail, x11rb::CURRENT_TIME, root.root, x, y, 0)
            .map_err(|e| error(&e))?;
    }
    conn.sync().map_err(|e| error(&e))?;
    tracing::debug!(x, y, "an X11 menu got a click outside it");
    Ok(())
}

delegate_xwayland_shell!(State);
