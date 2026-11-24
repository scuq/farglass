//! The system tray of the session (F102) and the menus of its items (F103).
//!
//! A program that stays in a tray looks for `org.kde.StatusNotifierWatcher` on
//! the session bus and registers a status notifier item there. The agent owns
//! that name. It reads each item and sends it to the tray channels of the
//! shell, and the panel shows one button for each item. A click in the shell
//! goes back to the program as a method call on its item.
//!
//! The menu of an item is a `com.canonical.dbusmenu` object of the program.
//! The agent reads it each time the shell asks, and sends a click on an entry
//! back as the event `clicked`.
//!
//! Each program on the bus can register an item, thus each value has a limit.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use farglass_proto::message::{
    TrayEvent, TrayItem, TrayMenuItem, TrayRequest, TrayStatus, TrayToggle,
};
use futures_util::{FutureExt as _, StreamExt as _};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::object_server::SignalEmitter;
use zbus::{Connection, MatchRule, MessageStream};
use zbus::{OwnedVariant, Variant};

use crate::launch::{MAX_ICON, data_dirs, find_tray_icon};

/// The bus name of the watcher of the status notifier items.
pub const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";

/// The object path of the watcher.
pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";

/// The interface of an item.
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";

/// The object path of an item that registers with a bus name only.
const ITEM_PATH: &str = "/StatusNotifierItem";

/// The largest number of items. The agent ignores an item above it.
pub const MAX_ITEMS: usize = 64;

/// The largest width and height of an icon in pixels that the agent accepts.
const MAX_SIDE: i32 = 512;

/// The width and height in pixels that the agent prefers for an icon. The panel shows it smaller.
const ICON_SIDE: i32 = 48;

/// The largest number of icon sizes that the agent reads from one item.
const MAX_PIXMAPS: usize = 16;

/// The largest number of characters of a name or a title.
const MAX_TEXT: usize = 256;

/// The interface of the menu of an item.
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";

/// The value of the property `Menu` of an item that has no menu.
const NO_MENU: &str = "/NO_DBUSMENU";

/// The largest depth of the submenus that the agent reads.
const MAX_MENU_DEPTH: usize = 8;

/// The largest number of entries of one menu, with its submenus.
pub const MAX_MENU_ITEMS: usize = 500;

/// The largest size in bytes of the icon of a menu entry.
const MAX_MENU_ICON: usize = 64 * 1024;

/// The time that a program has to answer a call.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

/// The wait after a signal of an item. A program sends some signals in a row.
const SETTLE: Duration = Duration::from_millis(50);

/// An icon of an item: the width, the height, and the pixels.
/// Each pixel has the bytes alpha, red, green, blue, with straight alpha.
pub type Pixmap = (i32, i32, Vec<u8>);

/// The properties of an item that the agent reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Properties {
    /// `Id`: the name of the item.
    pub id: String,
    /// `Title`.
    pub title: String,
    /// `Status`: `Passive`, `Active`, or `NeedsAttention`.
    pub status: String,
    /// `IconName`: the name of an icon, or the path of a file.
    pub icon_name: String,
    /// `IconThemePath`: a directory with the icons of the program.
    pub icon_theme_path: String,
    /// `IconPixmap`.
    pub icon_pixmap: Vec<Pixmap>,
    /// `AttentionIconName`.
    pub attention_icon_name: String,
    /// `AttentionIconPixmap`.
    pub attention_icon_pixmap: Vec<Pixmap>,
    /// The title of `ToolTip`.
    pub tooltip: String,
    /// `Menu`: the object path of the menu, or empty.
    pub menu: String,
    /// `ItemIsMenu`.
    pub item_is_menu: bool,
}

/// Returns the bus name and the object path of the item that registers with `service`.
///
/// A program gives its bus name, or the object path of the item on its own
/// connection. `sender` is the unique name of the program.
#[must_use]
pub fn split_service(service: &str, sender: &str) -> Option<(String, String)> {
    let (name, path) = if service.starts_with('/') {
        (sender, service)
    } else {
        (service, ITEM_PATH)
    };
    let valid = name.len() <= 255
        && path.len() <= 255
        && zbus::names::BusName::try_from(name).is_ok()
        && zbus::ObjectPath::try_from(path).is_ok();
    valid.then(|| (name.to_owned(), path.to_owned()))
}

/// Returns the status for the value of the property `Status`. An unknown value is active.
#[must_use]
pub fn status_of(status: &str) -> TrayStatus {
    match status {
        "Passive" => TrayStatus::Passive,
        "NeedsAttention" => TrayStatus::NeedsAttention,
        _ => TrayStatus::Active,
    }
}

/// Reports whether the pixels of `pixmap` match its size, and the size is in the limit.
fn pixmap_valid((width, height, pixels): &Pixmap) -> bool {
    let side = |value: &i32| {
        (1..=MAX_SIDE)
            .contains(value)
            .then(|| usize::try_from(*value).ok())
            .flatten()
    };
    match (side(width), side(height)) {
        (Some(width), Some(height)) => pixels.len() == width * height * 4,
        _ => false,
    }
}

/// Returns the icon to show: the smallest one of at least 48 pixels, or the largest one.
#[must_use]
pub fn best_pixmap(pixmaps: &[Pixmap]) -> Option<&Pixmap> {
    let side = |pixmap: &&Pixmap| pixmap.0.max(pixmap.1);
    let valid = || pixmaps.iter().filter(|pixmap| pixmap_valid(pixmap));
    valid()
        .filter(|pixmap| side(pixmap) >= ICON_SIDE)
        .min_by_key(side)
        .or_else(|| valid().max_by_key(side))
}

/// Returns the pixels of an item icon in the byte order red, green, blue, alpha.
#[must_use]
pub fn argb_to_rgba(argb: &[u8]) -> Vec<u8> {
    argb.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[a, r, g, b]| [r, g, b, a])
        .collect()
}

/// Returns `text` in one line and with no more than [`MAX_TEXT`] characters.
fn clip(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TEXT)
        .collect()
}

