//! The apps of the host for the launcher of the shell (F031).
//!
//! The agent reads the desktop entries of the XDG data directories, as a local
//! desktop does: `applications/*.desktop` in `XDG_DATA_HOME`, then in each
//! directory of `XDG_DATA_DIRS`. The first entry of an id wins, thus an entry
//! of the user replaces an entry of the system. The agent finds the icon of
//! each app in the hicolor theme or in `pixmaps`, and starts an app with the
//! variables of the desktop, each app in its own systemd --user scope.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};

use farglass_proto::message::HostApp;

/// The name of the desktop in `OnlyShowIn` and `NotShowIn`.
const DESKTOP: &str = crate::desktop::DESKTOP;

/// The largest icon in bytes: an icon goes to the shell in one data frame.
pub const MAX_ICON: u64 = 512 * 1024;

/// The sizes of the hicolor theme, in the order of preference for a launcher at 32 to 64 pixels.
const ICON_SIZES: [&str; 9] = [
    "scalable", "256x256", "128x128", "96x96", "64x64", "512x512", "48x48", "32x32", "24x24",
];

/// A desktop entry of an app that the launcher shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopApp {
    /// The desktop file id, for example `org.gnome.Calculator`.
    pub id: String,
    /// The name in the language of the user.
    pub name: String,
    /// The comment, or the generic name.
    pub comment: Option<String>,
    /// The keywords and the generic name.
    pub keywords: Vec<String>,
    /// The icon: a name in the icon theme or an absolute path.
    pub icon: Option<String>,
    /// The command line, with the field codes.
    pub exec: String,
    /// The working directory.
    pub path: Option<PathBuf>,
    /// The `StartupWMClass`.
    pub wm_class: Option<String>,
    /// The desktop file.
    pub file: PathBuf,
}

impl DesktopApp {
    /// Returns the facts for the shell. `icon` reports whether the agent found an icon file.
    #[must_use]
    pub fn host_app(&self, icon: bool) -> HostApp {
        let mut names = vec![self.id.to_lowercase()];
        if let Some(class) = &self.wm_class {
            names.push(class.to_lowercase());
        }
        if let Some(program) = exec_args(self).ok().and_then(|args| program_name(&args)) {
            names.push(program);
        }
        names.dedup();
        HostApp {
            id: self.id.clone(),
            name: self.name.clone(),
            comment: self.comment.clone(),
            keywords: self.keywords.clone(),
            icon,
            names,
        }
    }
}

/// Returns the name of the program of a command line in lowercase. For `env NAME=value program`, it is the name of `program`.
fn program_name(args: &[String]) -> Option<String> {
    let base = |arg: &String| {
        Path::new(arg)
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
    };
    let first = base(args.first()?)?;
    if first != "env" {
        return Some(first);
    }
    // The options of env that take a value: -u NAME and -C DIR.
    let mut rest = args[1..].iter();
    while let Some(arg) = rest.next() {
        if arg == "-u" || arg == "-C" || arg == "--unset" || arg == "--chdir" {
            rest.next();
        } else if !arg.contains('=') && !arg.starts_with('-') {
            return base(arg);
        }
    }
    None
}

/// An error of an app start.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    /// The id is not an app of the launcher.
    #[error("The app {0} is not installed.")]
    Unknown(String),
    /// The `Exec` key has a quote with no end, or no program.
    #[error("The command line of {0} is not valid.")]
    Exec(String),
    /// The program did not start.
    #[error("{name} did not start: {source}.")]
    Spawn {
        /// The name of the app.
        name: String,
        /// The cause.
        source: io::Error,
    },
}

/// Returns the XDG data directories: `XDG_DATA_HOME`, then `XDG_DATA_DIRS`, with the defaults of the specification.
#[must_use]
pub fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    home.into_iter()
        .chain(
            system
                .split(':')
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
        )
        .collect()
}

/// Returns the language names to try for a localized key, the best first, from `LC_ALL`, `LC_MESSAGES`, or `LANG`.
#[must_use]
pub fn languages() -> Vec<String> {
    let value = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default();
    languages_of(&value)
}

