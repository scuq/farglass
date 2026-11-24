//! The web browsers of the host and their profiles, for the bookmarks (F100).
//!
//! A bookmark names a browser by a short key (`firefox`, `chromium`, `chrome`,
//! `brave`) or by a desktop file id. The agent finds the desktop entry of the
//! browser, reads the names of its profiles, and gives the browser the options
//! of the profile and of a private window before the URL.

use std::path::{Path, PathBuf};

use farglass_proto::message::BookmarkBrowser;

use crate::launch::{DesktopApp, Launcher, exec_args};

/// How a browser takes a profile and a private window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// `-P NAME`, `--private-window`, and `--new-tab`.
    Firefox,
    /// `--profile-directory=DIR` and `--incognito`.
    Chromium,
    /// A different browser: the URL only.
    Other,
}

/// A browser that a bookmark can name by its key.
struct Known {
    key: &'static str,
    family: Family,
    /// The desktop file ids, the first installed one wins. An id with a dot is a Flatpak app.
    ids: &'static [&'static str],
    /// The directories of the profiles, relative to the home directory, or to the
    /// directory of the Flatpak app for an id with a dot.
    native: &'static [&'static str],
    flatpak: &'static str,
}

const KNOWN: [Known; 4] = [
    Known {
        key: "firefox",
        family: Family::Firefox,
        ids: &["firefox", "firefox-esr", "org.mozilla.firefox"],
        native: &[".mozilla/firefox", ".config/mozilla/firefox"],
        flatpak: ".mozilla/firefox",
    },
    Known {
        key: "chromium",
        family: Family::Chromium,
        ids: &["chromium", "chromium-browser", "org.chromium.Chromium"],
        native: &[".config/chromium"],
        flatpak: "config/chromium",
    },
    Known {
        key: "chrome",
        family: Family::Chromium,
        ids: &["google-chrome", "google-chrome-stable", "com.google.Chrome"],
        native: &[".config/google-chrome"],
        flatpak: "config/google-chrome",
    },
    Known {
        key: "brave",
        family: Family::Chromium,
        ids: &["brave-browser", "brave-browser-stable", "com.brave.Browser"],
        native: &[".config/BraveSoftware/Brave-Browser"],
        flatpak: "config/BraveSoftware/Brave-Browser",
    },
];

/// The keys of the browsers that a bookmark can name, with `client` for a tab of the client browser.
pub const KEYS: [&str; 5] = ["firefox", "chromium", "chrome", "brave", "client"];

/// A profile of a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The name that the browser shows.
    pub name: String,
    /// The directory of a Chromium profile, for example `Profile 1`. For Firefox, the name.
    pub dir: String,
}

/// An installed browser.
#[derive(Clone, Debug)]
pub struct Browser {
    /// The key or the desktop file id that the bookmark gives.
    pub key: String,
    /// The desktop entry.
    pub app: DesktopApp,
    /// How it takes the options.
    pub family: Family,
    /// Its profiles.
    pub profiles: Vec<Profile>,
}

impl Browser {
    /// Returns the facts for the shell.
    #[must_use]
    pub fn info(&self) -> BookmarkBrowser {
        BookmarkBrowser {
            id: self.key.clone(),
            name: self.app.name.clone(),
            profiles: self.profiles.iter().map(|p| p.name.clone()).collect(),
        }
    }

    /// Returns the profile with the name or the directory `name`.
    #[must_use]
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|p| p.name == name)
            .or_else(|| self.profiles.iter().find(|p| p.dir == name))
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// Returns the browser of the key or desktop file id `key`, if it is installed.
#[must_use]
pub fn find(launcher: &Launcher, key: &str) -> Option<Browser> {
    find_in(launcher, key, &home())
}

