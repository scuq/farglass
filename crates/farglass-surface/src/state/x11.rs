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
use std::process::Stdio;

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
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::xwm::{Reorder, ResizeEdge, WmWindowProperty, X11Window, XwmId};
use smithay::xwayland::{
    X11Surface, X11Wm, XWayland, XWaylandClientData, XWaylandEvent, XwmHandler,
};

use super::{Event, Rect, State};

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
}

/// Returns `true` if Xwayland owns the surface.
pub(super) fn is_xwayland(surface: &WlSurface) -> bool {
    surface
        .client()
        .is_some_and(|client| client.get_data::<XWaylandClientData>().is_some())
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
        }
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
pub(super) fn start(display: &DisplayHandle, handle: &LoopHandle<'static, State>) -> Option<u32> {
    let spawned = XWayland::spawn(
        display,
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
                }
            }
        }
        XWaylandEvent::Error => tracing::warn!("Xwayland stopped during its start"),
    });
    if let Err(error) = inserted {
        tracing::warn!(%error, "Xwayland has no event source, X11 apps do not show");
        return None;
    }
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

    fn disconnected(&mut self, _xwm: XwmId) {
        tracing::warn!("Xwayland stopped, X11 apps do not show");
        self.x11.wm = None;
    }
}

delegate_xwayland_shell!(State);