/// Returns the language names to try for the locale `value`, for example `de_AT.UTF-8@euro`.
fn languages_of(value: &str) -> Vec<String> {
    let (base, modifier) = value.split_once('@').unwrap_or((value, ""));
    let base = base.split_once('.').map_or(base, |(base, _)| base);
    if base.is_empty() || base == "C" || base == "POSIX" {
        return Vec::new();
    }
    let lang = base.split_once('_').map_or(base, |(lang, _)| lang);
    let mut names = Vec::new();
    if !modifier.is_empty() {
        names.push(format!("{base}@{modifier}"));
    }
    names.push(base.to_owned());
    if !modifier.is_empty() {
        names.push(format!("{lang}@{modifier}"));
    }
    names.push(lang.to_owned());
    names.dedup();
    names
}

/// Removes the escapes of a string value: `\s`, `\n`, `\t`, `\r`, and `\\`.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => {
                // `\;` stays for the split of a list.
                if other != '\\' {
                    out.push('\\');
                }
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Splits a list value at each `;` that has no backslash before it.
fn list(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(';') => current.push(';'),
                Some(other) => {
                    current.push('\\');
                    current.push(other);
                }
                None => current.push('\\'),
            },
            ';' => items.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    items.push(current);
    items
        .into_iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Reports whether `program` is an executable file: an absolute path, or a name in `PATH`.
fn executable(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    let is_exec = |path: &Path| {
        path.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        return is_exec(Path::new(program));
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| is_exec(&dir.join(program))))
}

/// Parses a desktop file. `languages` selects the localized keys, the best first.
/// Returns `None` for an entry that the launcher does not show. Such an entry still hides an entry with the same id in a later directory.
#[must_use]
pub fn parse(id: &str, text: &str, file: &Path, languages: &[String]) -> Option<DesktopApp> {
    let mut keys: HashMap<&str, &str> = HashMap::new();
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_entry = line.trim_end() == "[Desktop Entry]";
            continue;
        }
        if in_entry && let Some((key, value)) = line.split_once('=') {
            keys.entry(key.trim()).or_insert(value.trim());
        }
    }
    let localized = |key: &str| {
        languages
            .iter()
            .find_map(|lang| keys.get(format!("{key}[{lang}]").as_str()).copied())
            .or_else(|| keys.get(key).copied())
            .map(unescape)
    };
    let flag = |key: &str| keys.get(key).is_some_and(|value| *value == "true");
    let shown_here = keys
        .get("OnlyShowIn")
        .is_none_or(|value| list(value).iter().any(|d| d == DESKTOP))
        && !keys
            .get("NotShowIn")
            .is_some_and(|value| list(value).iter().any(|d| d == DESKTOP));
    let try_exec = keys
        .get("TryExec")
        .is_none_or(|program| executable(&unescape(program)));
    // An app for a terminal needs a terminal emulator of the host. The launcher does not show it.
    if keys.get("Type") != Some(&"Application")
        || flag("NoDisplay")
        || flag("Hidden")
        || flag("Terminal")
        || !shown_here
        || !try_exec
    {
        return None;
    }
    let (Some(name), Some(exec)) = (localized("Name"), keys.get("Exec").map(|v| unescape(v)))
    else {
        return None;
    };
    let generic = localized("GenericName");
    let mut keywords = localized("Keywords").map(|v| list(&v)).unwrap_or_default();
    if let Some(generic) = &generic {
        keywords.push(generic.clone());
    }
    Some(DesktopApp {
        id: id.to_owned(),
        name,
        comment: localized("Comment").or(generic),
        keywords,
        icon: localized("Icon").filter(|icon| !icon.is_empty()),
        exec,
        path: keys
            .get("Path")
            .map(|path| PathBuf::from(unescape(path)))
            .filter(|path| !path.as_os_str().is_empty()),
        wm_class: keys.get("StartupWMClass").map(|v| unescape(v)),
        file: file.to_owned(),
    })
}

/// Adds the desktop files of `dir` and its subdirectories to `found`, with the id prefix `prefix`.
fn walk(dir: &Path, prefix: &str, found: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            walk(&path, &format!("{prefix}{name}-"), found);
        } else if let Some(stem) = name.strip_suffix(".desktop") {
            found.push((format!("{prefix}{stem}"), path));
        }
    }
}

