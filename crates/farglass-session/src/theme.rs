//! The dark or light theme of the apps of a web session (F120, ADR-059).
//!
//! The shell sends the theme that it shows: dark or light. While the web
//! session runs, the agent sets the settings of the user that GTK, the desktop
//! portal, and KDE read:
//!
//! - GSettings `org.gnome.desktop.interface color-scheme`: libadwaita, GTK4,
//!   and through the portal Qt 6 and the browsers. They follow a change at once.
//! - GSettings `gtk-theme`, `Adwaita` or `Adwaita-dark`, for GTK3, only if the
//!   user has an Adwaita theme. A different theme of the user stays.
//! - The Breeze colors in `~/.config/kdeglobals` for KDE apps, with the signal
//!   `notifyChange` of `KGlobalSettings`, only if Breeze is installed.
//!
//! The web session and the local desktop share these settings, but never run
//! at the same time (ADR-015). Thus the agent saves the values of the user
//! before its first change, and restores them at the end of the web session.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

/// The GSettings schema of the keys.
const SCHEMA: &str = "org.gnome.desktop.interface";

/// The settings that the agent changes. A test gives a fake.
pub trait Backend: Send + Sync {
    /// Returns the value of a key of [`SCHEMA`] as GVariant text, for example `'prefer-dark'`, or `None` if it does not exist.
    fn get(&self, key: &str) -> Option<String>;
    /// Sets a key of [`SCHEMA`] to the GVariant text `value`. Returns `false` if it failed.
    fn set(&self, key: &str, value: &str) -> bool;
    /// Returns the values that a key of the type enum can have.
    fn range(&self, key: &str) -> Vec<String>;
    /// Returns the text of the KDE color scheme `name`, for example `BreezeDark`, if it is installed.
    fn kde_scheme(&self, name: &str) -> Option<String>;
    /// Tells the running KDE apps that the colors changed.
    fn kde_notify(&self);
}

/// What `kdeglobals` was before the first change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "text", rename_all = "kebab-case")]
enum Kde {
    /// The agent did not change it: no Breeze on the host.
    #[default]
    Untouched,
    /// The file did not exist.
    Absent,
    /// The text of the file.
    Text(String),
}

/// The values of the user before the first change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Saved {
    color_scheme: Option<String>,
    gtk_theme: Option<String>,
    #[serde(default)]
    kdeglobals: Kde,
}

/// The theme of the apps of the session.
pub struct Theme {
    backend: Box<dyn Backend>,
    /// The file with the saved values of the user.
    state: PathBuf,
    /// `~/.config/kdeglobals`.
    kdeglobals: PathBuf,
    /// Reports whether the agent follows the shell: only in a web session.
    enabled: AtomicBool,
    /// The theme that the agent set last: `Some(true)` for dark.
    applied: Mutex<Option<bool>>,
}

