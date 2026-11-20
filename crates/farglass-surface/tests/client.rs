//! Tests of the compositor with a real Wayland client: a toplevel with an shm
//! buffer, damage, configure, focus, close, a popup, input, and the clipboard.

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::os::fd::AsFd as _;
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::Duration;

use farglass_surface::{Clip, Command, Commit, Compositor, Event, Format, Rect};
use wayland_client::WEnum;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer,
    wl_data_source, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy as _, QueueHandle, delegate_noop, event_created_child,
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};

const WAIT: Duration = Duration::from_secs(5);

/// What the client got from the compositor.
#[derive(Default)]
struct App {
    configured: u32,
    size: (i32, i32),
    activated: bool,
    closed: bool,
    popup_configured: bool,
    popup_done: bool,
    pointer: Vec<String>,
    keys: Vec<(u32, bool)>,
    keyboard_focus: bool,
    /// The serial of the last keyboard enter, for a selection.
    enter_serial: u32,
    /// The text that the data source of the client writes for a paste.
    source_text: String,
    /// The mime types of each data offer of the compositor.
    offers: Vec<(wl_data_offer::WlDataOffer, Vec<String>)>,
    /// The offer of the current selection.
    selection: Option<wl_data_offer::WlDataOffer>,
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => app.offers.push((id, Vec::new())),
            wl_data_device::Event::Selection { id } => app.selection = id,
            _ => {}
        }
    }

    event_created_child!(App, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for App {
    fn event(
        app: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event
            && let Some((_, types)) = app.offers.iter_mut().find(|(o, _)| o.id() == offer.id())
        {
            types.push(mime_type);
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_source::Event::Send { fd, .. } = event {
            File::from(fd)
                .write_all(app.source_text.as_bytes())
                .unwrap();
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            } => app.pointer.push(format!("enter {surface_x} {surface_y}")),
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => app.pointer.push(format!("motion {surface_x} {surface_y}")),
            wl_pointer::Event::Button { button, state, .. } => {
                app.pointer.push(format!(
                    "button {button:#x} {}",
                    matches!(state, WEnum::Value(wl_pointer::ButtonState::Pressed))
                ));
            }
            wl_pointer::Event::Axis { value, .. } => app.pointer.push(format!("axis {value}")),
            wl_pointer::Event::Leave { .. } => app.pointer.push("leave".to_owned()),
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { serial, .. } => {
                app.keyboard_focus = true;
                app.enter_serial = serial;
            }
            wl_keyboard::Event::Leave { .. } => app.keyboard_focus = false,
            wl_keyboard::Event::Key { key, state, .. } => app.keys.push((
                key,
                matches!(state, WEnum::Value(wl_keyboard::KeyState::Pressed)),
            )),
            _ => {}
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for App {
    fn event(
        _: &mut Self,
        base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for App {
    fn event(
        app: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            app.configured += 1;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for App {
    fn event(
        app: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                app.size = (width, height);
                // Each state is a u32 in native byte order. 4 is Activated.
                app.activated = states
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|s| u32::from_ne_bytes(*s) == 4);
            }
            xdg_toplevel::Event::Close => app.closed = true,
            _ => {}
        }
    }
}

impl Dispatch<xdg_popup::XdgPopup, ()> for App {
    fn event(
        app: &mut Self,
        _: &xdg_popup::XdgPopup,
        event: xdg_popup::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_popup::Event::Configure { .. } => app.popup_configured = true,
            xdg_popup::Event::PopupDone => app.popup_done = true,
            _ => {}
        }
    }
}

delegate_noop!(App: ignore wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_surface::WlSurface);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: ignore wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_buffer::WlBuffer);
delegate_noop!(App: ignore xdg_positioner::XdgPositioner);
delegate_noop!(App: ignore wl_seat::WlSeat);
delegate_noop!(App: ignore wl_data_device_manager::WlDataDeviceManager);

struct Client {
    app: App,
    queue: EventQueue<App>,
    qh: QueueHandle<App>,
    compositor: wl_compositor::WlCompositor,
    shm: wl_shm::WlShm,
    wm: xdg_wm_base::XdgWmBase,
    seat: wl_seat::WlSeat,
    data: wl_data_device_manager::WlDataDeviceManager,
}

impl Client {
    fn connect(path: &std::path::Path) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(path).unwrap()).unwrap();
        let (globals, queue) = registry_queue_init::<App>(&connection).unwrap();
        let qh = queue.handle();
        Self {
            compositor: globals.bind(&qh, 4..=6, ()).unwrap(),
            shm: globals.bind(&qh, 1..=1, ()).unwrap(),
            wm: globals.bind(&qh, 1..=6, ()).unwrap(),
            seat: globals.bind(&qh, 5..=9, ()).unwrap(),
            data: globals.bind(&qh, 3..=3, ()).unwrap(),
            app: App::default(),
            queue,
            qh,
        }
    }

    fn roundtrip(&mut self) {
        self.queue.roundtrip(&mut self.app).unwrap();
    }

    /// Returns a buffer of `width` x `height` with each pixel `pixel` (blue, green, red, alpha).
    fn buffer(&self, width: i32, height: i32, pixel: [u8; 4]) -> wl_buffer::WlBuffer {
        let size = width * height * 4;
        let fd =
            nix::sys::memfd::memfd_create(c"farglass-test", nix::sys::memfd::MFdFlags::empty())
                .unwrap();
        let mut file = File::from(fd);
        file.write_all(&pixel.repeat(usize::try_from(width * height).unwrap()))
            .unwrap();
        let pool = self.shm.create_pool(file.as_fd(), size, &self.qh, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Argb8888,
            &self.qh,
            (),
        );
        pool.destroy();
        buffer
    }
}

fn next(
    events: &mpsc::Receiver<Event>,
    client: &mut Client,
    wanted: impl Fn(&Event) -> bool,
) -> Event {
    let end = std::time::Instant::now() + WAIT;
    loop {
        client.roundtrip();
        match events.recv_timeout(Duration::from_millis(50)) {
            Ok(event) if wanted(&event) => return event,
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("the compositor stopped"),
        }
        assert!(std::time::Instant::now() < end, "no matching event");
    }
}

fn commit_of(event: Event) -> Commit {
    let Event::Commit(commit) = event else {
        panic!("not a commit: {event:?}")
    };
    commit
}

#[test]
fn a_toplevel_gives_its_buffer_and_damage_and_the_shell_controls_size_focus_and_close() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("wayland-test");
    let (sender, events) = mpsc::channel();
    let compositor = Compositor::spawn(&socket, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let mut client = Client::connect(compositor.socket());

    let surface = client.compositor.create_surface(&client.qh, ());
    let xdg = client.wm.get_xdg_surface(&surface, &client.qh, ());
    let toplevel = xdg.get_toplevel(&client.qh, ());
    toplevel.set_title("Calculator".to_owned());
    toplevel.set_app_id("org.example.Calculator".to_owned());
    surface.commit();
    let Event::ToplevelNew { id } = next(&events, &mut client, |e| {
        matches!(e, Event::ToplevelNew { .. })
    }) else {
        unreachable!()
    };
    let changed = next(&events, &mut client, |e| {
        matches!(
            e,
            Event::ToplevelChanged {
                app_id: Some(_),
                ..
            }
        )
    });
    assert_eq!(
        changed,
        Event::ToplevelChanged {
            id,
            title: Some("Calculator".to_owned()),
            app_id: Some("org.example.Calculator".to_owned()),
            parent: None
        }
    );
    assert!(
        client.app.configured >= 1,
        "the first commit gives the first configure"
    );

    // The first buffer: the whole buffer is damage.
    let red = client.buffer(64, 48, [0, 0, 255, 255]);
    surface.attach(Some(&red), 0, 0);
    surface.damage_buffer(0, 0, 64, 48);
    surface.commit();
    let first = commit_of(next(&events, &mut client, |e| {
        matches!(e, Event::Commit(_))
    }));
    assert_eq!(
        (first.id, first.width, first.height, first.format),
        (id, 64, 48, Format::Argb8888)
    );
    assert_eq!(
        first.damage,
        [Rect {
            x: 0,
            y: 0,
            width: 64,
            height: 48
        }]
    );
    assert_eq!(first.pixels.len(), 64 * 48 * 4);
    assert_eq!(&first.pixels[..4], &[0, 0, 255, 255]);

    // A second buffer of the same size with a small damage.
    let blue = client.buffer(64, 48, [255, 0, 0, 255]);
    surface.attach(Some(&blue), 0, 0);
    surface.damage_buffer(10, 10, 5, 5);
    surface.damage_buffer(60, 40, 20, 20);
    surface.commit();
    let second = commit_of(next(&events, &mut client, |e| {
        matches!(e, Event::Commit(_))
    }));
    assert_eq!(
        second.damage,
        [
            Rect {
                x: 10,
                y: 10,
                width: 5,
                height: 5
            },
            Rect {
                x: 60,
                y: 40,
                width: 4,
                height: 8
            }
        ]
    );
    assert_eq!(&second.pixels[..4], &[255, 0, 0, 255]);

    // The shell gives the size, the focus, and the close.
    compositor.send(Command::Configure {
        id,
        width: 300,
        height: 200,
    });
    compositor.send(Command::Activate { id: Some(id) });
    let end = std::time::Instant::now() + WAIT;
    while !(client.app.size == (300, 200) && client.app.activated) {
        client.roundtrip();
        std::thread::sleep(Duration::from_millis(10));
        assert!(
            std::time::Instant::now() < end,
            "no configure with the size and the focus"
        );
    }
    compositor.send(Command::Close { id });
    while !client.app.closed {
        client.roundtrip();
        std::thread::sleep(Duration::from_millis(10));
        assert!(std::time::Instant::now() < end, "no close");
    }

    // A popup of the window.
    let popup_surface = client.compositor.create_surface(&client.qh, ());
    let popup_xdg = client.wm.get_xdg_surface(&popup_surface, &client.qh, ());
    let positioner = client.wm.create_positioner(&client.qh, ());
    positioner.set_size(50, 30);
    positioner.set_anchor_rect(5, 5, 10, 10);
    let popup = popup_xdg.get_popup(Some(&xdg), &positioner, &client.qh, ());
    popup_surface.commit();
    let new_popup = next(&events, &mut client, |e| {
        matches!(e, Event::PopupNew { .. })
    });
    let Event::PopupNew {
        id: popup_id,
        parent,
        geometry,
    } = new_popup
    else {
        unreachable!()
    };
    assert_eq!(parent, id);
    assert_eq!((geometry.width, geometry.height), (50, 30));
    while !client.app.popup_configured {
        client.roundtrip();
    }
    compositor.send(Command::PopupDismiss { id: popup_id });
    while !client.app.popup_done {
        client.roundtrip();
        std::thread::sleep(Duration::from_millis(10));
        assert!(std::time::Instant::now() < end, "no popup_done");
    }
    popup.destroy();
    popup_xdg.destroy();
    assert_eq!(
        next(&events, &mut client, |e| matches!(
            e,
            Event::PopupClosed { .. }
        )),
        Event::PopupClosed { id: popup_id }
    );

    // The client closes the window.
    toplevel.destroy();
    xdg.destroy();
    assert_eq!(
        next(&events, &mut client, |e| matches!(
            e,
            Event::ToplevelClosed { .. }
        )),
        Event::ToplevelClosed { id }
    );

    drop(compositor);
    assert!(!socket.exists(), "the socket goes with the compositor");
}

#[test]
fn a_second_compositor_on_a_used_socket_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("wayland-test");
    let first = Compositor::spawn(&socket, |_| {}).unwrap();
    assert!(Compositor::spawn(&socket, |_| {}).is_err());
    drop(first);
    // A file of a stopped compositor goes, and a new one starts.
    std::fs::write(&socket, b"").unwrap();
    let _again = Compositor::spawn(&socket, |_| {}).unwrap();
}

