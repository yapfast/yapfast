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
    zbus::block_on(async {
        match arguments.as_slice() {
            ["watch"] => linux::watch().await,
            ["activate"] => linux::activate(None, false).await,
            ["activate", token] => linux::activate(Some(token), false).await,
            ["activate", token, "after"] => linux::activate(Some(token), true).await,
            ["menu"] => linux::menu().await,
            ["click", label @ ..] => linux::click(&label.join(" ")).await,
            ["icon", path] => linux::icon(path).await,
            _ => {
                eprintln!(
                    "usage: tray_host watch | activate [TOKEN [after]] | menu | click LABEL | icon PATH"
                );
                std::process::exit(2);
            }
        }
    })
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use futures_util::StreamExt;
    use zbus::{Connection, OwnedObjectPath, OwnedVariant, Variant, message::Header};

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

    pub async fn watch() -> zbus::Result<()> {
        let items = Arc::new(Mutex::new(Vec::new()));
        let connection = zbus::connection::Builder::session()
            .serve_at(
                WATCHER_PATH,
                Watcher {
                    items: items.clone(),
                },
            )
            .name(WATCHER)
            .build()
            .await?;
        // Forget items whose owner left the bus, as a tray would.
        let bus = zbus::fdo::DBusProxy::new(&connection).await?;
        let mut changes = bus.receive_name_owner_changed().await?;
        while let Some(change) = changes.next().await {
            let arguments = change.args()?;
            if arguments.new_owner.is_none() {
                let name = arguments.name.to_string();
                let path_prefix = format!("{name}/");
                items
                    .lock()
                    .unwrap()
                    .retain(|item| item != &name && !item.starts_with(&path_prefix));
            }
        }
        Ok(())
    }

    /// The bus name and path of the first icon registered.
    async fn item(connection: &Connection) -> zbus::Result<(String, String)> {
        let items: Vec<String> = property(
            connection,
            WATCHER,
            WATCHER_PATH,
            WATCHER,
            "RegisteredStatusNotifierItems",
        )
        .await?
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

    async fn property(
        connection: &Connection,
        destination: &str,
        path: &str,
        interface: &str,
        name: &str,
    ) -> zbus::Result<OwnedVariant> {
        let reply = connection
            .call_method(
                Some(destination),
                path,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(interface, name),
            )
            .await?;
        reply.body().deserialize()
    }

    async fn provide_token(
        connection: &Connection,
        name: &str,
        path: &str,
        token: &str,
    ) -> zbus::Result<()> {
        connection
            .call_method(
                Some(name),
                path,
                Some(ITEM),
                "ProvideXdgActivationToken",
                &(token,),
            )
            .await?;
        Ok(())
    }

    pub async fn activate(token: Option<&str>, after: bool) -> zbus::Result<()> {
        let connection = Connection::session().await?;
        let (name, path) = item(&connection).await?;
        let late = if let Some(token) = token.filter(|_| after) {
            let (connection, name, path, token) = (
                connection.clone(),
                name.clone(),
                path.clone(),
                token.to_owned(),
            );
            Some(std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(5));
                zbus::block_on(provide_token(&connection, &name, &path, &token))
            }))
        } else {
            if let Some(token) = token {
                provide_token(&connection, &name, &path, token).await?;
            }
            None
        };
        connection
            .call_method(
                Some(name.as_str()),
                path.as_str(),
                Some(ITEM),
                "Activate",
                &(0i32, 0i32),
            )
            .await?;
        if let Some(late) = late {
            late.join().expect("the token's thread")?;
        }
        Ok(())
    }

    /// The menu's items as dbusmenu lays them out: id, properties, children.
    type Layout = (i32, HashMap<String, OwnedVariant>, Vec<OwnedVariant>);

    /// The icon's bus name, its menu's path, and the menu's visible items by
    /// id and label; a separator's label is "-".
    type Menu = (String, OwnedObjectPath, Vec<(i32, String)>);

    async fn items(connection: &Connection) -> zbus::Result<Menu> {
        let (name, path) = item(connection).await?;
        let menu: OwnedObjectPath = property(connection, &name, &path, ITEM, "Menu")
            .await?
            .try_into()?;
        let reply = connection
            .call_method(
                Some(name.as_str()),
                menu.as_str(),
                Some(MENU),
                "GetLayout",
                &(0i32, -1i32, Vec::<&str>::new()),
            )
            .await?;
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
                .is_none_or(|value| matches!(**value, Variant::Bool(true)));
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

    pub async fn menu() -> zbus::Result<()> {
        let connection = Connection::session().await?;
        let (_, _, items) = items(&connection).await?;
        let labels: Vec<String> = items.into_iter().map(|(_, label)| label).collect();
        println!("{}", labels.join(" | "));
        Ok(())
    }

    pub async fn click(label: &str) -> zbus::Result<()> {
        let connection = Connection::session().await?;
        let (name, menu, items) = items(&connection).await?;
        let Some((id, _)) = items.iter().find(|(_, shown)| shown == label) else {
            eprintln!("the menu has no {label:?}");
            std::process::exit(1);
        };
        connection
            .call_method(
                Some(name.as_str()),
                menu.as_str(),
                Some(MENU),
                "Event",
                &(*id, "clicked", Variant::from(0i32), 0u32),
            )
            .await?;
        Ok(())
    }

    pub async fn icon(path: &str) -> zbus::Result<()> {
        let connection = Connection::session().await?;
        let (name, item_path) = item(&connection).await?;
        let icons: Vec<(i32, i32, Vec<u8>)> =
            property(&connection, &name, &item_path, ITEM, "IconPixmap")
                .await?
                .try_into()?;
        let Some((width, height, argb)) = icons.into_iter().max_by_key(|(width, ..)| *width) else {
            eprintln!("the icon has no pixmap");
            std::process::exit(1);
        };
        let mut picture = format!(
            "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
        )
        .into_bytes();
        let (pixels, _) = argb.as_chunks::<4>();
        for &[alpha, red, green, blue] in pixels {
            picture.extend_from_slice(&[red, green, blue, alpha]);
        }
        std::fs::write(path, picture).map_err(|error| zbus::Error::Failure(error.to_string()))?;
        println!("{width}x{height}");
        Ok(())
    }
}