/// Returns the apps of the desktop entries in `dirs`, sorted by name.
#[must_use]
pub fn scan(dirs: &[PathBuf], languages: &[String]) -> Vec<DesktopApp> {
    let mut seen: HashMap<String, Option<DesktopApp>> = HashMap::new();
    for dir in dirs {
        let mut files = Vec::new();
        walk(&dir.join("applications"), "", &mut files);
        for (id, file) in files {
            if seen.contains_key(&id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let app = parse(&id, &text, &file, languages);
            seen.insert(id, app);
        }
    }
    let mut apps: Vec<DesktopApp> = seen.into_values().flatten().collect();
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    apps
}

/// Returns the mime type of an icon file from its extension: SVG or PNG only.
fn icon_mime(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("svg") => Some("image/svg+xml"),
        Some("png") => Some("image/png"),
        _ => None,
    }
}

/// Reports whether `path` is a file of at most [`MAX_ICON`] bytes.
fn icon_usable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_ICON)
}

/// The icon themes that [`find_tray_icon`] reads first, after hicolor.
const TRAY_THEMES: [&str; 6] = [
    "Adwaita",
    "AdwaitaLegacy",
    "breeze",
    "Papirus",
    "gnome",
    "oxygen",
];

/// The largest number of icon themes that [`find_tray_icon`] reads in one data directory.
const MAX_TRAY_THEMES: usize = 24;

/// The sizes of the status icons of a theme, the best for the panel first.
const TRAY_SIZES: [&str; 10] = [
    "scalable", "48x48", "64x64", "32x32", "24x24", "22x22", "96x96", "128x128", "256x256", "16x16",
];

/// Returns the names of the icon themes in `icons`: the usual themes first, then the others by name.
/// The hicolor theme is not in the list.
fn tray_themes(icons: &Path) -> Vec<String> {
    let mut others: Vec<String> = std::fs::read_dir(icons)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name != "hicolor" && !TRAY_THEMES.contains(&name.as_str()))
        .collect();
    others.sort();
    TRAY_THEMES
        .iter()
        .filter(|name| icons.join(name).is_dir())
        .map(|name| (*name).to_owned())
        .chain(others)
        .take(MAX_TRAY_THEMES)
        .collect()
}

/// Returns the file of the icon `icon` of a tray item and its mime type (F102).
///
/// `theme_path` is the `IconThemePath` of the item, or empty. The order is:
/// the directory `theme_path`, the hicolor theme in `theme_path`, the `status`
/// icons of the hicolor theme in `dirs`, [`find_icon`], then the `status`
/// icons of the other themes in `dirs`. The agent does not know the icon theme
/// of the user, because the web session has no desktop settings.
#[must_use]
pub fn find_tray_icon(
    dirs: &[PathBuf],
    theme_path: &str,
    icon: &str,
) -> Option<(PathBuf, &'static str)> {
    if icon.is_empty() {
        return None;
    }
    if icon.starts_with('/') {
        return find_icon(dirs, icon);
    }
    if icon.contains('/') || icon.starts_with('.') {
        return None;
    }
    let named = |base: PathBuf| {
        ["svg", "png"]
            .map(|extension| base.join(format!("{icon}.{extension}")))
            .into_iter()
            .find(|path| icon_usable(path))
            .and_then(|path| icon_mime(&path).map(|mime| (path, mime)))
    };
    let theme = Path::new(theme_path);
    if theme.is_absolute() {
        if let Some(found) = named(theme.to_path_buf()) {
            return Some(found);
        }
        for size in ICON_SIZES {
            for kind in ["status", "apps"] {
                if let Some(found) = named(theme.join("hicolor").join(size).join(kind)) {
                    return Some(found);
                }
            }
        }
    }
    for size in ICON_SIZES {
        for dir in dirs {
            if let Some(found) = named(dir.join("icons/hicolor").join(size).join("status")) {
                return Some(found);
            }
        }
    }
    if let Some(found) = find_icon(dirs, icon) {
        return Some(found);
    }
    for dir in dirs {
        let icons = dir.join("icons");
        for theme in tray_themes(&icons) {
            let theme = icons.join(theme);
            for size in TRAY_SIZES {
                // The usual layout is size/status. The breeze theme has status/size, with the number only.
                let number = size.split('x').next().unwrap_or(size);
                for base in [
                    theme.join(size).join("status"),
                    theme.join("status").join(number),
                ] {
                    if let Some(found) = named(base) {
                        return Some(found);
                    }
                }
            }
        }
    }
    None
}