/// Returns the icon of the item in base64 and its mime type.
///
/// With the status `NeedsAttention`, the attention icon comes first. A named
/// icon comes before the pixels, because an SVG file stays sharp.
fn icon_of(properties: &Properties, dirs: &[PathBuf]) -> Option<(String, &'static str)> {
    let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    let attention = status_of(&properties.status) == TrayStatus::NeedsAttention;
    let sources = [
        (
            &properties.attention_icon_name,
            &properties.attention_icon_pixmap,
        ),
        (&properties.icon_name, &properties.icon_pixmap),
    ];
    for (name, pixmaps) in sources.into_iter().skip(usize::from(!attention)) {
        if !name.is_empty()
            && let Some((path, mime)) = find_tray_icon(dirs, &properties.icon_theme_path, name)
            && let Ok(bytes) = std::fs::read(&path)
            && u64::try_from(bytes.len()).is_ok_and(|len| len <= MAX_ICON)
        {
            return Some((encode(&bytes), mime));
        }
        if let Some((width, height, pixels)) = best_pixmap(pixmaps)
            && let (Ok(width), Ok(height)) = (u32::try_from(*width), u32::try_from(*height))
            && let Some(png) =
                farglass_surface::cursor::png_rgba(width, height, &argb_to_rgba(pixels))
        {
            return Some((encode(&png), "image/png"));
        }
    }
    None
}

/// Returns the item `id` for the shell from the properties of the program.
/// `dirs` are the data directories with the icon themes.
#[must_use]
pub fn item_from(id: u32, properties: &Properties, dirs: &[PathBuf]) -> TrayItem {
    let title = [&properties.title, &properties.tooltip, &properties.id]
        .into_iter()
        .find(|text| !text.is_empty())
        .cloned()
        .unwrap_or_else(|| "Tray item".to_owned());
    let tooltip = (!properties.tooltip.is_empty() && properties.tooltip != title)
        .then(|| properties.tooltip.clone());
    let (icon, icon_mime) = icon_of(properties, dirs).unzip();
    TrayItem {
        id,
        name: properties.id.clone(),
        title,
        tooltip,
        status: status_of(&properties.status),
        icon,
        icon_mime: icon_mime.map(str::to_owned),
        menu: menu_path_of(properties).is_some(),
        menu_only: properties.item_is_menu,
    }
}

/// Returns the object path of the menu of the item, or `None` if it has no menu.
fn menu_path_of(properties: &Properties) -> Option<&str> {
    (!properties.menu.is_empty() && properties.menu != NO_MENU).then_some(properties.menu.as_str())
}

/// Returns `label` with no mnemonic marks: `_Open` is `Open`, and `a__b` is `a_b`.
#[must_use]
pub fn strip_mnemonic(label: &str) -> String {
    let mut text = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(char) = chars.next() {
        if char != '_' {
            text.push(char);
        } else if chars.next_if_eq(&'_').is_some() {
            text.push('_');
        }
    }
    text
}

/// Returns the value in `value`: a property of a menu entry is a variant in a variant.
fn inner<'a, 'v>(value: &'a Variant<'v>) -> &'a Variant<'v> {
    let mut value = value;
    while let Variant::Variant(next) = value {
        value = next;
    }
    value
}

