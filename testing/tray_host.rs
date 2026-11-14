//! A stand-in for a desktop's tray, for `scripts/ui-session start --tray`: a
//! minimal StatusNotifierWatcher on the session's private bus, and commands
//! that act on the icon registered with it as a tray would.
//!
//!   tray_host watch           keep a StatusNotifierWatcher running
//!   tray_host activate        click the icon
//!   tray_host activate TOKEN [after]
//!                             click it with an xdg-activation token, sent
//!                             before the click as KDE Plasma does, or 5 ms
//!                             after
//!   tray_host menu            print the menu's visible items
//!   tray_host click LABEL     choose a menu item
//!   tray_host icon PATH       save the largest icon as a PAM picture
//!
//! It runs only on the isolated test display, whose bus is its own: on the
//! desktop's bus it would stand in for the real tray.

#[cfg(target_os = "linux")]
fn main() -> zbus::Result<()> {
    if std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() != Ok("1") {
        eprintln!("tray_host runs only inside scripts/run-framework-isolated");
        std::process::exit(2);
    }
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    match arguments.as_slice() {
        ["watch"] => linux::watch(),
        ["activate"] => linux::activate(None, false),
        ["activate", token] => linux::activate(Some(token), false),
        ["activate", token, "after"] => linux::activate(Some(token), true),
        ["menu"] => linux::menu(),
        ["click", label @ ..] => linux::click(&label.join(" ")),
        ["icon", path] => linux::icon(path),
        _ => {
            eprintln!(
                "usage: tray_host watch | activate [TOKEN [after]] | menu | click LABEL | icon PATH"
            );
            std::process::exit(2);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use zbus::{
        blocking::Connection,
        message::Header,
        zvariant::{OwnedObjectPath, OwnedValue, Value},
    };

    const WATCHER: &str = "org.kde.StatusNotifierWatcher";
    const WATCHER_PATH: &str = "/StatusNotifierWatcher";
    const ITEM: &str = "org.kde.StatusNotifierItem";
    const ITEM_PATH: &str = "/StatusNotifierItem";
    const MENU: &str = "com.canonical.dbusmenu";

    struct Watcher {
        items: Arc<Mutex<Vec<String>>>,
    }

    #[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
    impl Watcher {
        fn register_status_notifier_item(&self, service: &str, #[zbus(header)] header: Header<'_>) {
            // A path names an object of the caller, a bus name one at the usual path.
            let service = match header.sender() {
                Some(sender) if service.starts_with('/') => format!("{sender}{service}"),
                _ => service.to_owned(),
            };
            eprintln!("tray_host: registered {service}");
            self.items.lock().unwrap().push(service);
        }

        fn register_status_notifier_host(&self, _service: &str) {}

        #[zbus(property)]
        fn registered_status_notifier_items(&self) -> Vec<String> {
            self.items.lock().unwrap().clone()
        }

        #[zbus(property)]
        fn is_status_notifier_host_registered(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn protocol_version(&self) -> i32 {
            0
        }
    }

    pub fn watch() -> zbus::Result<()> {
        let items = Arc::new(Mutex::new(Vec::new()));
        let connection = zbus::blocking::connection::Builder::session()?
            .serve_at(
                WATCHER_PATH,
                Watcher {
                    items: items.clone(),
                },
            )?
            .name(WATCHER)?
            .build()?;
        // Forget items whose owner left the bus, as a tray would.
        let bus = zbus::blocking::fdo::DBusProxy::new(&connection)?;
        for change in bus.receive_name_owner_changed()? {
            let arguments = change.args()?;
            if arguments.new_owner.is_none() {
                let name = arguments.name.to_string();
                items
                    .lock()
                    .unwrap()
                    .retain(|item| item != &name && !item.starts_with(&format!("{name}/")));
            }
        }
        Ok(())
    }

    /// The bus name and path of the first icon registered.
    fn item(connection: &Connection) -> zbus::Result<(String, String)> {
        let items: Vec<String> = property(
            connection,
            WATCHER,
            WATCHER_PATH,
            WATCHER,
            "RegisteredStatusNotifierItems",
        )?
        .try_into()?;
        let Some(item) = items.first() else {
            eprintln!("no icon is registered");
            std::process::exit(1);
        };
        Ok(match item.split_once('/') {
            Some((name, path)) => (name.to_owned(), format!("/{path}")),
            None => (item.clone(), ITEM_PATH.to_owned()),
        })
    }

    fn property(
        connection: &Connection,
        destination: &str,
        path: &str,
        interface: &str,
        name: &str,
    ) -> zbus::Result<OwnedValue> {
        let reply = connection.call_method(
            Some(destination),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(interface, name),
        )?;
        reply.body().deserialize()
    }

    pub fn activate(token: Option<&str>, after: bool) -> zbus::Result<()> {
        let connection = Connection::session()?;
        let (name, path) = item(&connection)?;
        let provide = {
            let (connection, name, path) = (connection.clone(), name.clone(), path.clone());
            let token = token.map(str::to_owned);
            move || -> zbus::Result<()> {
                if let Some(token) = token {
                    connection.call_method(
                        Some(name.as_str()),
                        path.as_str(),
                        Some(ITEM),
                        "ProvideXdgActivationToken",
                        &(token,),
                    )?;
                }
                Ok(())
            }
        };
        let late = if after {
            Some(std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(5));
                provide()
            }))
        } else {
            provide()?;
            None
        };
        connection.call_method(
            Some(name.as_str()),
            path.as_str(),
            Some(ITEM),
            "Activate",
            &(0i32, 0i32),
        )?;
        if let Some(late) = late {
            late.join().expect("the token's thread")?;
        }
        Ok(())
    }

    /// The menu's items as dbusmenu lays them out: id, properties, children.
    type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

    /// The icon's bus name, its menu's path, and the menu's visible items by
    /// id and label; a separator's label is "-".
    type Menu = (String, OwnedObjectPath, Vec<(i32, String)>);

    fn items(connection: &Connection) -> zbus::Result<Menu> {
        let (name, path) = item(connection)?;
        let menu: OwnedObjectPath = property(connection, &name, &path, ITEM, "Menu")?.try_into()?;
        let reply = connection.call_method(
            Some(name.as_str()),
            menu.as_str(),
            Some(MENU),
            "GetLayout",
            &(0i32, -1i32, Vec::<&str>::new()),
        )?;
        let (_, (_, _, children)): (u32, Layout) = reply.body().deserialize()?;
        let mut shown = Vec::new();
        for child in children {
            let (id, properties, _): Layout = child.try_into()?;
            let text = |key: &str| {
                properties
                    .get(key)
                    .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
            };
            let visible = properties
                .get("visible")
                .is_none_or(|value| matches!(**value, Value::Bool(true)));
            if !visible {
                continue;
            }
            let label = match text("type").as_deref() {
                Some("separator") => "-".to_owned(),
                _ => text("label").unwrap_or_default(),
            };
            shown.push((id, label));
        }
        Ok((name, menu, shown))
    }

    pub fn menu() -> zbus::Result<()> {
        let connection = Connection::session()?;
        let (_, _, items) = items(&connection)?;
        let labels: Vec<String> = items.into_iter().map(|(_, label)| label).collect();
        println!("{}", labels.join(" | "));
        Ok(())
    }

    pub fn click(label: &str) -> zbus::Result<()> {
        let connection = Connection::session()?;
        let (name, menu, items) = items(&connection)?;
        let Some((id, _)) = items.iter().find(|(_, shown)| shown == label) else {
            eprintln!("the menu has no {label:?}");
            std::process::exit(1);
        };
        connection.call_method(
            Some(name.as_str()),
            menu.as_str(),
            Some(MENU),
            "Event",
            &(*id, "clicked", Value::from(0i32), 0u32),
        )?;
        Ok(())
    }

    pub fn icon(path: &str) -> zbus::Result<()> {
        let connection = Connection::session()?;
        let (name, item_path) = item(&connection)?;
        let icons: Vec<(i32, i32, Vec<u8>)> =
            property(&connection, &name, &item_path, ITEM, "IconPixmap")?.try_into()?;
        let Some((width, height, argb)) = icons.into_iter().max_by_key(|(width, ..)| *width) else {
            eprintln!("the icon has no pixmap");
            std::process::exit(1);
        };
        let mut picture = format!(
            "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
        )
        .into_bytes();
        for pixel in argb.chunks_exact(4) {
            picture.extend_from_slice(&[pixel[1], pixel[2], pixel[3], pixel[0]]);
        }
        std::fs::write(path, picture).map_err(|error| zbus::Error::Failure(error.to_string()))?;
        println!("{width}x{height}");
        Ok(())
    }
}