/// Returns the file of the icon `icon` and its mime type: an absolute path, the hicolor theme, or `pixmaps`.
#[must_use]
pub fn find_icon(dirs: &[PathBuf], icon: &str) -> Option<(PathBuf, &'static str)> {
    let mime = icon_mime;
    let usable = icon_usable;
    if icon.starts_with('/') {
        let path = PathBuf::from(icon);
        return mime(&path)
            .filter(|_| usable(&path))
            .map(|mime| (path, mime));
    }
    if icon.contains('/') {
        return None;
    }
    for size in ICON_SIZES {
        for dir in dirs {
            let base = dir.join("icons/hicolor").join(size).join("apps");
            for extension in ["svg", "png"] {
                let path = base.join(format!("{icon}.{extension}"));
                if usable(&path)
                    && let Some(mime) = mime(&path)
                {
                    return Some((path, mime));
                }
            }
        }
    }
    dirs.iter()
        .flat_map(|dir| {
            ["svg", "png"].map(|extension| dir.join("pixmaps").join(format!("{icon}.{extension}")))
        })
        .find(|path| usable(path))
        .and_then(|path| mime(&path).map(|mime| (path, mime)))
}

/// Splits the `Exec` value of `app` into arguments and expands the field codes.
/// The launcher gives no files, thus `%f`, `%F`, `%u`, and `%U` go.
///
/// # Errors
///
/// [`LaunchError::Exec`] if a quote has no end or the line has no program.
pub fn exec_args(app: &DesktopApp) -> Result<Vec<String>, LaunchError> {
    exec_args_with(app, None)
}

/// As [`exec_args`], but `files` go at the place of the first `%f`, `%F`, `%u`, or `%U`,
/// or at the end if the line has none of them. A bookmark gives its options and its URL there (F100).
///
/// # Errors
///
/// [`LaunchError::Exec`] if a quote has no end or the line has no program.
pub fn exec_args_with(
    app: &DesktopApp,
    mut files: Option<&[String]>,
) -> Result<Vec<String>, LaunchError> {
    let invalid = || LaunchError::Exec(app.name.clone());
    // The words: the text and a flag that reports whether a part of it was in quotes.
    let mut words: Vec<(String, bool)> = Vec::new();
    let mut current: Option<(String, bool)> = None;
    let mut chars = app.exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let word = current.get_or_insert_with(|| (String::new(), true));
                word.1 = true;
                loop {
                    match chars.next().ok_or_else(invalid)? {
                        '"' => break,
                        '\\' => word.0.push(chars.next().ok_or_else(invalid)?),
                        other => word.0.push(other),
                    }
                }
            }
            ' ' | '\t' | '\n' => {
                if let Some(word) = current.take() {
                    words.push(word);
                }
            }
            other => current
                .get_or_insert_with(|| (String::new(), false))
                .0
                .push(other),
        }
    }
    if let Some(word) = current.take() {
        words.push(word);
    }
    let mut args = Vec::new();
    for (word, quoted) in words {
        if !quoted {
            match word.as_str() {
                "%f" | "%F" | "%u" | "%U" => {
                    // A file is never the program.
                    if args.is_empty() {
                        return Err(invalid());
                    }
                    args.extend(files.take().unwrap_or_default().iter().cloned());
                    continue;
                }
                "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => continue,
                "%i" => {
                    if let Some(icon) = &app.icon {
                        args.push("--icon".to_owned());
                        args.push(icon.clone());
                    }
                    continue;
                }
                _ => {}
            }
        }
        let mut arg = String::new();
        let mut chars = word.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                arg.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => arg.push('%'),
                Some('c') => arg.push_str(&app.name),
                Some('k') => arg.push_str(&app.file.to_string_lossy()),
                Some(_) | None => {}
            }
        }
        args.push(arg);
    }
    if args.first().is_none_or(String::is_empty) {
        return Err(invalid());
    }
    args.extend(files.unwrap_or_default().iter().cloned());
    Ok(args)
}

/// The browsers of the Chromium family that load the extension of video redirection (F040).
const CHROMIUM_BROWSERS: [&str; 6] = [
    "chromium",
    "chromium-browser",
    "brave-browser",
    "brave-browser-stable",
    "brave",
    "google-chrome-stable",
];