/// Returns the entries for the shell from `children`, the entries `(ia{sv}av)` of a menu of a program.
///
/// An entry that the program hides is not in the result. `depth` is the depth
/// of `children`, and the entries below [`MAX_MENU_DEPTH`] are not read.
/// `budget` is the number of entries that the result can still have.
#[must_use]
pub fn layout_to_items(
    children: &[Variant<'_>],
    depth: usize,
    budget: &mut usize,
) -> Vec<TrayMenuItem> {
    let mut items = Vec::new();
    if depth >= MAX_MENU_DEPTH {
        return items;
    }
    for child in children {
        if *budget == 0 {
            break;
        }
        let Variant::Structure(entry) = inner(child) else {
            continue;
        };
        let [
            Variant::I32(id),
            Variant::Dict(properties),
            Variant::Array(submenu),
        ] = entry.fields()
        else {
            continue;
        };
        let property = |name: &str| {
            properties
                .iter()
                .find(|(key, _)| matches!(key, Variant::Str(key) if key.as_str() == name))
                .map(|(_, value)| inner(value))
        };
        let text = |name: &str| match property(name) {
            Some(Variant::Str(text)) => text.as_str(),
            _ => "",
        };
        if matches!(property("visible"), Some(Variant::Bool(false))) {
            continue;
        }
        *budget -= 1;
        // The icon is a PNG file. A different file does not go to the shell.
        let icon = match property("icon-data") {
            Some(Variant::Array(bytes)) if bytes.len() <= MAX_MENU_ICON => bytes
                .inner()
                .iter()
                .map(|byte| match byte {
                    Variant::U8(byte) => Some(*byte),
                    _ => None,
                })
                .collect::<Option<Vec<u8>>>()
                .filter(|bytes| bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
                .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
            _ => None,
        };
        items.push(TrayMenuItem {
            id: *id,
            label: clip(&strip_mnemonic(text("label"))),
            enabled: !matches!(property("enabled"), Some(Variant::Bool(false))),
            separator: text("type") == "separator",
            toggle: match text("toggle-type") {
                "checkmark" => Some(TrayToggle::Check),
                "radio" => Some(TrayToggle::Radio),
                _ => None,
            },
            checked: matches!(property("toggle-state"), Some(Variant::I32(1))),
            icon,
            children: layout_to_items(submenu.inner(), depth + 1, budget),
        });
    }
    items
}

/// Returns one icon from the value `(iiay)`, or `None` if it is not valid.
/// The size is checked before the pixels are copied.
fn pixmap_from(value: &Variant<'_>) -> Option<Pixmap> {
    let Variant::Structure(structure) = value else {
        return None;
    };
    let [
        Variant::I32(width),
        Variant::I32(height),
        Variant::Array(bytes),
    ] = structure.fields()
    else {
        return None;
    };
    let side = |value: i32| {
        (1..=MAX_SIDE)
            .contains(&value)
            .then(|| usize::try_from(value).ok())
            .flatten()
    };
    if bytes.len() != side(*width)? * side(*height)? * 4 {
        return None;
    }
    let pixels = bytes
        .inner()
        .iter()
        .map(|byte| match byte {
            Variant::U8(byte) => Some(*byte),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()?;
    Some((*width, *height, pixels))
}

/// Returns the properties of an item from the answer of `GetAll`.
/// A property that is missing or has a different type stays empty.
fn properties_from(values: &HashMap<String, OwnedVariant>) -> Properties {
    let get = |key: &str| values.get(key).map(|value| &**value);
    let text = |key: &str| match get(key) {
        Some(Variant::Str(text)) => clip(text.as_str()),
        _ => String::new(),
    };
    let pixmaps = |key: &str| match get(key) {
        Some(Variant::Array(array)) => array
            .inner()
            .iter()
            .take(MAX_PIXMAPS)
            .filter_map(pixmap_from)
            .collect(),
        _ => Vec::new(),
    };
    // ToolTip is (icon name, icon pixmaps, title, text). The text can have markup, the shell shows the title only.
    let tooltip = match get("ToolTip") {
        Some(Variant::Structure(structure)) => match structure.fields() {
            [_, _, Variant::Str(title), _] => clip(title.as_str()),
            _ => String::new(),
        },
        _ => String::new(),
    };
    Properties {
        id: text("Id"),
        title: text("Title"),
        status: text("Status"),
        icon_name: text("IconName"),
        icon_theme_path: text("IconThemePath"),
        icon_pixmap: pixmaps("IconPixmap"),
        attention_icon_name: text("AttentionIconName"),
        attention_icon_pixmap: pixmaps("AttentionIconPixmap"),
        tooltip,
        menu: match get("Menu") {
            Some(Variant::ObjectPath(path)) => path.as_str().to_owned(),
            _ => String::new(),
        },
        item_is_menu: matches!(get("ItemIsMenu"), Some(Variant::Bool(true))),
    }
}

type EventSink = Arc<dyn Fn(TrayEvent) + Send + Sync>;

/// One registered item.
struct Entry {
    /// The unique bus name of the program.
    owner: String,
    /// The object path of the item.
    path: String,
    /// The item for the shell, after the agent read it.
    item: Option<TrayItem>,
    /// The object path of the menu of the item.
    menu: Option<String>,
    /// The item has no action for a click: the agent learned it from a click that the program refused.
    menu_only: bool,
    /// The task that follows the signals of the item.
    task: Option<AbortHandle>,
}

impl Entry {
    /// Returns the name of the item in the list of the watcher.
    fn service(&self) -> String {
        format!("{}{}", self.owner, self.path)
    }
}

#[derive(Default)]
struct Inner {
    items: BTreeMap<u32, Entry>,
    next_id: u32,
    viewers: HashMap<u64, EventSink>,
}

/// The items of the system tray and the tray channels of the shell.
pub struct Tray {
    inner: Mutex<Inner>,
    next_viewer: AtomicU64,
    requests: mpsc::UnboundedSender<TrayRequest>,
    receiver: Mutex<Option<mpsc::UnboundedReceiver<TrayRequest>>>,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray").finish_non_exhaustive()
    }
}

/// One open tray channel. Dropping it removes the channel.
pub struct ViewerGuard {
    tray: Arc<Tray>,
    id: u64,
}

impl std::fmt::Debug for ViewerGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerGuard").field("id", &self.id).finish()
    }
}

impl Drop for ViewerGuard {
    fn drop(&mut self) {
        self.tray.lock().viewers.remove(&self.id);
    }
}

impl Tray {
    /// Returns an empty tray.
    #[must_use]
    pub fn new() -> Arc<Self> {
        let (requests, receiver) = mpsc::unbounded_channel();
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            next_viewer: AtomicU64::new(1),
            requests,
            receiver: Mutex::new(Some(receiver)),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn broadcast(&self, event: &TrayEvent) {
        let viewers: Vec<EventSink> = self.lock().viewers.values().cloned().collect();
        for send in viewers {
            send(event.clone());
        }
    }

    /// Adds a tray channel. It gets all items at once, then each change.
    pub fn add_viewer(
        self: &Arc<Self>,
        send: impl Fn(TrayEvent) + Send + Sync + 'static,
    ) -> ViewerGuard {
        let id = self.next_viewer.fetch_add(1, Ordering::Relaxed);
        let send: EventSink = Arc::new(send);
        let items = {
            let mut inner = self.lock();
            inner.viewers.insert(id, send.clone());
            inner
                .items
                .values()
                .filter_map(|entry| entry.item.clone())
                .collect()
        };
        send(TrayEvent::Items { items });
        ViewerGuard {
            tray: self.clone(),
            id,
        }
    }

    /// Adds the item at `path` of the program with the unique bus name `owner`.
    ///
    /// Returns the id of the new item. Returns `None` if the item is registered
    /// already, or if the tray has [`MAX_ITEMS`] items. The shell does not get
    /// the item before [`Tray::update`].
    pub fn register(&self, owner: &str, path: &str) -> Option<u32> {
        let mut inner = self.lock();
        let known = inner
            .items
            .values()
            .any(|entry| entry.owner == owner && entry.path == path);
        if known || inner.items.len() >= MAX_ITEMS {
            return None;
        }
        inner.next_id = inner.next_id.wrapping_add(1).max(1);
        let id = inner.next_id;
        inner.items.insert(
            id,
            Entry {
                owner: owner.to_owned(),
                path: path.to_owned(),
                item: None,
                menu: None,
                menu_only: false,
                task: None,
            },
        );
        Some(id)
    }

    /// Gives the item `id` the task that follows its signals. The task stops when the item goes.
    fn set_task(&self, id: u32, task: AbortHandle) {
        match self.lock().items.get_mut(&id) {
            Some(entry) => entry.task = Some(task),
            // The program left the bus before the task started.
            None => task.abort(),
        }
    }

    /// Sets what the shell shows for the item `id`, and sends it to the shell if it changed.
    pub fn update(&self, id: u32, mut item: TrayItem) {
        {
            let mut inner = self.lock();
            let Some(entry) = inner.items.get_mut(&id) else {
                return;
            };
            item.menu_only |= entry.menu_only;
            if entry.item.as_ref() == Some(&item) {
                return;
            }
            entry.item = Some(item.clone());
        }
        self.broadcast(&TrayEvent::Item { item });
    }

    /// Removes each item of the program with the unique bus name `owner`.
    /// Returns the names of the removed items in the list of the watcher.
    pub fn remove_owner(&self, owner: &str) -> Vec<String> {
        let removed: Vec<(u32, Entry)> = {
            let mut inner = self.lock();
            let ids: Vec<u32> = inner
                .items
                .iter()
                .filter(|(_, entry)| entry.owner == owner)
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| inner.items.remove(&id).map(|entry| (id, entry)))
                .collect()
        };
        removed
            .into_iter()
            .map(|(id, entry)| {
                if let Some(task) = &entry.task {
                    task.abort();
                }
                if entry.item.is_some() {
                    self.broadcast(&TrayEvent::Removed { id });
                }
                entry.service()
            })
            .collect()
    }

    /// Returns the unique bus name and the object path of the item `id`.
    #[must_use]
    pub fn address(&self, id: u32) -> Option<(String, String)> {
        self.lock()
            .items
            .get(&id)
            .map(|entry| (entry.owner.clone(), entry.path.clone()))
    }

    /// Sets the object path of the menu of the item `id`, or `None` for an item with no menu.
    pub fn set_menu_path(&self, id: u32, menu: Option<String>) {
        if let Some(entry) = self.lock().items.get_mut(&id) {
            entry.menu = menu;
        }
    }

    /// Returns the object path of the menu of the item `id`.
    #[must_use]
    pub fn menu_path(&self, id: u32) -> Option<String> {
        self.lock()
            .items
            .get(&id)
            .and_then(|entry| entry.menu.clone())
    }

    /// Marks the item `id` as an item with no action for a click, and tells the shell.
    /// The mark stays when the agent reads the item again.
    pub fn set_menu_only(&self, id: u32) {
        let item = {
            let mut inner = self.lock();
            let Some(entry) = inner.items.get_mut(&id) else {
                return;
            };
            entry.menu_only = true;
            match entry.item.as_mut() {
                Some(item) if !item.menu_only => {
                    item.menu_only = true;
                    item.clone()
                }
                _ => return,
            }
        };
        self.broadcast(&TrayEvent::Item { item });
    }

    /// Sends the menu of the item `id` to the shell.
    pub fn show_menu(&self, id: u32, items: Vec<TrayMenuItem>) {
        self.broadcast(&TrayEvent::Menu { id, items });
    }

    /// Returns the names of the items in the list of the watcher.
    #[must_use]
    pub fn services(&self) -> Vec<String> {
        self.lock().items.values().map(Entry::service).collect()
    }

    /// Returns the items that the shell shows.
    #[must_use]
    pub fn items(&self) -> Vec<TrayItem> {
        self.lock()
            .items
            .values()
            .filter_map(|entry| entry.item.clone())
            .collect()
    }

    /// Takes a request of the shell. The D-Bus service sends it to the program.
    pub fn request(&self, request: TrayRequest) {
        let _ = self.requests.send(request);
    }

    /// Takes the queue of the requests of the shell. The D-Bus service takes it one time.
    pub fn take_requests(&self) -> Option<mpsc::UnboundedReceiver<TrayRequest>> {
        self.receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

/// The object `org.kde.StatusNotifierWatcher` on the session bus.
struct WatcherObject {
    tray: Arc<Tray>,
    connection: Connection,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl WatcherObject {
    /// Registers an item. `service` is a bus name, or the object path of the item of the caller.
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let Some((name, path)) = split_service(service, &sender) else {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "{service} is not a bus name and not an object path."
            )));
        };
        // The item is known by the unique name of its program: the agent sees when that name leaves the bus.
        let owner = if name.starts_with(':') {
            name
        } else {
            zbus::fdo::DBusProxy::new(&self.connection)
                .await?
                .get_name_owner(zbus::names::BusName::try_from(name.as_str())?)
                .await?
                .to_string()
        };
        let Some(id) = self.tray.register(&owner, &path) else {
            // The item is registered already, or the tray is full.
            return Ok(());
        };
        tracing::debug!(id, owner, path, "a tray item registered");
        let task = tokio::spawn(watch(
            self.tray.clone(),
            self.connection.clone(),
            id,
            owner.clone(),
            path.clone(),
        ));
        self.tray.set_task(id, task.abort_handle());
        Self::status_notifier_item_registered(&emitter, &format!("{owner}{path}")).await?;
        Ok(())
    }

    /// The agent is the only host. A different host can register, and the agent ignores it.
    #[allow(clippy::unused_self)]
    fn register_status_notifier_host(&self, _service: &str) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.tray.services()
    }

    /// The agent shows the items, thus a host is always registered.
    #[zbus(property)]
    #[allow(clippy::unused_self)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    #[allow(clippy::unused_self)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// Reads the properties of the item at `path` of `owner`. Returns `None` if the program does not answer in time.
