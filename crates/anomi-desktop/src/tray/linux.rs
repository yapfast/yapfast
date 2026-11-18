use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use ksni::{MenuItem, menu::StandardItem};

use super::{Choice, Chosen, Shown};

/// The tray's sizes of the icon; the tray scales the nearest.
const SIZES: [u32; 2] = [32, 64];

pub struct Tray {
    updates: mpsc::Sender<Shown>,
    /// Registered with a running StatusNotifierWatcher.
    online: Arc<AtomicBool>,
}

impl Tray {
    pub fn start(chosen: Chosen) -> Option<Self> {
        let icons = |unread| {
            SIZES
                .iter()
                .filter_map(|&size| super::logo(size, unread))
                .map(|pixels| ksni::Icon {
                    width: pixels.width() as i32,
                    height: pixels.height() as i32,
                    // ARGB, in network byte order.
                    data: pixels
                        .pixels()
                        .flat_map(|pixel| {
                            let [r, g, b, a] = pixel.0;
                            [a, r, g, b]
                        })
                        .collect(),
                })
                .collect()
        };
        let online = Arc::new(AtomicBool::new(false));
        let item = Item {
            shown: Shown::default(),
            icons: [icons(false), icons(true)],
            chosen,
            online: online.clone(),
        };
        let (updates, shown) = mpsc::channel();
        // Registering waits on the bus, and so does each update.
        let started = std::thread::Builder::new()
            .name("anomi-tray".into())
            .spawn(move || run(item, shown));
        if let Err(error) = started {
            tracing::warn!(target: crate::LOG, %error, "could not start the tray icon");
            return None;
        }
        Some(Self { updates, online })
    }

    pub fn available(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    pub fn show(&self, shown: Shown) {
        let _ = self.updates.send(shown);
    }
}

fn run(item: Item, shown: mpsc::Receiver<Shown>) {
    use ksni::blocking::TrayMethods;
    let online = item.online.clone();
    let chosen = item.chosen.clone();
    // Until registering says otherwise; it reports a missing watcher.
    online.store(true, Ordering::Relaxed);
    // A tray that starts after Anomi, or comes back, still gets the icon.
    let handle = match item.assume_sni_available(true).spawn() {
        Ok(handle) => handle,
        Err(error) => {
            online.store(false, Ordering::Relaxed);
            tracing::info!(target: crate::LOG, %error, "no tray icon; closing the window quits Anomi");
            return;
        }
    };
    if online.load(Ordering::Relaxed) {
        tracing::info!(target: crate::LOG, "tray icon shown");
        choose(&chosen, Choice::Available);
    }
    for shown in shown {
        handle.update(|item| item.shown = shown);
    }
    handle.shutdown().wait();
}

struct Item {
    shown: Shown,
    /// The icon without the unread dot, and with it.
    icons: [Vec<ksni::Icon>; 2],
    chosen: Chosen,
    online: Arc<AtomicBool>,
}

fn choose(chosen: &Chosen, choice: Choice) {
    // KDE Plasma sends a click on the icon or a menu item its
    // xdg-activation token just before the click, and ksni hands it to
    // the click. Most other trays, such as Waybar, send none.
    if choice == Choice::Open {
        *super::TOKEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            ksni::current_xdg_activation_token();
    }
    chosen(choice);
}

impl ksni::Tray for Item {
    fn id(&self) -> String {
        "yapfast".into()
    }

    fn title(&self) -> String {
        yapfast_lang::PRODUCT.into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Communications
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons[usize::from(self.shown.unread)].clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: yapfast_lang::PRODUCT.into(),
            description: if self.shown.unread {
                yapfast_lang::tr!("Unread messages").into()
            } else {
                String::new()
            },
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        choose(&self.chosen, Choice::Open);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let item = |label: &str, choice| {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |item: &mut Self| choose(&item.chosen, choice)),
                ..Default::default()
            }
            .into()
        };
        let mut menu = vec![item(yapfast_lang::tr!("Open Yapfast"), Choice::Open)];
        if self.shown.call {
            menu.push(item(
                yapfast_lang::text_in("Tray", "Disconnect"),
                Choice::Disconnect,
            ));
        }
        menu.push(MenuItem::Separator);
        menu.push(item(yapfast_lang::tr!("Quit"), Choice::Quit));
        menu
    }

    fn watcher_online(&self) {
        tracing::info!(target: crate::LOG, "tray icon shown");
        self.online.store(true, Ordering::Relaxed);
        choose(&self.chosen, Choice::Available);
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        tracing::info!(target: crate::LOG, ?reason, "no tray to show the icon");
        self.online.store(false, Ordering::Relaxed);
        choose(&self.chosen, Choice::Available);
        // Wait for it to come back.
        true
    }
}
