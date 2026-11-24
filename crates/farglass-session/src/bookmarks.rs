//! The bookmarks of the user (F100): the file `$XDG_CONFIG_HOME/farglass/bookmarks.toml`.
//!
//! The file is the only store: the user can edit it by hand and copy it to a
//! different host. A save from the shell keeps the comments of the file and
//! the comments of each bookmark that stays.
//!
//! ```toml
//! [options]
//! launcher = false
//!
//! [[bookmark]]
//! name = "gh"
//! url = "https://github.com/"
//! browser = "firefox"
//! profile = "work"
//! private = false
//! ```

use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use farglass_proto::message::{Bookmark, BookmarkFile, BookmarkProblem, BookmarksEvent};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, TableLike, Value};

use crate::browsers::{self, KEYS};
use crate::launch::Launcher;

/// The largest bookmarks file in bytes.
const MAX_FILE: u64 = 1 << 20;

/// Returns the path of the bookmarks file: `$XDG_CONFIG_HOME/farglass/bookmarks.toml`, or `~/.config/farglass/bookmarks.toml`.
#[must_use]
pub fn default_path() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map_or_else(|| PathBuf::from("/"), PathBuf::from)
                .join(".config")
        });
    config.join("farglass").join("bookmarks.toml")
}

/// Checks a URL of a bookmark: `http` or `https` with a host.
///
/// # Errors
///
/// A message for the user.
pub fn check_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| format!("{url} is not a URL."))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(format!("{url} is not an http or https URL."));
    }
    Ok(())
}

