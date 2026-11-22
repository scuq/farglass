//! The desktop environment of a web session.
//!
//! A local desktop gives `WAYLAND_DISPLAY` and `XDG_CURRENT_DESKTOP` to the
//! systemd user manager and to D-Bus activation at its start. A program that
//! starts on demand, for example the desktop portal, then finds the display.
//! A web session has no local desktop, thus the agent does it for the
//! compositor of farglass. With no display, such a program waits until its
//! start times out, and each app that calls it waits too.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// The configuration file of the host. `FARGLASSD_CONFIG` changes it, as for farglassd.
const CONFIG: &str = "/etc/farglass/farglassd.conf";

/// The part of the configuration file that the agent reads. The other options are for farglassd.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct HostConfig {
    x11_apps: bool,
}

/// Returns the value of `x11_apps` in the text of a configuration file.
///
/// # Errors
///
/// A text with the cause if the text is not valid TOML or `x11_apps` is not a boolean.
fn x11_apps_in(text: &str) -> Result<bool, String> {
    toml::from_str::<HostConfig>(text)
        .map(|config| config.x11_apps)
        .map_err(|error| error.to_string())
}

/// Returns `x11_apps` of the configuration file. A missing file gives the default, off.
/// A file that does not load or is not valid also turns X11 apps off, with a warning.
fn x11_apps_configured() -> bool {
    let path = std::env::var_os("FARGLASSD_CONFIG").map_or_else(|| CONFIG.into(), PathBuf::from);
    match std::fs::read_to_string(&path) {
        Ok(text) => x11_apps_in(&text).unwrap_or_else(|error| {
            tracing::warn!(path = %path.display(), %error, "the configuration file is not valid, X11 apps are off");
            false
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "the configuration file did not load, X11 apps are off");
            false
        }
    }
}

/// Returns `true` if the agent must start Xwayland (F083).
///
/// `x11_apps` in the configuration file of the host decides, and is off by default.
/// `FARGLASS_XWAYLAND=1` or `0` in the environment of the agent overrides it, for tests.
/// The program `Xwayland` must also be in `PATH`.
#[must_use]
pub fn x11_wanted() -> bool {
    let wanted = match std::env::var("FARGLASS_XWAYLAND").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => x11_apps_configured(),
    };
    if !wanted {
        tracing::info!("X11 apps are off: x11_apps in the configuration file is not true");
        return false;
    }
    let installed = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join("Xwayland").is_file()));
    if !installed {
        tracing::warn!("x11_apps is true, but Xwayland is not installed: X11 apps do not show");
    }
    installed
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_apps_are_off_unless_the_file_turns_them_on() {
        assert_eq!(x11_apps_in(""), Ok(false));
        assert_eq!(
            x11_apps_in("#x11_apps = true\nidle_lock_minutes = 5\n"),
            Ok(false)
        );
        assert_eq!(
            x11_apps_in("x11_apps = true\n[tls]\ncert = \"/etc/farglass/tls/a.pem\"\n"),
            Ok(true)
        );
        assert!(x11_apps_in("x11_apps = \"yes\"\n").is_err());
        assert!(x11_apps_in("x11_apps = \n").is_err());
        let packaged = include_str!("../../../packaging/farglassd.conf");
        assert_eq!(x11_apps_in(packaged), Ok(false));
    }
}