impl std::fmt::Debug for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Theme")
            .field("state", &self.state)
            .field("enabled", &self.enabled.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// Returns the GVariant text with no quotes, for example `prefer-dark` for `'prefer-dark'`.
fn unquote(value: &str) -> &str {
    value.trim().trim_matches('\'')
}

/// Returns the value of `FARGLASS_FOLLOW_THEME`: `Some(true)` for 1, `Some(false)` for 0.
#[must_use]
pub fn forced_by_env() -> Option<bool> {
    match std::env::var("FARGLASS_FOLLOW_THEME").ok()?.as_str() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

impl Theme {
    /// Returns the theme of this user, with `gsettings` and the files in the home directory. It follows the shell only after [`Theme::set_enabled`].
    #[must_use]
    pub fn new() -> Arc<Self> {
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let state = std::env::var_os("XDG_STATE_HOME")
            .map_or_else(|| home.join(".local/state"), PathBuf::from);
        let config =
            std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home.join(".config"), PathBuf::from);
        Arc::new(Self::with_backend(
            Box::new(System),
            state.join("farglass/theme-restore.json"),
            config.join("kdeglobals"),
        ))
    }

    /// Returns a theme with `backend`, the saved values in `state`, and the KDE settings in `kdeglobals`.
    #[must_use]
    pub fn with_backend(backend: Box<dyn Backend>, state: PathBuf, kdeglobals: PathBuf) -> Self {
        Self {
            backend,
            state,
            kdeglobals,
            enabled: AtomicBool::new(false),
            applied: Mutex::new(None),
        }
    }

    /// Lets the agent follow the theme of the shell, or not.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }

    /// Reports whether the agent follows the theme of the shell.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<bool>> {
        self.applied.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets the theme of the apps: dark or light. The first call saves the values of the user.
    /// It blocks for the `gsettings` calls: call it on a thread for blocking work.
    pub fn apply(&self, dark: bool) {
        if !self.enabled() {
            return;
        }
        let mut applied = self.lock();
        if *applied == Some(dark) {
            return;
        }
        let saved = match self.saved() {
            Some(saved) => saved,
            None => {
                let saved = self.capture();
                if let Err(error) = self.store(&saved) {
                    // With no saved values, no change: the agent could not put them back.
                    tracing::warn!(%error, "the theme of the apps stays, the agent cannot save the values of the user");
                    return;
                }
                saved
            }
        };
        if self.backend.get("color-scheme").is_some() {
            let light = if self
                .backend
                .range("color-scheme")
                .iter()
                .any(|v| v == "prefer-light")
            {
                "prefer-light"
            } else {
                "default"
            };
            self.backend
                .set("color-scheme", if dark { "prefer-dark" } else { light });
        }
        if saved
            .gtk_theme
            .as_deref()
            .is_some_and(|theme| matches!(unquote(theme), "Adwaita" | "Adwaita-dark"))
        {
            self.backend
                .set("gtk-theme", if dark { "Adwaita-dark" } else { "Adwaita" });
        }
        if saved.kdeglobals != Kde::Untouched {
            let (name, scheme) = if dark {
                ("BreezeDark", self.backend.kde_scheme("BreezeDark"))
            } else {
                self.backend
                    .kde_scheme("BreezeLight")
                    .map_or(("Breeze", self.backend.kde_scheme("Breeze")), |text| {
                        ("BreezeLight", Some(text))
                    })
            };
            if let Some(scheme) = scheme {
                let before = std::fs::read_to_string(&self.kdeglobals).unwrap_or_default();
                if let Err(error) = write(&self.kdeglobals, &merge_scheme(&before, &scheme, name)) {
                    tracing::warn!(%error, "the KDE apps keep their colors");
                } else {
                    self.backend.kde_notify();
                }
            }
        }
        *applied = Some(dark);
        tracing::info!(
            dark,
            "the apps of the session follow the theme of the shell"
        );
    }

    /// Puts back the values of the user and deletes the saved file. With no saved file, it changes nothing.
    pub fn restore(&self) {
        let mut applied = self.lock();
        let Some(saved) = self.saved() else {
            return;
        };
        if let Some(value) = &saved.color_scheme {
            self.backend.set("color-scheme", value);
        }
        if let Some(value) = &saved.gtk_theme {
            self.backend.set("gtk-theme", value);
        }
        match &saved.kdeglobals {
            Kde::Untouched => {}
            Kde::Absent => {
                let _ = std::fs::remove_file(&self.kdeglobals);
                self.backend.kde_notify();
            }
            Kde::Text(text) => {
                if let Err(error) = write(&self.kdeglobals, text) {
                    tracing::warn!(%error, "the KDE colors of the user did not come back");
                }
                self.backend.kde_notify();
            }
        }
        if let Err(error) = std::fs::remove_file(&self.state) {
            tracing::warn!(%error, "the saved theme values stay");
        }
        *applied = None;
        tracing::info!("the theme of the user is back");
    }

    /// Returns the values of the user now.
    fn capture(&self) -> Saved {
        let kde = self.backend.kde_scheme("BreezeDark").is_some();
        Saved {
            color_scheme: self.backend.get("color-scheme"),
            gtk_theme: self.backend.get("gtk-theme"),
            kdeglobals: if !kde {
                Kde::Untouched
            } else {
                match std::fs::read_to_string(&self.kdeglobals) {
                    Ok(text) => Kde::Text(text),
                    Err(_) => Kde::Absent,
                }
            },
        }
    }

    /// Returns the saved values, if the file is there.
    fn saved(&self) -> Option<Saved> {
        let text = std::fs::read_to_string(&self.state).ok()?;
        serde_json::from_str(&text)
            .map_err(|error| tracing::warn!(%error, "the saved theme values are not valid"))
            .ok()
    }

    fn store(&self, saved: &Saved) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(saved).map_err(std::io::Error::other)?;
        write(&self.state, &text)
    }
}

/// Writes `text` to `path` through a file next to it, so a crash leaves the old or the new text.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("farglass-new");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

/// One section of an INI file: its header line (empty for the lines before the first header) and its lines.
type Section = (String, Vec<String>);

fn sections(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = vec![(String::new(), Vec::new())];
    for line in text.lines() {
        if line.trim_start().starts_with('[') {
            out.push((line.trim().to_owned(), Vec::new()));
        } else if let Some(last) = out.last_mut() {
            last.1.push(line.to_owned());
        }
    }
    out
}

