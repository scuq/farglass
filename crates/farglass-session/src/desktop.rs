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
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
struct HostConfig {
    x11_apps: bool,
    ssh_agent: bool,
    ssh_agent_clear_on_lock: bool,
    microphone: bool,
    camera: bool,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            x11_apps: false,
            ssh_agent: true,
            ssh_agent_clear_on_lock: true,
            microphone: true,
            camera: true,
        }
    }
}

/// Returns the options of the agent in the text of a configuration file.
///
/// # Errors
///
/// A text with the cause if the text is not valid TOML or an option has the wrong type.
fn host_config_in(text: &str) -> Result<HostConfig, String> {
    toml::from_str::<HostConfig>(text).map_err(|error| error.to_string())
}

/// Returns the options of the agent in the configuration file, read one time. A missing file
/// gives the defaults. A file that does not load or is not valid turns X11 apps off and keeps the
/// other defaults, with a warning.
fn host_config() -> &'static HostConfig {
    static CONFIG_OF_HOST: std::sync::OnceLock<HostConfig> = std::sync::OnceLock::new();
    CONFIG_OF_HOST.get_or_init(|| {
        let path =
            std::env::var_os("FARGLASSD_CONFIG").map_or_else(|| CONFIG.into(), PathBuf::from);
        match std::fs::read_to_string(&path) {
            Ok(text) => host_config_in(&text).unwrap_or_else(|error| {
                tracing::warn!(path = %path.display(), %error, "the configuration file is not valid, X11 apps are off");
                HostConfig::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HostConfig::default(),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "the configuration file did not load, X11 apps are off");
                HostConfig::default()
            }
        }
    })
}

/// Returns `x11_apps` of the configuration file, off by default.
fn x11_apps_configured() -> bool {
    host_config().x11_apps
}

/// Returns `true` if the agent must start an ssh-agent for the session (F093).
///
/// `ssh_agent` in the configuration file of the host decides, and is on by default.
/// `FARGLASS_SSH_AGENT=1` or `0` in the environment of the agent overrides it, for tests.
#[must_use]
pub fn ssh_agent_wanted() -> bool {
    match std::env::var("FARGLASS_SSH_AGENT").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => host_config().ssh_agent,
    }
}

/// Returns `ssh_agent_clear_on_lock` of the configuration file, on by default (F093).
#[must_use]
pub fn ssh_agent_clear_on_lock() -> bool {
    host_config().ssh_agent_clear_on_lock
}

/// Returns `true` if the apps of the session can use the microphone of the client (F114).
///
/// `microphone` in the configuration file of the host decides, and is on by default.
#[must_use]
pub fn microphone_allowed() -> bool {
    host_config().microphone
}

/// Returns `true` if the apps of the session can use the camera of the client (F116).
///
/// `camera` in the configuration file of the host decides, and is on by default.
#[must_use]
pub fn camera_allowed() -> bool {
    host_config().camera
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
/// with `DISPLAY` if the session has an X11 display, and `SSH_AUTH_SOCK` if it has an ssh-agent.
/// The sound of PulseAudio and PipeWire clients goes to the sink of the browser (F038).
/// PulseAudio clients record from the microphone source of the browser (F114).
#[must_use]
pub fn variables(socket: &Path) -> Vec<(String, String)> {
    let mut variables = base_variables(socket);
    if let Some(number) = X11_DISPLAY.get() {
        variables.push(("DISPLAY".to_owned(), format!(":{number}")));
    }
    if let Some(agent) = crate::ssh_agent::socket() {
        variables.push(("SSH_AUTH_SOCK".to_owned(), agent.display().to_string()));
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
            "PULSE_SOURCE".to_owned(),
            crate::mic::SOURCE_NAME.to_owned(),
        ),
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
        let x11_apps_in = |text: &str| host_config_in(text).map(|config| config.x11_apps);
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
        assert_eq!(host_config_in(packaged), Ok(HostConfig::default()));
    }

    #[test]
    fn the_ssh_agent_is_on_and_clears_on_lock_unless_the_file_turns_it_off() {
        assert_eq!(host_config_in(""), Ok(HostConfig::default()));
        assert!(HostConfig::default().ssh_agent && HostConfig::default().ssh_agent_clear_on_lock);
        let config =
            host_config_in("ssh_agent = false\nssh_agent_clear_on_lock = false\n").unwrap();
        assert!(!config.ssh_agent && !config.ssh_agent_clear_on_lock);
        assert!(host_config_in("ssh_agent = 1\n").is_err());
    }

    #[test]
    fn the_microphone_is_on_unless_the_file_turns_it_off() {
        assert!(HostConfig::default().microphone);
        assert!(!host_config_in("microphone = false\n").unwrap().microphone);
        assert!(host_config_in("microphone = \"no\"\n").is_err());
    }

    #[test]
    fn the_camera_is_on_unless_the_file_turns_it_off() {
        assert!(HostConfig::default().camera);
        assert!(!host_config_in("camera = false\n").unwrap().camera);
        assert!(host_config_in("camera = 1\n").is_err());
    }
}