async fn read(connection: &Connection, owner: &str, path: &str) -> Option<Properties> {
    let call = connection.call_method(
        Some(owner),
        path,
        Some("org.freedesktop.DBus.Properties"),
        "GetAll",
        &(ITEM_INTERFACE,),
    );
    let reply = tokio::time::timeout(CALL_TIMEOUT, call).await.ok()?.ok()?;
    let values: HashMap<String, OwnedVariant> = reply.body().deserialize().ok()?;
    Some(properties_from(&values))
}

/// Reads the item `id` and gives it to the shell. The icon files are read on a blocking thread.
async fn refresh(tray: &Tray, connection: &Connection, id: u32, owner: &str, path: &str) {
    let Some(properties) = read(connection, owner, path).await else {
        tracing::debug!(id, owner, path, "a tray item did not give its properties");
        return;
    };
    tray.set_menu_path(id, menu_path_of(&properties).map(str::to_owned));
    let item = tokio::task::spawn_blocking(move || item_from(id, &properties, &data_dirs())).await;
    if let Ok(item) = item {
        tray.update(id, item);
    }
}

/// Follows the item `id`: reads it, and reads it again after each of its signals.
async fn watch(tray: Arc<Tray>, connection: Connection, id: u32, owner: String, path: String) {
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(owner.as_str())
        .path(path.as_str())
        .interface(ITEM_INTERFACE)
        .build();
    // The signals come first: a change between the first read and the match rule is not lost.
    let mut signals = match rule {
        Ok(rule) => {
            MessageStream::for_match_rule(rule, &connection, std::num::NonZeroUsize::new(16))
                .await
                .ok()
        }
        Err(_) => None,
    };
    refresh(&tray, &connection, id, &owner, &path).await;
    let Some(signals) = signals.as_mut() else {
        return;
    };
    while signals.next().await.is_some() {
        tokio::time::sleep(SETTLE).await;
        while signals.next().now_or_never().flatten().is_some() {}
        refresh(&tray, &connection, id, &owner, &path).await;
    }
}