/// Returns `kdeglobals` with the colors of the KDE color scheme `scheme` and `ColorScheme=name` in `[General]`.
/// The other sections and keys of the user stay.
fn merge_scheme(kdeglobals: &str, scheme: &str, name: &str) -> String {
    let mut out = sections(kdeglobals);
    for (header, lines) in sections(scheme) {
        let colors = header.starts_with("[Colors:")
            || header.starts_with("[ColorEffects:")
            || header == "[WM]";
        if !colors {
            continue;
        }
        match out.iter_mut().find(|(h, _)| *h == header) {
            Some(section) => section.1 = lines,
            None => out.push((header, lines)),
        }
    }
    let general = match out.iter().position(|(h, _)| h == "[General]") {
        Some(at) => at,
        None => {
            out.push(("[General]".to_owned(), Vec::new()));
            out.len() - 1
        }
    };
    let lines = &mut out[general].1;
    let entry = format!("ColorScheme={name}");
    match lines
        .iter_mut()
        .find(|line| line.trim_start().starts_with("ColorScheme="))
    {
        Some(line) => *line = entry,
        None => lines.insert(0, entry),
    }
    let mut text = String::new();
    for (header, lines) in out {
        if !header.is_empty() {
            if !text.is_empty() && !text.ends_with("\n\n") {
                text.push('\n');
            }
            text.push_str(&header);
            text.push('\n');
        }
        for line in lines.iter().filter(|line| !line.trim().is_empty()) {
            text.push_str(line);
            text.push('\n');
        }
    }
    text
}

/// The settings of the host: `gsettings`, the color schemes in the data directories, and `gdbus`.
struct System;

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

impl Backend for System {
    fn get(&self, key: &str) -> Option<String> {
        run("gsettings", &["get", SCHEMA, key])
    }

    fn set(&self, key: &str, value: &str) -> bool {
        run("gsettings", &["set", SCHEMA, key, value]).is_some()
    }