/// Returns the directory of the extension of video redirection: `FARGLASS_REDIRECT_EXTENSION`, or the installed one.
fn redirect_extension() -> PathBuf {
    std::env::var_os("FARGLASS_REDIRECT_EXTENSION").map_or_else(
        || PathBuf::from("/usr/share/farglass/redirect-extension"),
        PathBuf::from,
    )
}

/// Adds `--load-extension` with the extension of video redirection after the program, if the program
/// is a Chromium browser and the extension is installed. Google Chrome ignores the option.
fn with_redirect_extension(mut args: Vec<String>, extension: &Path) -> Vec<String> {
    let at = args.iter().position(|arg| {
        Path::new(arg)
            .file_name()
            .is_some_and(|name| CHROMIUM_BROWSERS.iter().any(|b| name == *b))
    });
    if let Some(at) = at
        && extension.join("manifest.json").is_file()
    {
        args.insert(at + 1, format!("--load-extension={}", extension.display()));
    }
    args
}

/// The Chromium feature that reads cameras from PipeWire through the camera portal (ADR-056).
const CAMERA_FEATURE: &str = "WebRtcPipeWireCamera";

/// Turns on the PipeWire camera of a Chromium browser, so it finds the camera of the client (F116).
///
/// The feature goes into the `--enable-features` option of the command line, or into a new one after the program.
fn with_camera_feature(mut args: Vec<String>) -> Vec<String> {
    let Some(at) = args.iter().position(|arg| {
        Path::new(arg)
            .file_name()
            .is_some_and(|name| CHROMIUM_BROWSERS.iter().any(|b| name == *b))
    }) else {
        return args;
    };
    // Chromium reads the last --enable-features only: the feature joins that one.
    if let Some(features) = args[at + 1..]
        .iter_mut()
        .rev()
        .find(|arg| arg.starts_with("--enable-features="))
    {
        if !features["--enable-features=".len()..]
            .split(',')
            .any(|feature| feature == CAMERA_FEATURE)
        {
            features.push(',');
            features.push_str(CAMERA_FEATURE);
        }
    } else {
        args.insert(at + 1, format!("--enable-features={CAMERA_FEATURE}"));
    }
    args
}

/// Returns the name of a systemd unit with the text `text`: each byte that a unit name cannot have is `\xNN`, as `systemd-escape` does.
fn unit_text(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.') {
                char::from(byte).to_string()
            } else {
                format!("\\x{byte:02x}")
            }
        })
        .collect()
}

/// How the agent starts the apps.
#[derive(Clone, Debug)]
pub struct LaunchOptions {
    /// Start each app in its own systemd --user scope.
    pub scope: bool,
    /// The Wayland socket of the session. An app gets it as `WAYLAND_DISPLAY` if it exists.
    pub wayland: Option<PathBuf>,
    /// The XDG data directories.
    pub dirs: Vec<PathBuf>,
    /// The languages of the user, the best first.
    pub languages: Vec<String>,
}

impl LaunchOptions {
    /// Returns the options for this process: the scope as for the terminals, and the XDG directories.
    #[must_use]
    pub fn from_env() -> Self {
        let terminal = crate::terminal::TerminalOptions::from_env();
        Self {
            scope: terminal.scope,
            wayland: terminal.wayland,
            dirs: data_dirs(),
            languages: languages(),
        }
    }
}

/// The apps of the host and their start.
#[derive(Debug)]
pub struct Launcher {
    options: LaunchOptions,
    apps: Mutex<Vec<DesktopApp>>,
}

impl Launcher {
    /// Returns a launcher with `options`. It reads the desktop entries at the first call of [`Launcher::list`].
    #[must_use]
    pub fn new(options: LaunchOptions) -> Arc<Self> {
        Arc::new(Self {
            options,
            apps: Mutex::new(Vec::new()),
        })
    }

    /// Reads the desktop entries again and returns the apps for the shell.
    pub fn list(&self) -> Vec<HostApp> {
        let apps = scan(&self.options.dirs, &self.options.languages);
        let list = apps
            .iter()
            .map(|app| {
                let icon = app
                    .icon
                    .as_deref()
                    .is_some_and(|icon| find_icon(&self.options.dirs, icon).is_some());
                app.host_app(icon)
            })
            .collect();
        *self.apps.lock().unwrap_or_else(PoisonError::into_inner) = apps;
        list
    }

