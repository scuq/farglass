//! camera-node - a PipeWire camera node with media.class Video/Source (F115)
//!
//! Usage: camera-node SECONDS [plain|choice] [I420|NV12|YUY2]
//!
//! The node "farglass-camera" gives a moving test picture at 1280x720 and 30
//! frames each second for SECONDS. It has media.role Camera, so the camera
//! portal and the PipeWire camera of Chromium and Firefox can find it.
//! "plain" (the default) writes the size and the frame rate of EnumFormat as
//! plain values, as the V4L2 nodes of PipeWire do. "choice" writes them in a
//! choice of the type None, as the pipewiresink of GStreamer does.
//! The node is the driver of its graph: a timer starts each frame.
//!
//! With GRANT=yes, the node does the work that WirePlumber does not do on
//! Debian 13: the camera portal hides all nodes from the PipeWire connection
//! that it gives to an app. The program gives each such connection of a host
//! app (app id "") the permissions rwx on this node, and on no other node.
//!
//! Exit status: 0 after SECONDS, 1 on an error.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use spa::pod::{ChoiceValue, Object, Pod, Property, PropertyFlags, Value};
use spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const FPS: u32 = 30;

fn property(key: u32, value: Value) -> Property {
    Property {
        key,
        flags: PropertyFlags::empty(),
        value,
    }
}

fn serialize(value: &Value) -> Vec<u8> {
    spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), value)
        .expect("the pod serializes")
        .0
        .into_inner()
}

/// Returns the EnumFormat pod of the node.
fn enum_format(format: u32, choice: bool) -> Vec<u8> {
    let size = Rectangle {
        width: WIDTH,
        height: HEIGHT,
    };
    let rate = Fraction { num: FPS, denom: 1 };
    let (size, rate) = if choice {
        (
            Value::Choice(ChoiceValue::Rectangle(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::None(size),
            ))),
            Value::Choice(ChoiceValue::Fraction(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::None(rate),
            ))),
        )
    } else {
        (Value::Rectangle(size), Value::Fraction(rate))
    };
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
            property(spa::sys::SPA_FORMAT_VIDEO_format, Value::Id(Id(format))),
            property(spa::sys::SPA_FORMAT_VIDEO_size, size),
            property(spa::sys::SPA_FORMAT_VIDEO_framerate, rate),
        ],
    }))
}

/// Returns the bytes of one frame and the stride of its first plane.
fn frame_size(format: u32) -> (u32, u32) {
    match format {
        spa::sys::SPA_VIDEO_FORMAT_YUY2 => (WIDTH * HEIGHT * 2, WIDTH * 2),
        _ => (WIDTH * HEIGHT * 3 / 2, WIDTH),
    }
}

/// Returns the Buffers pod: one block with a full frame.
fn buffers(format: u32) -> Vec<u8> {
    let (size, stride) = frame_size(format);
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
            property(
                spa::sys::SPA_PARAM_BUFFERS_dataType,
                Value::Choice(ChoiceValue::Int(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Flags {
                        default: (1 << spa::sys::SPA_DATA_MemFd) | (1 << spa::sys::SPA_DATA_MemPtr),
                        flags: vec![],
                    },
                ))),
            ),
            property(spa::sys::SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
            property(spa::sys::SPA_PARAM_BUFFERS_size, Value::Int(size as i32)),
            property(spa::sys::SPA_PARAM_BUFFERS_stride, Value::Int(stride as i32)),
        ],
    }))
}

/// Draws frame `n`: a gray picture with a white bar that moves.
fn draw(format: u32, n: u64, bytes: &mut [u8]) {
    let bar = (n * 16 % u64::from(WIDTH)) as usize;
    let width = WIDTH as usize;
    let height = HEIGHT as usize;
    match format {
        spa::sys::SPA_VIDEO_FORMAT_YUY2 => {
            for row in bytes.chunks_exact_mut(width * 2).take(height) {
                for (x, pair) in row.chunks_exact_mut(4).enumerate() {
                    let y = if (x * 2).abs_diff(bar) < 40 { 235 } else { 100 };
                    pair.copy_from_slice(&[y, 128, y, 128]);
                }
            }
        }
        _ => {
            let (luma, chroma) = bytes.split_at_mut(width * height);
            for row in luma.chunks_exact_mut(width) {
                for (x, y) in row.iter_mut().enumerate() {
                    *y = if x.abs_diff(bar) < 40 { 235 } else { 100 };
                }
            }
            chroma.fill(128);
        }
    }
}