    fn range(&self, key: &str) -> Vec<String> {
        run("gsettings", &["range", SCHEMA, key])
            .map(|text| {
                text.lines()
                    .skip(1)
                    .map(|v| unquote(v).to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn kde_scheme(&self, name: &str) -> Option<String> {
        let dirs = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
        dirs.split(':')
            .filter(|dir| !dir.is_empty())
            .find_map(|dir| {
                std::fs::read_to_string(
                    Path::new(dir)
                        .join("color-schemes")
                        .join(format!("{name}.colors")),
                )
                .ok()
            })
    }

    fn kde_notify(&self) {
        // KGlobalSettings::PaletteChanged is 0.
        let _ = run(
            "gdbus",
            &[
                "emit",
                "--session",
                "--object-path",
                "/KGlobalSettings",
                "--signal",
                "org.kde.KGlobalSettings.notifyChange",
                "0",
                "0",
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicUsize;

    use super::*;

    /// GSettings in memory, and Breeze if `kde`.
    struct Fake {
        values: Mutex<HashMap<String, String>>,
        kde: bool,
        notified: Arc<AtomicUsize>,
    }

    impl Backend for Fake {
        fn get(&self, key: &str) -> Option<String> {
            self.values.lock().unwrap().get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) -> bool {
            let mut values = self.values.lock().unwrap();
            if !values.contains_key(key) {
                return false;
            }
            values.insert(key.to_owned(), format!("'{}'", unquote(value)));
            true
        }
        fn range(&self, _key: &str) -> Vec<String> {
            vec![
                "default".into(),
                "prefer-dark".into(),
                "prefer-light".into(),
            ]
        }
        fn kde_scheme(&self, name: &str) -> Option<String> {
            self.kde.then(|| {
                format!("[ColorEffects:Disabled]\nIntensityEffect=2\n\n[Colors:Window]\nBackgroundNormal={name}\n\n[General]\nName={name}\n\n[WM]\nactiveBackground={name}\n")
            })
        }
        fn kde_notify(&self) {
            self.notified.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn theme(dir: &Path, gtk_theme: &str, kde: bool) -> (Theme, Arc<AtomicUsize>) {
        let notified = Arc::new(AtomicUsize::new(0));
        let values = HashMap::from([
            ("color-scheme".to_owned(), "'default'".to_owned()),
            ("gtk-theme".to_owned(), format!("'{gtk_theme}'")),
        ]);
        let fake = Fake {
            values: Mutex::new(values),
            kde,
            notified: notified.clone(),
        };
        let theme = Theme::with_backend(
            Box::new(fake),
            dir.join("state/theme-restore.json"),
            dir.join("kdeglobals"),
        );
        theme.set_enabled(true);
        (theme, notified)
    }

    fn value(theme: &Theme, key: &str) -> String {
        theme.backend.get(key).unwrap()
    }

    #[test]
    fn the_apps_follow_the_shell_and_the_values_of_the_user_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let (theme, _) = theme(dir.path(), "Adwaita", false);
        theme.apply(true);
        assert_eq!(value(&theme, "color-scheme"), "'prefer-dark'");
        assert_eq!(value(&theme, "gtk-theme"), "'Adwaita-dark'");
        theme.apply(false);
        assert_eq!(value(&theme, "color-scheme"), "'prefer-light'");
        assert_eq!(value(&theme, "gtk-theme"), "'Adwaita'");
        theme.apply(true);
        // The saved values are the ones before the first change.
        let saved: Saved =
            serde_json::from_str(&std::fs::read_to_string(&theme.state).unwrap()).unwrap();
        assert_eq!(saved.color_scheme.as_deref(), Some("'default'"));
        assert_eq!(saved.gtk_theme.as_deref(), Some("'Adwaita'"));
        theme.restore();
        assert_eq!(value(&theme, "color-scheme"), "'default'");
        assert_eq!(value(&theme, "gtk-theme"), "'Adwaita'");
        assert!(!theme.state.exists());
    }

    #[test]
    fn a_theme_of_the_user_that_is_not_adwaita_stays() {
        let dir = tempfile::tempdir().unwrap();
        let (theme, _) = theme(dir.path(), "Arc-Darker", false);
        theme.apply(true);
        assert_eq!(value(&theme, "color-scheme"), "'prefer-dark'");
        assert_eq!(value(&theme, "gtk-theme"), "'Arc-Darker'");
    }

    #[test]
    fn the_agent_changes_nothing_outside_a_web_session() {
        let dir = tempfile::tempdir().unwrap();
        let (theme, _) = theme(dir.path(), "Adwaita", true);
        theme.set_enabled(false);
        theme.apply(true);
        assert_eq!(value(&theme, "color-scheme"), "'default'");
        assert!(!theme.state.exists());
        assert!(!dir.path().join("kdeglobals").exists());
    }

    #[test]
    fn the_saved_values_of_a_crash_stay_the_original_ones() {
        let dir = tempfile::tempdir().unwrap();
        let (theme, _) = theme(dir.path(), "Adwaita", false);
        theme.apply(true);
        // The agent stops with no restore. The next agent sees the dark values, but keeps the saved ones.
        let (next, _) = self::theme(dir.path(), "Adwaita-dark", false);
        next.backend.set("color-scheme", "prefer-dark");
        next.apply(false);
        next.restore();
        assert_eq!(value(&next, "color-scheme"), "'default'");
        assert_eq!(value(&next, "gtk-theme"), "'Adwaita'");
    }

    #[test]
    fn kde_gets_the_breeze_colors_and_the_file_of_the_user_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let kdeglobals = dir.path().join("kdeglobals");
        std::fs::write(
            &kdeglobals,
            "[General]\nfont=Noto Sans,10\n\n[KDE]\nSingleClick=false\n",
        )
        .unwrap();
        let (theme, notified) = theme(dir.path(), "Adwaita", true);
        theme.apply(true);
        let text = std::fs::read_to_string(&kdeglobals).unwrap();
        assert!(text.contains("ColorScheme=BreezeDark"), "{text}");
        assert!(text.contains("BackgroundNormal=BreezeDark"), "{text}");
        assert!(text.contains("activeBackground=BreezeDark"), "{text}");
        assert!(
            text.contains("font=Noto Sans,10") && text.contains("SingleClick=false"),
            "{text}"
        );
        assert!(
            !text.contains("Name=BreezeDark"),
            "the [General] keys of the scheme stay out: {text}"
        );
        theme.apply(false);
        assert!(
            std::fs::read_to_string(&kdeglobals)
                .unwrap()
                .contains("ColorScheme=BreezeLight")
        );
        theme.restore();
        assert_eq!(
            std::fs::read_to_string(&kdeglobals).unwrap(),
            "[General]\nfont=Noto Sans,10\n\n[KDE]\nSingleClick=false\n"
        );
        assert_eq!(notified.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn a_kdeglobals_that_was_not_there_goes_at_the_restore() {
        let dir = tempfile::tempdir().unwrap();
        let (theme, _) = theme(dir.path(), "Adwaita", true);
        theme.apply(true);
        assert!(dir.path().join("kdeglobals").exists());
        theme.restore();
        assert!(!dir.path().join("kdeglobals").exists());
    }
}