/// Parses the text of a bookmarks file. A bookmark with a field of the wrong type gets an empty value.
///
/// # Errors
///
/// A message for the user if the text is not valid TOML.
pub fn parse(text: &str) -> Result<BookmarkFile, String> {
    let doc: DocumentMut = text.parse().map_err(|error| format!("{error}"))?;
    let launcher = doc
        .get("options")
        .and_then(|options| options.get("launcher"))
        .and_then(Item::as_bool)
        .unwrap_or(false);
    let text_of =
        |table: &Table, key: &str| table.get(key).and_then(Item::as_str).map(str::to_owned);
    let bookmarks = doc
        .get("bookmark")
        .and_then(Item::as_array_of_tables)
        .map(|tables| {
            tables
                .iter()
                .map(|table| Bookmark {
                    name: text_of(table, "name").unwrap_or_default(),
                    url: text_of(table, "url").unwrap_or_default(),
                    browser: text_of(table, "browser").unwrap_or_default(),
                    profile: text_of(table, "profile").filter(|p| !p.is_empty()),
                    private: table
                        .get("private")
                        .and_then(Item::as_bool)
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(BookmarkFile {
        launcher,
        bookmarks,
    })
}

/// Checks the contents before a save: each name is set and unique, each URL is valid, and each browser is set.
/// A browser that is not installed is permitted: the file can come from a different host.
///
/// # Errors
///
/// A message for the user.
pub fn check(file: &BookmarkFile) -> Result<(), String> {
    let mut names = HashSet::new();
    for bookmark in &file.bookmarks {
        let name = bookmark.name.trim();
        if name.is_empty() {
            return Err("Each bookmark must have a name.".to_owned());
        }
        if name != bookmark.name {
            return Err(format!(
                "The name \"{}\" must not start or end with a space.",
                bookmark.name
            ));
        }
        if !names.insert(name) {
            return Err(format!("Two bookmarks have the name {name}."));
        }
        check_url(&bookmark.url).map_err(|error| format!("{name}: {error}"))?;
        if bookmark.browser.trim().is_empty() {
            return Err(format!("{name}: select a browser."));
        }
    }
    Ok(())
}

/// Returns the text of the file with the contents `file`. The comments and the
/// layout of `old` stay: a bookmark keeps its table if its name stays.
#[must_use]
pub fn render(old: &DocumentMut, file: &BookmarkFile) -> String {
    let mut doc = old.clone();
    let options = doc
        .entry("options")
        .or_insert_with(|| Item::Table(Table::new()));
    if let Some(options) = options.as_table_like_mut() {
        set(options, "launcher", file.launcher.into());
    }
    let old_tables: Vec<Table> = old
        .get("bookmark")
        .and_then(Item::as_array_of_tables)
        .map(|tables| tables.iter().cloned().collect())
        .unwrap_or_default();
    let mut tables = ArrayOfTables::new();
    for bookmark in &file.bookmarks {
        let mut table = old_tables
            .iter()
            .find(|table| table.get("name").and_then(Item::as_str) == Some(&bookmark.name))
            .cloned()
            .unwrap_or_default();
        set(&mut table, "name", bookmark.name.as_str().into());
        set(&mut table, "url", bookmark.url.as_str().into());
        set(&mut table, "browser", bookmark.browser.as_str().into());
        match &bookmark.profile {
            Some(profile) if !profile.is_empty() => {
                set(&mut table, "profile", profile.as_str().into());
            }
            _ => {
                table.remove("profile");
            }
        }
        if bookmark.private {
            set(&mut table, "private", true.into());
        } else {
            table.remove("private");
        }
        tables.push(table);
    }
    doc.remove("bookmark");
    if !tables.is_empty() {
        doc.insert("bookmark", Item::ArrayOfTables(tables));
    }
    doc.to_string()
}

/// Sets `key` of `table` to `new`. A value that was there keeps its comments.
fn set(table: &mut dyn TableLike, key: &str, mut new: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        None => {
            table.insert(key, Item::Value(new));
        }
    }
}

/// The text of a new file: a comment that tells the format.
const NEW_FILE: &str = "\
# The bookmarks of farglass. Copy this file to use the bookmarks on a different host.
#
# browser: firefox, chromium, chrome, brave, client (a tab of the browser that shows
# farglass), or the desktop file id of a different browser.
# profile: the name of the profile that the browser shows. private: true for a private window.
";

/// The bookmarks file of the session and the browsers that open its bookmarks.
#[derive(Debug)]
pub struct Bookmarks {
    path: PathBuf,
    launcher: Arc<Launcher>,
    /// Only one save at a time.
    lock: Mutex<()>,
}

impl Bookmarks {
    /// Returns the bookmarks of the file `path`. The browsers come from the desktop entries of `launcher`.
    #[must_use]
    pub fn new(path: PathBuf, launcher: Arc<Launcher>) -> Arc<Self> {
        Arc::new(Self {
            path,
            launcher,
            lock: Mutex::new(()),
        })
    }

    /// Reads the file. `Ok(None)` if it does not exist.
    fn read(&self) -> Result<Option<String>, String> {
        match std::fs::metadata(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("farglass cannot read the file: {error}.")),
            Ok(meta) if meta.len() > MAX_FILE => {
                return Err("The file is larger than 1 MiB.".to_owned());
            }
            Ok(_) => {}
        }
        std::fs::read_to_string(&self.path)
            .map(Some)
            .map_err(|error| format!("farglass cannot read the file: {error}."))
    }

    /// Reads the file and the browsers, and checks each bookmark.
    #[must_use]
    pub fn load(&self) -> BookmarksEvent {
        let (file, error) = match self
            .read()
            .and_then(|text| text.map_or(Ok(BookmarkFile::default()), |t| parse(&t)))
        {
            Ok(file) => (file, None),
            Err(error) => (BookmarkFile::default(), Some(error)),
        };
        let installed = browsers::installed(&self.launcher);
        let mut names = HashSet::new();
        let problems = file
            .bookmarks
            .iter()
            .enumerate()
            .filter_map(|(index, bookmark)| {
                let message = if !names.insert(bookmark.name.as_str()) {
                    Some(format!("Two bookmarks have the name {}.", bookmark.name))
                } else {
                    self.target(bookmark).err()
                };
                Some(BookmarkProblem {
                    index: u32::try_from(index).ok()?,
                    message: message?,
                })
            })
            .collect();
        BookmarksEvent::Loaded {
            path: self.path.display().to_string(),
            file,
            browsers: installed.iter().map(browsers::Browser::info).collect(),
            problems,
            error,
        }
    }

    /// Returns the arguments that open `bookmark`, or `Ok(None)` for the client browser.
    fn target(&self, bookmark: &Bookmark) -> Result<Option<(String, Vec<String>)>, String> {
        if bookmark.name.trim().is_empty() {
            return Err("The bookmark has no name.".to_owned());
        }
        check_url(&bookmark.url)?;
        if bookmark.browser == "client" {
            return Ok(None);
        }
        if bookmark.browser.is_empty() {
            return Err("The bookmark has no browser.".to_owned());
        }
        let browser = browsers::find(&self.launcher, &bookmark.browser).ok_or_else(|| {
            if KEYS.contains(&bookmark.browser.as_str()) {
                format!("{} is not installed on this host.", bookmark.browser)
            } else {
                format!("No app {} is installed on this host.", bookmark.browser)
            }
        })?;
        let args = browsers::url_args(
            &browser,
            bookmark.profile.as_deref(),
            bookmark.private,
            &bookmark.url,
        )?;
        Ok(Some((browser.app.id, args)))
    }

    /// Writes `file` and returns the new contents. The file does not change if `file` is not valid,
    /// or if the file on the disk is not valid TOML: a save does not overwrite a change by hand.
    ///
    /// # Errors
    ///
    /// A message for the user.
    pub fn save(&self, file: &BookmarkFile) -> Result<BookmarksEvent, String> {
        check(file)?;
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let old = self.read()?;
        let doc: DocumentMut = old.as_deref().unwrap_or_default().parse().map_err(|error| {
            format!("The file is not valid TOML, thus farglass does not save over it. Correct the file first: {error}")
        })?;
        let mut text = render(&doc, file);
        if old.is_none() {
            text = format!("{NEW_FILE}\n{text}");
        }
        write_atomic(&self.path, text.as_bytes())
            .map_err(|error| format!("farglass cannot write the file: {error}."))?;
        Ok(self.load())
    }

    /// Opens the bookmark `name` of the file in its browser on the host.
    ///
    /// # Errors
    ///
    /// A message for the user.
    pub fn open(&self, name: &str) -> Result<(), String> {
        let file = parse(&self.read()?.unwrap_or_default())?;
        let bookmark = file
            .bookmarks
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| format!("No bookmark has the name {name}."))?;
        let (id, args) = self.target(bookmark)?.ok_or_else(|| {
            format!("{name} opens in the browser of this computer, not on the host.")
        })?;
        self.launcher
            .launch_with(&id, Some(&args))
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

/// Writes `bytes` to a new file next to `path` and renames it to `path`.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = dir.join(format!(".{name}.{}", crate::terminal::random_hex()));
    let result = (|| {
        let mut out = std::fs::File::create_new(&temp)?;
        out.write_all(bytes)?;
        out.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bookmark(name: &str, url: &str) -> Bookmark {
        Bookmark {
            name: name.into(),
            url: url.into(),
            browser: "firefox".into(),
            profile: None,
            private: false,
        }
    }

    #[test]
    fn only_http_and_https_urls_with_a_host_are_valid() {
        assert!(check_url("https://example.com/a?b").is_ok());
        assert!(check_url("http://10.0.0.1:8080/").is_ok());
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "-P evil",
            "--new-instance",
            "example.com",
            "https://",
            "",
        ] {
            assert!(check_url(bad).is_err(), "{bad} is not valid");
        }
    }

    #[test]
    fn parse_reads_the_options_and_each_bookmark() {
        let file = parse(
            "[options]\nlauncher = true\n\n[[bookmark]]\nname = \"gh\"\nurl = \"https://github.com/\"\n\
             browser = \"chromium\"\nprofile = \"Work\"\nprivate = true\n\n\
             [[bookmark]]\nname = \"bad\"\nurl = 3\n",
        )
        .unwrap();
        assert!(file.launcher);
        assert_eq!(
            file.bookmarks[0],
            Bookmark {
                name: "gh".into(),
                url: "https://github.com/".into(),
                browser: "chromium".into(),
                profile: Some("Work".into()),
                private: true,
            }
        );
        assert_eq!(
            file.bookmarks[1].url, "",
            "a field of the wrong type is empty"
        );
        assert_eq!(parse("").unwrap(), BookmarkFile::default());
        assert!(parse("[[bookmark]\n").is_err());
    }

    #[test]
    fn check_refuses_an_empty_or_a_double_name_and_a_bad_url() {
        let ok = BookmarkFile {
            launcher: false,
            bookmarks: vec![
                bookmark("a", "https://a.example/"),
                bookmark("b", "https://b.example/"),
            ],
        };
        assert!(check(&ok).is_ok());
        let mut double = ok.clone();
        double.bookmarks[1].name = "a".into();
        assert!(check(&double).is_err());
        let mut empty = ok.clone();
        empty.bookmarks[0].name = " ".into();
        assert!(check(&empty).is_err());
        let mut url = ok.clone();
        url.bookmarks[0].url = "file:///etc/passwd".into();
        assert!(check(&url).is_err());
        let mut browser = ok;
        browser.bookmarks[0].browser = String::new();
        assert!(check(&browser).is_err());
    }

    #[test]
    fn a_save_keeps_the_comments_and_the_order_of_the_file() {
        let old: DocumentMut = "# my bookmarks\n\n[options]\nlauncher = false # off\n\n\
                                # the code\n[[bookmark]]\nname = \"gh\" # short\nurl = \"https://github.com/\"\nbrowser = \"firefox\"\n\n\
                                [[bookmark]]\nname = \"old\"\nurl = \"https://old.example/\"\nbrowser = \"client\"\n"
            .parse()
            .unwrap();
        let mut gh = bookmark("gh", "https://github.com/scuq");
        gh.private = true;
        let file = BookmarkFile {
            launcher: true,
            bookmarks: vec![bookmark("new", "https://new.example/"), gh],
        };
        let text = render(&old, &file);
        assert!(text.starts_with("# my bookmarks\n"), "{text}");
        assert!(text.contains("# the code\n"), "{text}");
        assert!(text.contains("name = \"gh\" # short"), "{text}");
        assert!(!text.contains("old.example"), "{text}");
        assert!(
            text.find("new.example") < text.find("github.com/scuq"),
            "{text}"
        );
        assert_eq!(parse(&text).unwrap(), file);
    }

    #[test]
    fn a_save_writes_a_new_file_and_refuses_to_save_over_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("farglass/bookmarks.toml");
        let bookmarks = Bookmarks::new(
            path.clone(),
            Launcher::new(crate::launch::LaunchOptions {
                scope: false,
                wayland: None,
                dirs: Vec::new(),
                languages: Vec::new(),
            }),
        );
        let mut client = bookmark("docs", "https://docs.example/");
        client.browser = "client".into();
        let mut missing = bookmark("ff", "https://ff.example/");
        missing.profile = Some("work".into());
        let file = BookmarkFile {
            launcher: false,
            bookmarks: vec![client, missing],
        };
        let BookmarksEvent::Loaded {
            file: saved,
            problems,
            error,
            ..
        } = bookmarks.save(&file).unwrap()
        else {
            panic!("a save answers with the file");
        };
        assert_eq!(saved, file);
        assert_eq!(error, None);
        assert_eq!(problems.len(), 1, "firefox is not installed: {problems:?}");
        assert_eq!(problems[0].index, 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# The bookmarks of farglass."), "{text}");
        assert!(
            bookmarks.open("docs").is_err(),
            "a client bookmark does not open on the host"
        );
        assert!(bookmarks.open("nothing").is_err());

        std::fs::write(&path, "[[bookmark]\n").unwrap();
        assert!(bookmarks.save(&file).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[[bookmark]\n");
        let BookmarksEvent::Loaded { error, .. } = bookmarks.load() else {
            panic!("a load answers with the file");
        };
        assert!(error.is_some());
    }
}