fn main() -> Result<(), pw::Error> {
    let args: Vec<String> = std::env::args().collect();
    let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let choice = args.get(2).is_some_and(|s| s == "choice");
    let format = match args.get(3).map(String::as_str) {
        Some("NV12") => spa::sys::SPA_VIDEO_FORMAT_NV12,
        Some("YUY2") => spa::sys::SPA_VIDEO_FORMAT_YUY2,
        _ => spa::sys::SPA_VIDEO_FORMAT_I420,
    };

    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Video",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_CLASS => "Video/Source",
        *pw::keys::MEDIA_ROLE => "Camera",
        *pw::keys::NODE_NAME => "farglass-camera",
        *pw::keys::NODE_DESCRIPTION => "farglass-camera",
    };
    let stream = pw::stream::StreamRc::new(core.clone(), "farglass-camera", props)?;
    let frame = Rc::new(Cell::new(0_u64));
    let streaming = Rc::new(Cell::new(false));
    let count = frame.clone();
    let on = streaming.clone();
    let _listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(move |_, _, old, new| {
            eprintln!("state: {old:?} -> {new:?}");
            on.set(matches!(new, pw::stream::StreamState::Streaming));
        })
        .param_changed(move |stream, _, id, param| {
            if id != spa::sys::SPA_PARAM_Format || param.is_none() {
                return;
            }
            eprintln!("format: set");
            let values = buffers(format);
            let mut params = [Pod::from_bytes(&values).expect("the Buffers pod is valid")];
            if let Err(e) = stream.update_params(&mut params) {
                eprintln!("update_params: {e}");
            }
        })
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let (size, stride) = frame_size(format);
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            if let Some(bytes) = data.data() {
                let used = (size as usize).min(bytes.len());
                draw(format, count.get(), &mut bytes[..used]);
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = stride as i32;
                *chunk.size_mut() = used as u32;
            }
            count.set(count.get() + 1);
        })
        .register()?;

    let values = enum_format(format, choice);
    let mut params = [Pod::from_bytes(&values).expect("the EnumFormat pod is valid")];
    stream.connect(
        spa::utils::Direction::Output,
        None,
        pw::stream::StreamFlags::DRIVER | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;

    let tick = mainloop.loop_().add_timer({
        let stream = stream.clone();
        let streaming = streaming.clone();
        move |_| {
            if streaming.get() {
                let _ = stream.trigger_process();
            }
        }
    });
    let interval = Duration::from_micros(1_000_000 / u64::from(FPS));
    let _ = tick.update_timer(Some(interval), Some(interval));
    // The portal connections of host apps, by client id, and their proxies.
    let node_id = Rc::new(Cell::new(u32::MAX));
    let clients: Rc<std::cell::RefCell<Vec<(u32, Rc<pw::client::Client>, pw::client::ClientListener)>>> =
        Rc::default();
    let registry = core.get_registry_rc()?;
    let _registry_listener = if std::env::var("GRANT").is_ok_and(|v| v == "yes") {
        let registry_of = registry.clone();
        let clients = clients.clone();
        let node_id = node_id.clone();
        Some(
            registry
                .add_listener_local()
                .global(move |global| {
                    if global.type_ != pw::types::ObjectType::Client {
                        return;
                    }
                    let Ok(client) = registry_of.bind::<pw::client::Client, _>(global) else {
                        return;
                    };
                    let id = global.id;
                    let node_id = node_id.clone();
                    let client = Rc::new(client);
                    let proxy = client.clone();
                    let listener = client
                        .add_listener_local()
                        .info(move |info| {
                            let Some(props) = info.props() else { return };
                            let host = props.get("pipewire.access.portal.app_id") == Some("");
                            let camera = props
                                .get("pipewire.access.portal.media_roles")
                                .is_some_and(|r| r.split(',').any(|r| r.trim() == "Camera"));
                            if host && camera && node_id.get() != u32::MAX {
                                proxy.update_permissions(&[pw::permissions::Permission::new(
                                    node_id.get(),
                                    pw::permissions::PermissionFlags::R
                                        | pw::permissions::PermissionFlags::W
                                        | pw::permissions::PermissionFlags::X,
                                )]);
                                eprintln!("grant: client {id} gets node {}", node_id.get());
                            }
                        })
                        .register();
                    clients.borrow_mut().push((id, client, listener));
                })
                .register(),
        )
    } else {
        None
    };
    let node_of = stream.clone();
    let id_of = node_id.clone();
    let _id = mainloop.loop_().add_timer(move |_| {
        if id_of.get() == u32::MAX && node_of.node_id() != u32::MAX {
            id_of.set(node_of.node_id());
        }
    });
    let _ = _id.update_timer(Some(Duration::from_millis(10)), Some(Duration::from_millis(100)));

    let stop = mainloop.clone();
    let end = mainloop.loop_().add_timer(move |_| stop.quit());
    let _ = end.update_timer(Some(Duration::from_secs(seconds)), None);
    eprintln!("node: farglass-camera, {}", if choice { "choice" } else { "plain" });
    mainloop.run();
    eprintln!("frames: {}", frame.get());
    Ok(())
}