/// Why a call to a program failed.
enum CallError {
    /// The program answered with an error, for example for a method that it does not have.
    Refused(zbus::Error),
    /// The program did not answer in [`CALL_TIMEOUT`].
    Timeout,
}

/// Calls the method `member` of `interface` at `path` of the program `owner`, and waits for the answer.
async fn call<B>(
    connection: &Connection,
    owner: &str,
    path: &str,
    interface: &str,
    member: &str,
    body: &B,
) -> Result<zbus::Message, CallError>
where
    B: serde::Serialize + zbus::DynamicType + Sync,
{
    let call = connection.call_method(Some(owner), path, Some(interface), member, body);
    match tokio::time::timeout(CALL_TIMEOUT, call).await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(error)) => {
            tracing::debug!(%error, member, owner, "a tray item refused a call");
            Err(CallError::Refused(error))
        }
        Err(_) => {
            tracing::debug!(member, owner, "a tray item did not answer a call");
            Err(CallError::Timeout)
        }
    }
}

/// Reads the menu at `path` of the program `owner`. Returns `None` if the program does not give it.
async fn read_menu(connection: &Connection, owner: &str, path: &str) -> Option<Vec<TrayMenuItem>> {
    // The program can fill the menu now. Not each program has this method.
    let _ = call(
        connection,
        owner,
        path,
        MENU_INTERFACE,
        "AboutToShow",
        &(0_i32,),
    )
    .await;
    // The full tree from the root, with all properties.
    let body = (0_i32, -1_i32, Vec::<&str>::new());
    let reply = call(connection, owner, path, MENU_INTERFACE, "GetLayout", &body)
        .await
        .ok()?;
    let body = reply.body();
    let layout: zbus::Structure<'_> = body.deserialize().ok()?;
    let [_revision, Variant::Structure(root)] = layout.fields() else {
        return None;
    };
    let [_, _, Variant::Array(children)] = root.fields() else {
        return None;
    };
    let mut budget = MAX_MENU_ITEMS;
    Some(layout_to_items(children.inner(), 0, &mut budget))
}

/// Reads the menu of the item `id` and sends it to the shell.
async fn open_menu(tray: &Tray, connection: &Connection, id: u32, owner: &str) {
    let Some(path) = tray.menu_path(id) else {
        return;
    };
    match read_menu(connection, owner, &path).await {
        Some(items) => tray.show_menu(id, items),
        None => tracing::debug!(id, owner, path, "a tray item did not give its menu"),
    }
}

/// Does a request of the shell: a method call on the item of a program, or on its menu.
async fn handle(tray: Arc<Tray>, connection: Connection, request: TrayRequest) {
    let id = item_of(&request);
    let Some((owner, path)) = tray.address(id) else {
        return;
    };
    let item = |member: &'static str| (owner.as_str(), path.as_str(), ITEM_INTERFACE, member);
    match request {
        TrayRequest::Activate { x, y, .. } => {
            let (owner, path, interface, member) = item("Activate");
            let done = call(&connection, owner, path, interface, member, &(x, y)).await;
            // An item of libappindicator has no Activate: its menu is its only action.
            // The bus gives a standard error, for example UnknownMethod, as an FDO error.
            if matches!(
                done,
                Err(CallError::Refused(
                    zbus::Error::MethodError(..) | zbus::Error::FDO(_)
                ))
            ) && tray.menu_path(id).is_some()
            {
                tray.set_menu_only(id);
                open_menu(&tray, &connection, id, owner).await;
            }
        }
        TrayRequest::SecondaryActivate { x, y, .. } => {
            let (owner, path, interface, member) = item("SecondaryActivate");
            let _ = call(&connection, owner, path, interface, member, &(x, y)).await;
        }
        TrayRequest::Scroll {
            delta, horizontal, ..
        } => {
            let (owner, path, interface, member) = item("Scroll");
            let orientation = if horizontal { "horizontal" } else { "vertical" };
            let _ = call(
                &connection,
                owner,
                path,
                interface,
                member,
                &(delta, orientation),
            )
            .await;
        }
        TrayRequest::Menu { .. } => open_menu(&tray, &connection, id, &owner).await,
        TrayRequest::MenuClick { item, .. } => {
            if let Some(menu) = tray.menu_path(id) {
                // The event has the id of the entry, the name, a value that `clicked` does not use, and a time.
                let body = (item, "clicked", Variant::from(0_i32), 0_u32);
                let _ = call(&connection, &owner, &menu, MENU_INTERFACE, "Event", &body).await;
            }
        }
    }
}

/// Returns the id of the item of `request`.
const fn item_of(request: &TrayRequest) -> u32 {
    match request {
        TrayRequest::Activate { id, .. }
        | TrayRequest::SecondaryActivate { id, .. }
        | TrayRequest::Scroll { id, .. }
        | TrayRequest::Menu { id }
        | TrayRequest::MenuClick { id, .. } => *id,
    }
}