#[test]
fn pointer_and_keyboard_input_reach_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("wayland-test");
    let (sender, events) = mpsc::channel();
    let compositor = Compositor::spawn(&socket, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let mut client = Client::connect(compositor.socket());
    let _pointer = client.seat.get_pointer(&client.qh, ());
    let _keyboard = client.seat.get_keyboard(&client.qh, ());
    let surface = client.compositor.create_surface(&client.qh, ());
    let xdg = client.wm.get_xdg_surface(&surface, &client.qh, ());
    let _toplevel = xdg.get_toplevel(&client.qh, ());
    surface.commit();
    let Event::ToplevelNew { id } = next(&events, &mut client, |e| {
        matches!(e, Event::ToplevelNew { .. })
    }) else {
        unreachable!()
    };
    client.roundtrip();
    let buffer = client.buffer(100, 80, [9, 9, 9, 255]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    next(&events, &mut client, |e| matches!(e, Event::Commit(_)));

    compositor.send(Command::PointerMotion {
        id,
        x: 12.5,
        y: 30.0,
    });
    compositor.send(Command::PointerMotion {
        id,
        x: 20.0,
        y: 31.0,
    });
    compositor.send(Command::PointerButton {
        button: 0x110,
        pressed: true,
    });
    compositor.send(Command::PointerButton {
        button: 0x110,
        pressed: false,
    });
    compositor.send(Command::PointerAxis { dx: 0.0, dy: 15.0 });
    // No activate: the key names its window, which gets the keyboard focus first.
    compositor.send(Command::Key {
        id: Some(id),
        code: 30,
        pressed: true,
    });
    compositor.send(Command::Key {
        id: Some(id),
        code: 30,
        pressed: false,
    });
    compositor.send(Command::PointerLeave);
    let end = std::time::Instant::now() + WAIT;
    while client.app.keys.len() < 2 || !client.app.pointer.contains(&"leave".to_owned()) {
        client.roundtrip();
        std::thread::sleep(Duration::from_millis(10));
        assert!(
            std::time::Instant::now() < end,
            "input missing: {:?} {:?}",
            client.app.pointer,
            client.app.keys
        );
    }
    assert_eq!(
        client.app.pointer,
        [
            "enter 12.5 30",
            "motion 20 31",
            "button 0x110 true",
            "button 0x110 false",
            "axis 15",
            "leave"
        ]
    );
    assert!(client.app.keyboard_focus);
    assert_eq!(
        client.app.keys,
        [(30, true), (30, false)],
        "the key code of A goes to the window with the focus"
    );
}

#[test]
fn the_clipboard_goes_from_an_app_to_the_shell_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("wayland-test");
    let (sender, events) = mpsc::channel();
    let compositor = Compositor::spawn(&socket, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let mut client = Client::connect(compositor.socket());
    let _keyboard = client.seat.get_keyboard(&client.qh, ());
    let device = client.data.get_data_device(&client.seat, &client.qh, ());
    let surface = client.compositor.create_surface(&client.qh, ());
    let xdg = client.wm.get_xdg_surface(&surface, &client.qh, ());
    let _toplevel = xdg.get_toplevel(&client.qh, ());
    surface.commit();
    let Event::ToplevelNew { id } = next(&events, &mut client, |e| {
        matches!(e, Event::ToplevelNew { .. })
    }) else {
        unreachable!()
    };
    compositor.send(Command::Activate { id: Some(id) });
    let end = std::time::Instant::now() + WAIT;
    while !client.app.keyboard_focus {
        client.roundtrip();
        assert!(std::time::Instant::now() < end, "no keyboard focus");
    }

    // A copy in the app: the shell gets the text.
    client.app.source_text = "copied in the app \u{2713}".to_owned();
    let source = client.data.create_data_source(&client.qh, ());
    source.offer("image/png".to_owned());
    source.offer("text/plain;charset=utf-8".to_owned());
    device.set_selection(Some(&source), client.app.enter_serial);
    let event = next(&events, &mut client, |e| matches!(e, Event::Clipboard(_)));
    assert_eq!(
        event,
        Event::Clipboard(Clip::Text("copied in the app \u{2713}".to_owned())),
        "text goes before an image"
    );

    // A copy on the client computer: the app gets an offer of the compositor and pastes it.
    compositor.send(Command::SetClipboard(Clip::Text(
        "copied in the browser".to_owned(),
    )));
    let end = std::time::Instant::now() + WAIT;
    let offer = loop {
        client.roundtrip();
        if let Some(offer) = client.app.selection.clone() {
            break offer;
        }
        assert!(std::time::Instant::now() < end, "no selection offer");
    };
    let types = client
        .app
        .offers
        .iter()
        .find(|(o, _)| o.id() == offer.id())
        .map(|(_, t)| t.clone())
        .unwrap();
    assert!(
        types.contains(&"text/plain;charset=utf-8".to_owned()),
        "{types:?}"
    );
    assert!(types.contains(&"UTF8_STRING".to_owned()), "{types:?}");
    let (mut reader, writer) = std::io::pipe().unwrap();
    offer.receive("UTF8_STRING".to_owned(), writer.as_fd());
    drop(writer);
    client.roundtrip();
    let mut pasted = String::new();
    reader.read_to_string(&mut pasted).unwrap();
    assert_eq!(pasted, "copied in the browser");
    assert!(
        events.try_iter().all(|e| !matches!(e, Event::Clipboard(_))),
        "the clipboard of the shell does not come back to the shell"
    );
}
