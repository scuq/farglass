//! The desktop environment of a web session.
//!
//! A local desktop gives `WAYLAND_DISPLAY` and `XDG_CURRENT_DESKTOP` to the
//! systemd user manager and to D-Bus activation at its start. A program that
//! starts on demand, for example the desktop portal, then finds the display.
//! A web session has no local desktop, thus the agent does it for the
//! compositor of farglass. With no display, such a program waits until its
//! start times out, and each app that calls it waits too.

use std::collections::HashMap;
use std::path::Path;

use farglass_proto::message::Bus;
use zbus::Connection;

use crate::dbus::Buses;

/// The name of the desktop in `XDG_CURRENT_DESKTOP`. The portal reads `farglass-portals.conf` for it.
pub const DESKTOP: &str = "farglass";

/// The number of the X11 display of Xwayland, if the compositor of the session started it (F082).
static X11_DISPLAY: std::sync::OnceLock<u32> = std::sync::OnceLock::new();

/// Sets the number of the X11 display of the session. Only the first call has an effect.
pub fn set_x11_display(number: u32) {
    let _ = X11_DISPLAY.set(number);
}

/// Returns `true` if the session has an X11 display.
#[must_use]
pub fn has_x11() -> bool {
    X11_DISPLAY.get().is_some()
}

/// Returns `true` if the agent must start Xwayland: the program is in `PATH`,
/// and `FARGLASS_XWAYLAND` is not `0`.
#[must_use]
pub fn x11_wanted() -> bool {
    std::env::var_os("FARGLASS_XWAYLAND").is_none_or(|value| value != "0")
        && std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| dir.join("Xwayland").is_file())
        })
}

/// Returns the variables of the desktop for the Wayland socket `socket`,
/// with `DISPLAY` if the session has an X11 display.
/// The sound of PulseAudio and PipeWire clients goes to the sink of the browser (F038).
#[must_use]
pub fn variables(socket: &Path) -> Vec<(String, String)> {
    let mut variables = base_variables(socket);
    if let Some(number) = X11_DISPLAY.get() {
        variables.push(("DISPLAY".to_owned(), format!(":{number}")));
    }
    variables
}

fn base_variables(socket: &Path) -> Vec<(String, String)> {
    vec![
        ("WAYLAND_DISPLAY".to_owned(), socket.display().to_string()),
        ("XDG_SESSION_TYPE".to_owned(), "wayland".to_owned()),
        ("XDG_CURRENT_DESKTOP".to_owned(), DESKTOP.to_owned()),
        ("PULSE_SINK".to_owned(), crate::audio::SINK_NAME.to_owned()),
        (
            "PIPEWIRE_NODE".to_owned(),
            crate::audio::SINK_NAME.to_owned(),
        ),
    ]
}

/// Gives the variables of the desktop to the systemd user manager and to D-Bus activation.
///
/// Returns `false` and changes nothing if the user manager has a display already,
/// for example from a local GNOME session of the same user.
///
/// # Errors
///
/// A D-Bus error if the session bus or the user manager refuses a call.
pub async fn announce(buses: &Buses, socket: &Path) -> zbus::Result<bool> {
    let connection: Connection = buses.connection(Bus::Session).await?;
    let manager = (
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    );
    let reply = connection
        .call_method(
            Some(manager.0),
            manager.1,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(manager.2, "Environment"),
        )
        .await?;
    let value: zbus::OwnedVariant = reply.body().deserialize()?;
    let current: Vec<String> = Vec::<String>::try_from(value).unwrap_or_default();
    if current
        .iter()
        .any(|entry| entry.starts_with("WAYLAND_DISPLAY=") || entry.starts_with("DISPLAY="))
    {
        return Ok(false);
    }
    let variables = variables(socket);
    let assignments: Vec<String> = variables
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    connection
        .call_method(
            Some(manager.0),
            manager.1,
            Some(manager.2),
            "SetEnvironment",
            &(assignments,),
        )
        .await?;
    let map: HashMap<&str, &str> = variables
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "UpdateActivationEnvironment",
            &(map,),
        )
        .await?;
    Ok(true)
}
