//! tray-item is a status notifier item with a menu, for the end-to-end test of the system tray.
//!
//! Usage:
//!
//!     cargo run -p farglass-session --example tray-item -- WATCHER
//!
//! WATCHER is the bus name of the watcher, for example the value of
//! FARGLASS_TRAY_NAME of the agent. The item registers there with an object
//! path, as an Electron program does.
//!
//! The program prints one line for each call that it gets: `registered`,
//! `activate X Y`, and `event ID NAME`. The menu entry Quit, with the id 7,
//! stops the program, and its item leaves the tray.

use std::collections::HashMap;

use zbus::{OwnedObjectPath, OwnedVariant, Variant};

const ITEM_PATH: &str = "/org/ayatana/NotificationItem/farglass_e2e";
const MENU_PATH: &str = "/MenuBar";

/// The id of the menu entry Quit.
const QUIT: i32 = 7;

struct Item;

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn id(&self) -> &'static str {
        "farglass-e2e"
    }

    #[zbus(property)]
    fn title(&self) -> &'static str {
        "E2E item"
    }

    #[zbus(property)]
    fn status(&self) -> &'static str {
        "Active"
    }

    /// One icon of 2 x 2 pixels: red, green, blue, and white, as alpha, red, green, blue.
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        let pixels = vec![
            255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255,
        ];
        vec![(2, 2, pixels)]
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("a valid object path")
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    fn activate(&self, x: i32, y: i32) {
        println!("activate {x} {y}");
    }
}

/// One entry `(ia{sv}av)` of the menu.
type Entry = (i32, HashMap<String, OwnedVariant>, Vec<OwnedVariant>);

fn entry(id: i32, properties: &[(&str, Variant<'static>)], children: Vec<Entry>) -> Entry {
    let properties = properties
        .iter()
        .filter_map(|(key, value)| {
            let value = value.try_clone().ok()?.try_into().ok()?;
            Some(((*key).to_owned(), value))
        })
        .collect();
    let children = children
        .into_iter()
        .filter_map(|child| Variant::from(child).try_into().ok())
        .collect();
    (id, properties, children)
}

struct Menu;

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn get_layout(&self, _parent: i32, _depth: i32, _properties: Vec<String>) -> (u32, Entry) {
        let check = [
            ("label", "Sync".into()),
            ("toggle-type", "checkmark".into()),
            ("toggle-state", 1_i32.into()),
        ];
        let hidden = [("label", "Hidden".into()), ("visible", false.into())];
        let more = [
            ("label", "_More".into()),
            ("children-display", "submenu".into()),
        ];
        let root = entry(
            0,
            &[("children-display", "submenu".into())],
            vec![
                entry(1, &[("label", "_Show window".into())], Vec::new()),
                entry(2, &check, Vec::new()),
                entry(3, &[("type", "separator".into())], Vec::new()),
                entry(4, &hidden, Vec::new()),
                entry(
                    5,
                    &more,
                    vec![
                        entry(
                            6,
                            &[("label", "Lock".into()), ("enabled", false.into())],
                            Vec::new(),
                        ),
                        entry(QUIT, &[("label", "_Quit".into())], Vec::new()),
                    ],
                ),
            ],
        );
        (1, root)
    }

    fn event(&self, id: i32, event_id: &str, _data: OwnedVariant, _timestamp: u32) {
        println!("event {id} {event_id}");
        if id == QUIT && event_id == "clicked" {
            std::process::exit(0);
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> zbus::Result<()> {
    let Some(watcher) = std::env::args().nth(1) else {
        eprintln!("usage: tray-item WATCHER");
        std::process::exit(2);
    };
    let connection = zbus::Connection::session().await?;
    connection.object_server().at(ITEM_PATH, Item).await?;
    connection.object_server().at(MENU_PATH, Menu).await?;
    connection
        .call_method(
            Some(watcher.as_str()),
            "/StatusNotifierWatcher",
            Some("org.kde.StatusNotifierWatcher"),
            "RegisterStatusNotifierItem",
            &(ITEM_PATH,),
        )
        .await?;
    println!("registered");
    std::future::pending::<()>().await;
    Ok(())
}