    /// Returns the desktop entry `id`. It reads the entries again if the last list does not have it.
    pub fn app(&self, id: &str) -> Option<DesktopApp> {
        let find = |apps: &[DesktopApp]| apps.iter().find(|app| app.id == id).cloned();
        if let Some(app) = find(&self.apps.lock().unwrap_or_else(PoisonError::into_inner)) {
            return Some(app);
        }
        self.list();
        find(&self.apps.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Returns the icon of the app `id`: its mime type and its bytes.
    pub fn icon(&self, id: &str) -> Option<(&'static str, Vec<u8>)> {
        let app = self.app(id)?;
        let (path, mime) = find_icon(&self.options.dirs, app.icon.as_deref()?)?;
        let bytes = std::fs::read(path).ok()?;
        (bytes.len() as u64 <= MAX_ICON).then_some((mime, bytes))
    }

    /// Starts the app `id` with the variables of the desktop, in its own scope if the options say so.
    /// Returns the process id. The app continues when the agent stops.
    ///
    /// # Errors
    ///
    /// [`LaunchError`] if the app is not known, its command line is not valid, or it does not start.
    pub fn launch(&self, id: &str) -> Result<u32, LaunchError> {
        self.launch_with(id, None)
    }

    /// As [`Launcher::launch`], with `files` at the place of the file field code (see [`exec_args_with`]).
    ///
    /// # Errors
    ///
    /// [`LaunchError`] if the app is not known, its command line is not valid, or it does not start.
    pub fn launch_with(&self, id: &str, files: Option<&[String]>) -> Result<u32, LaunchError> {
        let app = self
            .app(id)
            .ok_or_else(|| LaunchError::Unknown(id.to_owned()))?;
        let mut args = with_redirect_extension(exec_args_with(&app, files)?, &redirect_extension());
        if crate::desktop::camera_allowed() {
            args = with_camera_feature(args);
        }
        let mut command = if self.options.scope {
            let unit = format!(
                "app-farglass-{}-{}.scope",
                unit_text(&app.id),
                crate::terminal::random_hex()
            );
            let mut command = std::process::Command::new("systemd-run");
            command
                .args(["--user", "--scope", "--quiet", "--collect"])
                .arg(format!("--unit={unit}"))
                .arg(format!("--description={}", app.name))
                .arg("--")
                .args(&args);
            command
        } else {
            let mut command = std::process::Command::new(&args[0]);
            command.args(&args[1..]);
            command
        };
        if let Some(socket) = self.options.wayland.as_deref().filter(|path| path.exists()) {
            // The app shows on the compositor of the session, not on a local display of the same user.
            // With Xwayland, `variables` gives the X11 display of the session. A Qt app with
            // no Wayland plugin, for example one with its own Qt, then uses X11 (F082).
            let qt = if crate::desktop::has_x11() {
                "wayland;xcb"
            } else {
                "wayland"
            };
            command
                .env_remove("DISPLAY")
                .envs(crate::desktop::variables(socket))
                .env("MOZ_ENABLE_WAYLAND", "1")
                .env("QT_QPA_PLATFORM", qt)
                .env("ELECTRON_OZONE_PLATFORM_HINT", "wayland");
        }
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let dir = app
            .path
            .clone()
            .filter(|path| path.is_dir())
            .unwrap_or(home);
        command
            .env_remove("FARGLASS_AGENT_SOCKET")
            .env("DESKTOP_STARTUP_ID", "")
            .env("GIO_LAUNCHED_DESKTOP_FILE", &app.file)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // The app gets its own process group: a signal to the process group of the agent does not stop it.
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command.spawn().map_err(|source| LaunchError::Spawn {
            name: app.name.clone(),
            source,
        })?;
        let pid = child.id();
        tracing::info!(
            app = app.id,
            pid,
            scope = self.options.scope,
            "started an app"
        );
        // A thread waits for the end of the app, thus it does not stay as a zombie.
        let _ = std::thread::Builder::new()
            .name("farglass-app".to_owned())
            .spawn(move || {
                let _ = child.wait();
            });
        Ok(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(exec: &str) -> DesktopApp {
        DesktopApp {
            id: "test".into(),
            name: "Test App".into(),
            comment: None,
            keywords: Vec::new(),
            icon: Some("test-icon".into()),
            exec: unescape(exec),
            path: None,
            wm_class: None,
            file: PathBuf::from("/usr/share/applications/test.desktop"),
        }
    }

    #[test]
    fn exec_splits_quotes_and_expands_the_field_codes() {
        assert_eq!(
            exec_args(&app("/usr/bin/foo %U --name=%c %i %k")).unwrap(),
            [
                "/usr/bin/foo",
                "--name=Test App",
                "--icon",
                "test-icon",
                "/usr/share/applications/test.desktop"
            ]
        );
        assert_eq!(
            exec_args(&app(r#""/opt/My App/run" "a \\"b\\"" 100%% %F"#)).unwrap(),
            ["/opt/My App/run", "a \"b\"", "100%"]
        );
        assert_eq!(
            exec_args(&app(r#"sh -c "echo \\\\$HOME""#)).unwrap(),
            ["sh", "-c", "echo \\$HOME"],
            "a backslash in quotes is written four times in the file"
        );
        assert!(exec_args(&app("\"no end")).is_err());
        assert!(exec_args(&app("%U")).is_err());
    }

    #[test]
    fn the_files_of_a_bookmark_go_at_the_place_of_the_field_code() {
        let files = [
            "-P".to_owned(),
            "work".to_owned(),
            "https://example.com/".to_owned(),
        ];
        assert_eq!(
            exec_args_with(&app("firefox %u --flag"), Some(&files)).unwrap(),
            ["firefox", "-P", "work", "https://example.com/", "--flag"]
        );
        assert_eq!(
            exec_args_with(&app("firefox %U %F"), Some(&files)).unwrap(),
            ["firefox", "-P", "work", "https://example.com/"],
            "only the first field code gets the files"
        );
        assert_eq!(
            exec_args_with(&app("/opt/browser"), Some(&files)).unwrap(),
            ["/opt/browser", "-P", "work", "https://example.com/"],
            "a line with no field code gets the files at the end"
        );
        assert!(
            exec_args_with(&app("%U"), Some(&files)).is_err(),
            "a file is never the program"
        );
    }

    #[test]
    fn the_language_of_the_user_selects_the_localized_name() {
        assert_eq!(
            languages_of("de_AT.UTF-8@euro"),
            ["de_AT@euro", "de_AT", "de@euro", "de"]
        );
        assert_eq!(languages_of("C.UTF-8"), Vec::<String>::new());
        let text = "[Desktop Entry]\nType=Application\nName=Calculator\nName[de]=Rechner\nKeywords=calc;math\\;s;\nKeywords[de]=Rechnen;\nGenericName=Calculator App\nExec=calc\n[Desktop Action new]\nName=New\n";
        let Some(app) = parse("calc", text, Path::new("/x"), &languages_of("de_AT.UTF-8")) else {
            panic!("not shown")
        };
        assert_eq!(app.name, "Rechner");
        assert_eq!(app.keywords, ["Rechnen", "Calculator App"]);
        assert_eq!(app.comment.as_deref(), Some("Calculator App"));
        let Some(app) = parse("calc", text, Path::new("/x"), &[]) else {
            panic!("not shown")
        };
        assert_eq!(app.name, "Calculator");
        assert_eq!(app.keywords, ["calc", "math;s", "Calculator App"]);
    }

    #[test]
    fn hidden_entries_and_entries_of_other_desktops_are_not_shown() {
        let base = "[Desktop Entry]\nType=Application\nName=X\nExec=x\n";
        for extra in [
            "NoDisplay=true\n",
            "Hidden=true\n",
            "Terminal=true\n",
            "OnlyShowIn=GNOME;KDE;\n",
            "NotShowIn=farglass;\n",
            "TryExec=/nonexistent/program\n",
        ] {
            assert_eq!(
                parse("x", &format!("{base}{extra}"), Path::new("/x"), &[]),
                None,
                "{extra}"
            );
        }
        assert!(
            parse(
                "x",
                &format!("{base}OnlyShowIn=GNOME;farglass;\nTryExec=sh\n"),
                Path::new("/x"),
                &[]
            )
            .is_some()
        );
        assert_eq!(
            parse(
                "x",
                "[Desktop Entry]\nType=Link\nName=X\nURL=y\n",
                Path::new("/x"),
                &[]
            ),
            None
        );
    }

    #[test]
    fn the_first_directory_wins_and_a_hidden_entry_hides_the_id() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user");
        let system = dir.path().join("system");
        for (root, id, body) in [
            (&user, "a", "Name=User A\nExec=a\n"),
            (&user, "b", "Name=B\nExec=b\nHidden=true\n"),
            (&system, "a", "Name=System A\nExec=a\n"),
            (&system, "b", "Name=System B\nExec=b\n"),
            (&system, "kde/c", "Name=C\nExec=c\nIcon=c-icon\n"),
        ] {
            let path = root.join("applications").join(format!("{id}.desktop"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, format!("[Desktop Entry]\nType=Application\n{body}")).unwrap();
        }
        let icon = system.join("icons/hicolor/48x48/apps/c-icon.png");
        std::fs::create_dir_all(icon.parent().unwrap()).unwrap();
        std::fs::write(&icon, b"png").unwrap();
        let svg = system.join("icons/hicolor/scalable/apps/c-icon.svg");
        std::fs::create_dir_all(svg.parent().unwrap()).unwrap();
        std::fs::write(&svg, b"<svg/>").unwrap();

        let dirs = vec![user, system];
        let apps = scan(&dirs, &[]);
        assert_eq!(
            apps.iter()
                .map(|a| (a.id.as_str(), a.name.as_str()))
                .collect::<Vec<_>>(),
            [("kde-c", "C"), ("a", "User A")]
        );
        assert_eq!(
            find_icon(&dirs, "c-icon"),
            Some((svg, "image/svg+xml")),
            "a scalable icon goes first"
        );
        assert_eq!(find_icon(&dirs, "missing"), None);

        let launcher = Launcher::new(LaunchOptions {
            scope: false,
            wayland: None,
            dirs,
            languages: Vec::new(),
        });
        let list = launcher.list();
        assert_eq!(list[0].names, ["kde-c", "c"]);
        assert!(list[0].icon);
        assert!(!list[1].icon);
        assert_eq!(
            launcher.icon("kde-c"),
            Some(("image/svg+xml", b"<svg/>".to_vec()))
        );
        assert!(matches!(
            launcher.launch("nothing"),
            Err(LaunchError::Unknown(_))
        ));
    }

    #[test]
    fn the_program_name_skips_env_and_its_variables() {
        let args = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            program_name(&args("/usr/bin/Foo --x")).as_deref(),
            Some("foo")
        );
        assert_eq!(
            program_name(&args("env -u A B=1 /usr/bin/gnome-calculator")).as_deref(),
            Some("gnome-calculator")
        );
    }

    #[test]
    fn a_chromium_browser_gets_the_pipewire_camera() {
        let args = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            with_camera_feature(args("/usr/bin/chromium %U")),
            [
                "/usr/bin/chromium",
                "--enable-features=WebRtcPipeWireCamera",
                "%U"
            ]
        );
        assert_eq!(
            with_camera_feature(args("chromium --enable-features=A --enable-features=B,C")),
            [
                "chromium",
                "--enable-features=A",
                "--enable-features=B,C,WebRtcPipeWireCamera"
            ]
        );
        assert_eq!(
            with_camera_feature(args("chromium --enable-features=WebRtcPipeWireCamera")),
            ["chromium", "--enable-features=WebRtcPipeWireCamera"]
        );
        assert_eq!(with_camera_feature(args("firefox %u")), ["firefox", "%u"]);
    }

    #[test]
    fn a_chromium_browser_gets_the_extension_of_video_redirection() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("manifest.json"), "{}").unwrap();
        let args = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<_>>();
        let load = format!("--load-extension={}", dir.path().display());
        assert_eq!(
            with_redirect_extension(
                args("/usr/bin/brave-browser-stable --incognito"),
                dir.path()
            ),
            [
                "/usr/bin/brave-browser-stable",
                load.as_str(),
                "--incognito"
            ]
        );
        assert_eq!(
            with_redirect_extension(args("env A=1 chromium"), dir.path()),
            ["env", "A=1", "chromium", load.as_str()]
        );
        assert_eq!(
            with_redirect_extension(args("gnome-calculator"), dir.path()),
            ["gnome-calculator"]
        );
        assert_eq!(
            with_redirect_extension(args("chromium"), &dir.path().join("missing")),
            ["chromium"]
        );
    }

    #[test]
    fn a_unit_name_escapes_each_other_byte() {
        assert_eq!(unit_text("org.gnome.Calculator"), "org.gnome.Calculator");
        assert_eq!(unit_text("kde-my app"), "kde\\x2dmy\\x20app");
    }
}