/// Serves the watcher on `connection` under the bus name `name`, and sends the requests of the shell to the items.
///
/// The agent does not take the name from a different owner, for example the
/// tray of a local desktop. It waits in the queue of the bus and gets the name
/// when the other owner gives it up (ADR-054).
///
/// # Errors
///
/// A D-Bus error if the bus refuses the object or the request for the name.
pub async fn serve(tray: Arc<Tray>, connection: &Connection, name: &str) -> zbus::Result<()> {
    let Some(mut requests) = tray.take_requests() else {
        return Ok(());
    };
    connection
        .object_server()
        .at(
            WATCHER_PATH,
            WatcherObject {
                tray: tray.clone(),
                connection: connection.clone(),
            },
        )
        .await?;
    let emitter = SignalEmitter::new(connection, WATCHER_PATH)?.into_owned();
    // An item goes when its program leaves the bus.
    let mut changes = zbus::fdo::DBusProxy::new(connection)
        .await?
        .receive_name_owner_changed()
        .await?;
    {
        let tray = tray.clone();
        let emitter = emitter.clone();
        tokio::spawn(async move {
            while let Some(change) = changes.next().await {
                let Ok(args) = change.args() else {
                    continue;
                };
                if args.new_owner().is_some() {
                    continue;
                }
                for service in tray.remove_owner(args.name().as_str()) {
                    let _ =
                        WatcherObject::status_notifier_item_unregistered(&emitter, &service).await;
                }
            }
        });
    }
    match connection
        .request_name_with_flags(name, RequestNameFlags::empty())
        .await?
    {
        RequestNameReply::InQueue => {
            tracing::warn!(
                name,
                "a different program owns the tray name, the agent waits for it"
            );
        }
        _ => tracing::info!(name, "serving the system tray"),
    }
    // A program that started before the agent registers its item again after this signal.
    if let Err(error) = WatcherObject::status_notifier_host_registered(&emitter).await {
        tracing::warn!(%error, "the tray host signal did not go");
    }
    let connection = connection.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            // Each request has its own task: a program that does not answer does not stop the other items.
            tokio::spawn(handle(tray.clone(), connection.clone(), request));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(tray: &Arc<Tray>) -> (ViewerGuard, std::sync::mpsc::Receiver<TrayEvent>) {
        let (send, events) = std::sync::mpsc::channel();
        let guard = tray.add_viewer(move |event| {
            let _ = send.send(event);
        });
        (guard, events)
    }

    fn item(id: u32, title: &str) -> TrayItem {
        item_from(
            id,
            &Properties {
                id: title.to_lowercase(),
                title: title.to_owned(),
                status: "Active".to_owned(),
                ..Properties::default()
            },
            &[],
        )
    }

    #[test]
    fn a_program_registers_with_its_bus_name_or_with_an_object_path() {
        assert_eq!(
            split_service("/org/ayatana/NotificationItem/app", ":1.42"),
            Some((
                ":1.42".to_owned(),
                "/org/ayatana/NotificationItem/app".to_owned()
            ))
        );
        assert_eq!(
            split_service("org.kde.StatusNotifierItem-7-1", ":1.42"),
            Some((
                "org.kde.StatusNotifierItem-7-1".to_owned(),
                "/StatusNotifierItem".to_owned()
            ))
        );
        assert_eq!(
            split_service(":1.9", ":1.42"),
            Some((":1.9".to_owned(), "/StatusNotifierItem".to_owned()))
        );
        assert_eq!(split_service("", ":1.42"), None);
        assert_eq!(split_service("no dots", ":1.42"), None);
        assert_eq!(split_service("/bad path/", ":1.42"), None);
    }

    #[test]
    fn the_status_has_three_values_and_an_unknown_one_is_active() {
        assert_eq!(status_of("Passive"), TrayStatus::Passive);
        assert_eq!(status_of("NeedsAttention"), TrayStatus::NeedsAttention);
        assert_eq!(status_of("Active"), TrayStatus::Active);
        assert_eq!(status_of(""), TrayStatus::Active);
    }

    #[test]
    fn the_best_icon_is_the_smallest_of_at_least_48_pixels_and_a_wrong_size_does_not_count() {
        let pixmap = |side: i32| {
            let pixels = usize::try_from(side * side * 4).unwrap();
            (side, side, vec![0; pixels])
        };
        let all = [pixmap(16), pixmap(64), pixmap(48), pixmap(256)];
        assert_eq!(best_pixmap(&all).unwrap().0, 48);
        assert_eq!(best_pixmap(&[pixmap(16), pixmap(24)]).unwrap().0, 24);
        // The pixels do not match the size, or the size is above the limit.
        assert_eq!(best_pixmap(&[(48, 48, vec![0; 8])]), None);
        assert_eq!(
            best_pixmap(&[(0, 4, Vec::new()), (-1, -1, Vec::new())]),
            None
        );
        assert_eq!(best_pixmap(&[(1024, 1, vec![0; 4096])]), None);
        assert_eq!(best_pixmap(&[]), None);
    }

    #[test]
    fn the_alpha_byte_moves_to_the_end() {
        assert_eq!(
            argb_to_rgba(&[255, 1, 2, 3, 128, 4, 5, 6, 9]),
            [1, 2, 3, 255, 4, 5, 6, 128]
        );
    }

    #[test]
    fn the_title_falls_back_to_the_tooltip_and_to_the_name() {
        let properties = Properties {
            id: "seafile".to_owned(),
            tooltip: "Seafile: all files are in sync".to_owned(),
            menu: "/MenuBar".to_owned(),
            ..Properties::default()
        };
        let shown = item_from(1, &properties, &[]);
        assert_eq!(shown.title, "Seafile: all files are in sync");
        assert_eq!(shown.tooltip, None);
        assert!(shown.menu);
        let named = item_from(
            1,
            &Properties {
                tooltip: String::new(),
                menu: "/NO_DBUSMENU".to_owned(),
                ..properties
            },
            &[],
        );
        assert_eq!(named.title, "seafile");
        assert!(!named.menu);
        assert_eq!(item_from(1, &Properties::default(), &[]).title, "Tray item");
    }

    #[test]
    fn the_icon_is_a_png_of_the_pixels_and_the_attention_icon_comes_first() {
        let red = vec![255, 255, 0, 0];
        let blue = vec![255, 0, 0, 255];
        let properties = Properties {
            status: "Active".to_owned(),
            icon_pixmap: vec![(1, 1, red.clone())],
            attention_icon_pixmap: vec![(1, 1, blue.clone())],
            ..Properties::default()
        };
        let png = |argb: &[u8]| {
            base64::engine::general_purpose::STANDARD
                .encode(farglass_surface::cursor::png_rgba(1, 1, &argb_to_rgba(argb)).unwrap())
        };
        let usual = item_from(1, &properties, &[]);
        assert_eq!(usual.icon, Some(png(&red)));
        assert_eq!(usual.icon_mime.as_deref(), Some("image/png"));
        let attention = item_from(
            1,
            &Properties {
                status: "NeedsAttention".to_owned(),
                ..properties.clone()
            },
            &[],
        );
        assert_eq!(attention.icon, Some(png(&blue)));
        // With no attention icon, the usual icon stays.
        let fallback = item_from(
            1,
            &Properties {
                status: "NeedsAttention".to_owned(),
                attention_icon_pixmap: Vec::new(),
                ..properties
            },
            &[],
        );
        assert_eq!(fallback.icon, Some(png(&red)));
    }

    #[test]
    fn a_named_icon_comes_from_the_theme_path_of_the_item() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tray-icon.svg"), "<svg/>").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "no icon").unwrap();
        let properties = Properties {
            icon_name: "tray-icon".to_owned(),
            icon_theme_path: dir.path().to_str().unwrap().to_owned(),
            ..Properties::default()
        };
        let shown = item_from(1, &properties, &[]);
        assert_eq!(shown.icon_mime.as_deref(), Some("image/svg+xml"));
        assert_eq!(
            shown.icon,
            Some(base64::engine::general_purpose::STANDARD.encode("<svg/>"))
        );
        // Only SVG and PNG files, and no path out of the directory.
        for name in ["secret", "secret.txt", "../tray-icon", ".hidden"] {
            let other = Properties {
                icon_name: name.to_owned(),
                ..properties.clone()
            };
            assert_eq!(item_from(1, &other, &[]).icon, None, "{name}");
        }
    }

    #[test]
    fn a_named_icon_comes_from_hicolor_first_and_then_from_a_different_theme() {
        let dir = tempfile::tempdir().unwrap();
        let file = |path: &str, text: &str| {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        file("icons/gnome/48x48/status/dialog-information.png", "gnome");
        file("icons/breeze/status/22/dialog-information.svg", "breeze");
        file("icons/gnome/48x48/status/sync-ok.png", "gnome");
        file("icons/hicolor/32x32/status/sync-ok.png", "hicolor");
        let dirs = [dir.path().to_path_buf()];
        let found = |name: &str| {
            find_tray_icon(&dirs, "", name).map(|(path, _)| std::fs::read_to_string(path).unwrap())
        };
        // The breeze theme comes before the gnome theme, and hicolor comes before both.
        assert_eq!(found("dialog-information").as_deref(), Some("breeze"));
        assert_eq!(found("sync-ok").as_deref(), Some("hicolor"));
        assert_eq!(found("missing"), None);
    }

    #[test]
    fn a_label_has_no_mnemonic_marks() {
        assert_eq!(strip_mnemonic("_Show window"), "Show window");
        assert_eq!(strip_mnemonic("Save _As…"), "Save As…");
        assert_eq!(strip_mnemonic("snake__case"), "snake_case");
        assert_eq!(strip_mnemonic("_"), "");
        assert_eq!(strip_mnemonic("plain"), "plain");
    }

    /// Returns an entry `(ia{sv}av)` of a menu, as a program sends it.
    fn entry(
        id: i32,
        properties: &[(&str, Variant<'static>)],
        children: Vec<Variant<'static>>,
    ) -> Variant<'static> {
        let properties: HashMap<String, Variant<'static>> = properties
            .iter()
            .map(|(key, value)| {
                (
                    (*key).to_owned(),
                    Variant::Variant(Box::new(value.try_clone().unwrap())),
                )
            })
            .collect();
        Variant::Variant(Box::new(Variant::from((id, properties, children))))
    }

    #[test]
    fn a_menu_becomes_a_tree_with_states_and_with_no_hidden_entry() {
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let children = vec![
            entry(
                1,
                &[("label", "_Show".into()), ("icon-data", png.clone().into())],
                Vec::new(),
            ),
            entry(
                2,
                &[
                    ("label", "Sync".into()),
                    ("toggle-type", "checkmark".into()),
                    ("toggle-state", 1_i32.into()),
                ],
                Vec::new(),
            ),
            entry(3, &[("type", "separator".into())], Vec::new()),
            entry(
                4,
                &[("label", "Hidden".into()), ("visible", false.into())],
                Vec::new(),
            ),
            entry(
                5,
                &[
                    ("label", "More".into()),
                    ("children-display", "submenu".into()),
                ],
                vec![
                    entry(
                        6,
                        &[
                            ("label", "Slow".into()),
                            ("toggle-type", "radio".into()),
                            ("toggle-state", 0_i32.into()),
                        ],
                        Vec::new(),
                    ),
                    entry(
                        7,
                        &[
                            ("label", "Quit".into()),
                            ("enabled", false.into()),
                            ("icon-data", b"<html>".to_vec().into()),
                        ],
                        Vec::new(),
                    ),
                ],
            ),
        ];
        let mut budget = MAX_MENU_ITEMS;
        let items = layout_to_items(&children, 0, &mut budget);
        assert_eq!(budget, MAX_MENU_ITEMS - 6);
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            [1, 2, 3, 5]
        );
        assert_eq!(items[0].label, "Show");
        assert_eq!(
            items[0].icon,
            Some(base64::engine::general_purpose::STANDARD.encode(&png))
        );
        assert!(items[0].enabled && !items[0].separator);
        assert_eq!(
            (items[1].toggle, items[1].checked),
            (Some(TrayToggle::Check), true)
        );
        assert!(items[2].separator);
        let more = &items[3].children;
        assert_eq!(
            (more[0].toggle, more[0].checked),
            (Some(TrayToggle::Radio), false)
        );
        assert_eq!(more[1].label, "Quit");
        assert!(!more[1].enabled);
        // The icon is not a PNG file.
        assert_eq!(more[1].icon, None);
    }

    #[test]
    fn a_menu_has_a_limit_of_entries_and_of_depth() {
        let flat: Vec<Variant<'static>> = (0..10).map(|id| entry(id, &[], Vec::new())).collect();
        let mut budget = 4;
        assert_eq!(layout_to_items(&flat, 0, &mut budget).len(), 4);
        assert_eq!(budget, 0);

        let mut deep = entry(100, &[], Vec::new());
        for id in 0..12 {
            deep = entry(id, &[], vec![deep]);
        }
        let mut budget = MAX_MENU_ITEMS;
        let mut level = layout_to_items(&[deep], 0, &mut budget);
        let mut depth = 0;
        while let Some(item) = level.pop() {
            depth += 1;
            level = item.children;
        }
        assert_eq!(depth, MAX_MENU_DEPTH);
    }

    #[test]
    fn an_item_with_no_click_action_stays_a_menu_after_the_agent_reads_it_again() {
        let tray = Tray::new();
        let id = tray.register(":1.5", "/StatusNotifierItem").unwrap();
        tray.set_menu_path(id, Some("/MenuBar".to_owned()));
        tray.update(id, item(id, "Probe"));
        let (_guard, events) = shown(&tray);
        events.try_recv().unwrap();

        tray.set_menu_only(id);
        let TrayEvent::Item { item: marked } = events.try_recv().unwrap() else {
            panic!("no item");
        };
        assert!(marked.menu_only);
        // The mark is set one time, and the properties of the program do not remove it.
        tray.set_menu_only(id);
        tray.update(id, item(id, "Probe"));
        assert!(events.try_recv().is_err());
        assert!(tray.items()[0].menu_only);
        assert_eq!(tray.menu_path(id).as_deref(), Some("/MenuBar"));

        tray.show_menu(id, Vec::new());
        assert_eq!(
            events.try_recv().unwrap(),
            TrayEvent::Menu {
                id,
                items: Vec::new()
            }
        );
    }

    #[test]
    fn the_properties_come_from_the_values_of_the_program() {
        let pixels = vec![255_u8, 1, 2, 3];
        let values: HashMap<String, OwnedVariant> = [
            ("Id", Variant::from("bitwarden")),
            ("Title", Variant::from("Bitwarden\nvault")),
            ("Status", Variant::from("Passive")),
            (
                "IconPixmap",
                Variant::from(vec![(1_i32, 1_i32, pixels.clone()), (2, 2, pixels.clone())]),
            ),
            (
                "ToolTip",
                Variant::from((
                    "",
                    Vec::<(i32, i32, Vec<u8>)>::new(),
                    "Locked",
                    "<b>text</b>",
                )),
            ),
            (
                "Menu",
                Variant::from(zbus::ObjectPath::try_from("/MenuBar").unwrap()),
            ),
            ("ItemIsMenu", Variant::from(true)),
            // A wrong type stays empty.
            ("IconName", Variant::from(7_u32)),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.try_into().unwrap()))
        .collect();
        let properties = properties_from(&values);
        assert_eq!(properties.id, "bitwarden");
        assert_eq!(properties.title, "Bitwarden vault");
        assert_eq!(properties.status, "Passive");
        // The second icon has pixels that do not match its size.
        assert_eq!(properties.icon_pixmap, vec![(1, 1, pixels)]);
        assert_eq!(properties.tooltip, "Locked");
        assert_eq!(properties.menu, "/MenuBar");
        assert!(properties.item_is_menu);
        assert_eq!(properties.icon_name, "");
    }

    #[test]
    fn a_channel_gets_all_items_at_once_and_then_each_change() {
        let tray = Tray::new();
        let first = tray.register(":1.5", "/StatusNotifierItem").unwrap();
        tray.update(first, item(first, "Bitwarden"));
        // An item that the agent did not read yet does not show.
        tray.register(":1.6", "/StatusNotifierItem").unwrap();

        let (_guard, events) = shown(&tray);
        assert_eq!(
            events.try_recv().unwrap(),
            TrayEvent::Items {
                items: vec![item(first, "Bitwarden")]
            }
        );
        // The same values again send nothing.
        tray.update(first, item(first, "Bitwarden"));
        assert!(events.try_recv().is_err());
        tray.update(first, item(first, "Locked"));
        assert_eq!(
            events.try_recv().unwrap(),
            TrayEvent::Item {
                item: item(first, "Locked")
            }
        );
    }

    #[test]
    fn an_item_registers_one_time_and_goes_with_its_program() {
        let tray = Tray::new();
        let first = tray.register(":1.5", "/StatusNotifierItem").unwrap();
        assert_eq!(tray.register(":1.5", "/StatusNotifierItem"), None);
        let second = tray.register(":1.5", "/Second").unwrap();
        let other = tray.register(":1.6", "/StatusNotifierItem").unwrap();
        tray.update(first, item(first, "One"));
        tray.update(other, item(other, "Other"));
        let (_guard, events) = shown(&tray);
        events.try_recv().unwrap();

        assert_eq!(
            tray.remove_owner(":1.5"),
            [":1.5/StatusNotifierItem", ":1.5/Second"]
        );
        // The shell did not get the second item, thus it gets no removal of it.
        assert_eq!(events.try_recv().unwrap(), TrayEvent::Removed { id: first });
        assert!(events.try_recv().is_err());
        assert_eq!(tray.address(second), None);
        assert_eq!(tray.services(), [":1.6/StatusNotifierItem"]);
        assert_eq!(tray.items(), vec![item(other, "Other")]);
        assert!(tray.remove_owner(":1.5").is_empty());
        // An update of an item that is gone does nothing.
        tray.update(first, item(first, "One"));
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn the_tray_takes_no_item_above_the_limit() {
        let tray = Tray::new();
        for index in 0..MAX_ITEMS {
            assert!(tray.register(":1.5", &format!("/Item{index}")).is_some());
        }
        assert_eq!(tray.register(":1.5", "/OneMore"), None);
        tray.remove_owner(":1.5");
        assert!(tray.register(":1.5", "/OneMore").is_some());
    }
}