fn find_in(launcher: &Launcher, key: &str, home: &Path) -> Option<Browser> {
    if let Some(known) = KNOWN.iter().find(|k| k.key == key) {
        let (id, app) = known
            .ids
            .iter()
            .find_map(|id| launcher.app(id).map(|app| (*id, app)))?;
        let dirs: Vec<PathBuf> = if id.contains('.') {
            vec![home.join(".var/app").join(id).join(known.flatpak)]
        } else {
            known.native.iter().map(|dir| home.join(dir)).collect()
        };
        return Some(Browser {
            key: key.to_owned(),
            app,
            family: known.family,
            profiles: profiles(known.family, &dirs),
        });
    }
    // A desktop file id: the family comes from the program, and it has no profiles.
    let app = launcher.app(key)?;
    let program = exec_args(&app)
        .ok()
        .and_then(|args| {
            Path::new(args.first()?)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
        })
        .unwrap_or_default();
    let family = if program.contains("firefox") || program.contains("librewolf") {
        Family::Firefox
    } else if ["chromium", "chrome", "brave", "vivaldi", "edge"]
        .iter()
        .any(|name| program.contains(name))
    {
        Family::Chromium
    } else {
        Family::Other
    };
    Some(Browser {
        key: key.to_owned(),
        app,
        family,
        profiles: Vec::new(),
    })
}

/// Returns the installed browsers of [`KNOWN`].
#[must_use]
pub fn installed(launcher: &Launcher) -> Vec<Browser> {
    let home = home();
    KNOWN
        .iter()
        .filter_map(|known| find_in(launcher, known.key, &home))
        .collect()
}

/// Reads the profiles of a browser of `family` from the first directory in `dirs` that has them.
fn profiles(family: Family, dirs: &[PathBuf]) -> Vec<Profile> {
    for dir in dirs {
        let found = match family {
            Family::Firefox => std::fs::read_to_string(dir.join("profiles.ini"))
                .map(|text| firefox_profiles(&text))
                .unwrap_or_default(),
            Family::Chromium => std::fs::read_to_string(dir.join("Local State"))
                .map(|text| chromium_profiles(&text))
                .unwrap_or_default(),
            Family::Other => Vec::new(),
        };
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

/// Returns the profiles of a Firefox `profiles.ini`: the `Name` of each `[Profile*]` section.
#[must_use]
pub fn firefox_profiles(text: &str) -> Vec<Profile> {
    let mut found = Vec::new();
    let mut in_profile = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_profile = line.starts_with("[Profile");
        } else if in_profile && let Some(name) = line.strip_prefix("Name=") {
            found.push(Profile {
                name: name.to_owned(),
                dir: name.to_owned(),
            });
        }
    }
    found
}

/// Returns the profiles of a Chromium `Local State` file: `profile.info_cache`, sorted by directory.
#[must_use]
pub fn chromium_profiles(text: &str) -> Vec<Profile> {
    let Ok(state) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(cache) = state
        .pointer("/profile/info_cache")
        .and_then(|v| v.as_object())
    else {
        return Vec::new();
    };
    let mut found: Vec<Profile> = cache
        .iter()
        .map(|(dir, info)| Profile {
            name: info
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or(dir)
                .to_owned(),
            dir: dir.clone(),
        })
        .collect();
    // "Default" first, then "Profile 1", "Profile 2", and so on.
    found.sort_by_key(|p| (p.dir != "Default", p.dir.len(), p.dir.clone()));
    found
}

/// Returns the arguments for the file field code of the browser: the profile, the private window, and the URL.
///
/// The caller checked that `url` is an `http` or `https` URL, thus it does not start with `-`.
///
/// # Errors
///
/// A message for the user if the browser has no such profile, or cannot open a private window.
pub fn url_args(
    browser: &Browser,
    profile: Option<&str>,
    private: bool,
    url: &str,
) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let name = &browser.app.name;
    if let Some(wanted) = profile {
        if browser.family == Family::Other {
            return Err(format!("{name} has no profiles for farglass."));
        }
        let found = browser
            .profile(wanted)
            .ok_or_else(|| format!("{name} has no profile {wanted}."))?;
        match browser.family {
            Family::Firefox => args.extend(["-P".to_owned(), found.dir.clone()]),
            Family::Chromium => args.push(format!("--profile-directory={}", found.dir)),
            Family::Other => {}
        }
    }
    match (browser.family, private) {
        (Family::Firefox, true) => args.push("--private-window".to_owned()),
        (Family::Firefox, false) => args.push("--new-tab".to_owned()),
        (Family::Chromium, true) => args.push("--incognito".to_owned()),
        (Family::Other, true) => {
            return Err(format!("farglass cannot open a private window of {name}."));
        }
        (Family::Chromium | Family::Other, false) => {}
    }
    args.push(url.to_owned());
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser(family: Family, profiles: Vec<Profile>) -> Browser {
        Browser {
            key: "test".into(),
            app: DesktopApp {
                id: "test".into(),
                name: "Test Browser".into(),
                comment: None,
                keywords: Vec::new(),
                icon: None,
                exec: "test %u".into(),
                path: None,
                wm_class: None,
                file: PathBuf::from("/usr/share/applications/test.desktop"),
            },
            family,
            profiles,
        }
    }

    fn profile(name: &str, dir: &str) -> Profile {
        Profile {
            name: name.into(),
            dir: dir.into(),
        }
    }

    #[test]
    fn firefox_profiles_are_the_names_of_the_profile_sections() {
        let ini = "[Install4F96D1932A9F858E]\nDefault=abc.default-release\n\n\
                   [Profile1]\nName=work\nIsRelative=1\nPath=x1.work\n\n\
                   [Profile0]\nName=default-release\nPath=abc.default-release\n\n\
                   [General]\nStartWithLastProfile=1\nName=not a profile\n";
        let names: Vec<String> = firefox_profiles(ini).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["work", "default-release"]);
    }

    #[test]
    fn chromium_profiles_map_the_shown_name_to_the_directory() {
        let state = r#"{"profile":{"info_cache":{
            "Profile 2":{"name":"Private stuff"},
            "Default":{"name":"Person 1"},
            "Profile 1":{"name":"Work"}}}}"#;
        assert_eq!(
            chromium_profiles(state),
            [
                profile("Person 1", "Default"),
                profile("Work", "Profile 1"),
                profile("Private stuff", "Profile 2"),
            ]
        );
        assert!(chromium_profiles("not json").is_empty());
        assert!(chromium_profiles("{}").is_empty());
    }

    #[test]
    fn firefox_gets_the_profile_and_a_new_tab_or_a_private_window() {
        let firefox = browser(Family::Firefox, vec![profile("work", "work")]);
        let url = "https://example.com/";
        assert_eq!(
            url_args(&firefox, Some("work"), false, url).unwrap(),
            ["-P", "work", "--new-tab", url]
        );
        assert_eq!(
            url_args(&firefox, None, true, url).unwrap(),
            ["--private-window", url]
        );
        assert!(url_args(&firefox, Some("home"), false, url).is_err());
    }

    #[test]
    fn chromium_gets_the_profile_directory_by_its_shown_name() {
        let chromium = browser(Family::Chromium, vec![profile("Work", "Profile 1")]);
        let url = "https://example.com/";
        assert_eq!(
            url_args(&chromium, Some("Work"), true, url).unwrap(),
            ["--profile-directory=Profile 1", "--incognito", url]
        );
        assert_eq!(
            url_args(&chromium, Some("Profile 1"), false, url).unwrap(),
            ["--profile-directory=Profile 1", url],
            "the directory name also selects the profile"
        );
        assert_eq!(url_args(&chromium, None, false, url).unwrap(), [url]);
    }

    #[test]
    fn a_different_browser_gets_the_url_only() {
        let other = browser(Family::Other, Vec::new());
        let url = "https://example.com/";
        assert_eq!(url_args(&other, None, false, url).unwrap(), [url]);
        assert!(url_args(&other, None, true, url).is_err());
        assert!(url_args(&other, Some("work"), false, url).is_err());
    }
}
