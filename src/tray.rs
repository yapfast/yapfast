//! Yapfast's icon in the system tray, which keeps Yapfast running with its window
//! closed: a StatusNotifierItem on Linux, on the session bus Yapfast is given,
//! and a notification-area icon on Windows. Clicking it opens the window; its
//! menu opens Yapfast, leaves a call and quits.

use std::cell::Cell;

/// What the tray, or the Mac's menu bar, asks for, or says. Each runs on
/// Slint's event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Open,
    Disconnect,
    Quit,
    /// A tray came or went, which decides what closing the window does.
    Available,
    /// Settings… from the Mac's menu bar, which no tray offers.
    Settings,
    /// The Dock icon was clicked with no window up, which only the Mac has.
    Reopen,
}

/// What the icon and its menu show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// A mention or a DM waits unread: the icon has a dot.
    pub unread: bool,
    /// In a call: the menu offers Disconnect.
    pub call: bool,
}

pub struct Tray {
    platform: platform::Tray,
    shown: Cell<Shown>,
}

impl Tray {
    /// Puts Yapfast's icon in the tray. `chosen` runs on Slint's event loop with
    /// what the person picked. `None` where there's no tray to use.
    pub fn start(chosen: fn(Choice)) -> Option<Self> {
        let platform = platform::Tray::start(chosen)?;
        Some(Self {
            platform,
            shown: Cell::new(Shown::default()),
        })
    }

    /// Whether a tray shows the icon now, so the window can close into it.
    pub fn available(&self) -> bool {
        self.platform.available()
    }

    /// Shows the unread dot and the call's Disconnect, telling the tray only
    /// when they change.
    pub fn show(&self, shown: Shown) {
        if self.shown.replace(shown) != shown {
            self.platform.show(shown);
        }
    }
}

/// Yapfast's logo in Yap's marigold, `size` pixels square, with a red dot for
/// unread messages cut into its corner.
fn logo(size: u32, unread: bool) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let mut svg = include_str!("../ui/icons/logo.svg")
        .replacen(
            r#"width="24" height="24""#,
            &format!(r#"width="{size}" height="{size}""#),
            1,
        )
        .replacen(r##"fill="#fff" mask"##, r##"fill="#ffbf47" mask"##, 1);
    if unread {
        svg = svg
            .replacen("</mask>", r#"<circle cx="74" cy="74" r="21" /></mask>"#, 1)
            .replacen(
                "</svg>",
                r##"<circle cx="74" cy="74" r="15" fill="#f0545e" /></svg>"##,
                1,
            );
    }
    slint::Image::load_from_svg_data(svg.as_bytes())
        .ok()?
        .to_rgba8()
}

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
use none as platform;
#[cfg(windows)]
use windows as platform;

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };

    use ksni::{MenuItem, menu::StandardItem};

    use super::{Choice, Shown};

    /// The tray's sizes of the icon; the tray scales the nearest.
    const SIZES: [u32; 2] = [32, 64];

    pub struct Tray {
        updates: mpsc::Sender<Shown>,
        /// Registered with a running StatusNotifierWatcher.
        online: Arc<AtomicBool>,
    }

    impl Tray {
        pub fn start(chosen: fn(Choice)) -> Option<Self> {
            let icons = |unread| {
                SIZES
                    .iter()
                    .filter_map(|&size| super::logo(size, unread))
                    .map(|pixels| ksni::Icon {
                        width: pixels.width() as i32,
                        height: pixels.height() as i32,
                        // ARGB, in network byte order.
                        data: pixels
                            .as_slice()
                            .iter()
                            .flat_map(|pixel| [pixel.a, pixel.r, pixel.g, pixel.b])
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
                .name("yapfast-tray".into())
                .spawn(move || run(item, shown));
            if let Err(error) = started {
                tracing::warn!(%error, "could not start the tray icon");
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
        let chosen = item.chosen;
        // Until registering says otherwise; it reports a missing watcher.
        online.store(true, Ordering::Relaxed);
        // A tray that starts after Yapfast, or comes back, still gets the icon.
        let handle = match item.assume_sni_available(true).spawn() {
            Ok(handle) => handle,
            Err(error) => {
                online.store(false, Ordering::Relaxed);
                tracing::info!(%error, "no tray icon; closing the window quits Yapfast");
                return;
            }
        };
        if online.load(Ordering::Relaxed) {
            tracing::info!("tray icon shown");
            choose(chosen, Choice::Available);
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
        chosen: fn(Choice),
        online: Arc<AtomicBool>,
    }

    fn choose(chosen: fn(Choice), choice: Choice) {
        let _ = slint::invoke_from_event_loop(move || chosen(choice));
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            "yapfast".into()
        }

        fn title(&self) -> String {
            "Yapfast".into()
        }

        fn category(&self) -> ksni::Category {
            ksni::Category::Communications
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icons[usize::from(self.shown.unread)].clone()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "Yapfast".into(),
                description: if self.shown.unread {
                    "Unread messages".into()
                } else {
                    String::new()
                },
                ..Default::default()
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            choose(self.chosen, Choice::Open);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            let item = |label: &str, choice| {
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |item: &mut Self| choose(item.chosen, choice)),
                    ..Default::default()
                }
                .into()
            };
            let mut menu = vec![item("Open Yapfast", Choice::Open)];
            if self.shown.call {
                menu.push(item("Disconnect", Choice::Disconnect));
            }
            menu.push(MenuItem::Separator);
            menu.push(item("Quit", Choice::Quit));
            menu
        }

        fn watcher_online(&self) {
            tracing::info!("tray icon shown");
            self.online.store(true, Ordering::Relaxed);
            choose(self.chosen, Choice::Available);
        }

        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            tracing::info!(?reason, "no tray to show the icon");
            self.online.store(false, Ordering::Relaxed);
            choose(self.chosen, Choice::Available);
            // Wait for it to come back.
            true
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::cell::{Cell, RefCell};

    use windows_sys::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                Shell::{
                    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
                    NOTIFY_ICON_MESSAGE, NOTIFYICONDATAW, Shell_NotifyIconW,
                },
                WindowsAndMessaging::{
                    AppendMenuW, CreateIcon, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
                    DestroyIcon, DestroyMenu, DestroyWindow, GetCursorPos, GetSystemMetrics, HICON,
                    MF_SEPARATOR, MF_STRING, PostMessageW, RegisterClassW, RegisterWindowMessageW,
                    SM_CXSMICON, SetForegroundWindow, SetMenuDefaultItem, TPM_NONOTIFY,
                    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CONTEXTMENU,
                    WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_USER, WNDCLASSW, WS_OVERLAPPED,
                },
            },
        },
        w,
    };

    use super::{Choice, Shown};

    /// The message the icon sends the hidden window with what happened to it.
    const ICON_MESSAGE: u32 = WM_APP + 1;
    /// Keyboard selection, which the shell sends as `NIN_SELECT` and `NIN_KEYSELECT`.
    const NIN_SELECT: u32 = WM_USER;
    const NIN_KEYSELECT: u32 = WM_USER + 1;
    const OPEN: usize = 1;
    const DISCONNECT: usize = 2;
    const QUIT: usize = 3;

    thread_local! {
        /// What the hidden window's procedure needs: the tray runs on the UI
        /// thread, whose message loop winit runs.
        static ICON: RefCell<Option<Icon>> = const { RefCell::new(None) };
        static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    }

    struct Icon {
        data: NOTIFYICONDATAW,
        /// The notification area shows it.
        added: bool,
        /// Without the unread dot, and with it.
        icons: [HICON; 2],
        shown: Shown,
        chosen: fn(Choice),
    }

    pub struct Tray {
        window: HWND,
    }

    impl Tray {
        pub fn start(chosen: fn(Choice)) -> Option<Self> {
            // SAFETY: plain Win32 calls on the UI thread; every pointer passed
            // lives across the call, and the window and icons are released in
            // `drop`.
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let class = WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance,
                    lpszClassName: w!("YapfastTray"),
                    ..std::mem::zeroed()
                };
                RegisterClassW(&class);
                // A hidden top-level window rather than a message-only one,
                // which wouldn't hear that Explorer restarted.
                let window = CreateWindowExW(
                    0,
                    w!("YapfastTray"),
                    w!("Yapfast"),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                );
                if window.is_null() {
                    tracing::warn!("could not make the tray icon's window");
                    return None;
                }
                TASKBAR_CREATED.set(RegisterWindowMessageW(w!("TaskbarCreated")));
                let size = GetSystemMetrics(SM_CXSMICON).clamp(16, 256) as u32;
                let icons = [icon(size, false), icon(size, true)];
                let mut data = NOTIFYICONDATAW {
                    cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                    hWnd: window,
                    uID: 1,
                    uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                    uCallbackMessage: ICON_MESSAGE,
                    hIcon: icons[0],
                    ..std::mem::zeroed()
                };
                for (place, unit) in data.szTip.iter_mut().zip("Yapfast".encode_utf16()) {
                    *place = unit;
                }
                let added = notify(NIM_ADD, &data);
                if !added {
                    tracing::info!("no notification area; closing the window quits Yapfast");
                }
                ICON.set(Some(Icon {
                    data,
                    added,
                    icons,
                    shown: Shown::default(),
                    chosen,
                }));
                Some(Self { window })
            }
        }

        pub fn available(&self) -> bool {
            ICON.with_borrow(|icon| icon.as_ref().is_some_and(|icon| icon.added))
        }

        pub fn show(&self, shown: Shown) {
            let data = ICON.with_borrow_mut(|icon| {
                let icon = icon.as_mut()?;
                icon.shown = shown;
                icon.data.hIcon = icon.icons[usize::from(shown.unread)];
                Some(icon.data)
            });
            if let Some(data) = data {
                notify(NIM_MODIFY, &data);
            }
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            if let Some(icon) = ICON.take() {
                notify(NIM_DELETE, &icon.data);
                for picture in icon.icons {
                    // SAFETY: the pictures are this tray's, and no longer shown.
                    unsafe { DestroyIcon(picture) };
                }
            }
            // SAFETY: the window is this tray's, on this thread.
            unsafe { DestroyWindow(self.window) };
        }
    }

    /// Tells the notification area about the icon. Not while `ICON` is
    /// borrowed: waiting on Explorer, the thread can run the window procedure.
    fn notify(message: NOTIFY_ICON_MESSAGE, data: &NOTIFYICONDATAW) -> bool {
        // SAFETY: the data names this tray's live window and icon.
        unsafe { Shell_NotifyIconW(message, data) != 0 }
    }

    /// The logo as an icon with an alpha channel: colour as BGRA, and an empty
    /// mask, which the alpha replaces.
    fn icon(size: u32, unread: bool) -> HICON {
        let Some(pixels) = super::logo(size, unread) else {
            return std::ptr::null_mut();
        };
        let colour: Vec<u8> = pixels
            .as_slice()
            .iter()
            .flat_map(|pixel| [pixel.b, pixel.g, pixel.r, pixel.a])
            .collect();
        // Rows of the 1-bit mask are padded to 16 bits.
        let mask = vec![0u8; size.div_ceil(16) as usize * 2 * size as usize];
        // SAFETY: both buffers hold `size` rows of `size` pixels.
        unsafe {
            CreateIcon(
                GetModuleHandleW(std::ptr::null()),
                size as i32,
                size as i32,
                1,
                32,
                mask.as_ptr(),
                colour.as_ptr(),
            )
        }
    }

    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == ICON_MESSAGE {
            match lparam as u32 {
                WM_LBUTTONUP | NIN_SELECT | NIN_KEYSELECT => choose(Choice::Open),
                WM_RBUTTONUP | WM_CONTEXTMENU => menu(window),
                _ => {}
            }
            return 0;
        }
        // Explorer started again, with an empty notification area.
        if message == TASKBAR_CREATED.get() && message != 0 {
            if let Some(data) = ICON.with_borrow(|icon| icon.as_ref().map(|icon| icon.data)) {
                let added = notify(NIM_ADD, &data);
                ICON.with_borrow_mut(|icon| {
                    if let Some(icon) = icon {
                        icon.added = added;
                    }
                });
            }
            choose(Choice::Available);
            return 0;
        }
        // SAFETY: the default for every other message.
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    fn choose(choice: Choice) {
        if let Some(chosen) = ICON.with_borrow(|icon| icon.as_ref().map(|icon| icon.chosen)) {
            // Not from inside the window procedure, which the menu's own loop runs.
            let _ = slint::invoke_from_event_loop(move || chosen(choice));
        }
    }

    /// The icon's menu, at the pointer.
    fn menu(window: HWND) {
        let call = ICON.with_borrow(|icon| icon.as_ref().is_some_and(|icon| icon.shown.call));
        // SAFETY: the menu is made, shown and destroyed here, for a window of
        // this thread.
        let picked = unsafe {
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, OPEN, w!("Open Yapfast"));
            if call {
                AppendMenuW(menu, MF_STRING, DISCONNECT, w!("Disconnect"));
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, QUIT, w!("Quit"));
            SetMenuDefaultItem(menu, OPEN as u32, 0);
            let mut at = POINT { x: 0, y: 0 };
            GetCursorPos(&mut at);
            // Without this the menu stays open when clicking elsewhere.
            SetForegroundWindow(window);
            let picked = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                at.x,
                at.y,
                0,
                window,
                std::ptr::null(),
            );
            PostMessageW(window, WM_NULL, 0, 0);
            DestroyMenu(menu);
            picked as usize
        };
        match picked {
            OPEN => choose(Choice::Open),
            DISCONNECT => choose(Choice::Disconnect),
            QUIT => choose(Choice::Quit),
            _ => {}
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    //! An icon in the menu bar's right side, as `native/appkit.m` draws it.
    //! Left-clicking it opens the window; its menu, from a right-click, holds
    //! the same three things the other desktops' hold.

    use super::{Choice, Shown};

    /// The logo's side in pixels. The bar draws it at half, so a Retina bar
    /// gets one pixel per dot.
    const SIDE: u32 = 36;

    pub struct Tray;

    /// The logo as the bytes AppKit wants: RGBA, row by row.
    fn pixels(unread: bool) -> Option<Vec<u8>> {
        Some(
            super::logo(SIDE, unread)?
                .as_slice()
                .iter()
                .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
                .collect(),
        )
    }

    impl Tray {
        pub fn start(chosen: fn(Choice)) -> Option<Self> {
            let pixels = pixels(false)?;
            crate::appkit::tray_start(chosen, &pixels, SIDE);
            tracing::info!("tray icon shown");
            // The menu bar's right side is always there, so closing the window
            // can always mean the tray.
            let _ = slint::invoke_from_event_loop(move || chosen(Choice::Available));
            Some(Self)
        }

        pub fn available(&self) -> bool {
            crate::appkit::tray_available()
        }

        pub fn show(&self, shown: Shown) {
            let Some(pixels) = pixels(shown.unread) else {
                return;
            };
            crate::appkit::tray_show(&pixels, SIDE, shown.call);
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            crate::appkit::tray_stop();
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod none {
    use super::{Choice, Shown};

    pub struct Tray;

    impl Tray {
        pub fn start(_: fn(Choice)) -> Option<Self> {
            None
        }

        pub fn available(&self) -> bool {
            false
        }

        pub fn show(&self, _: Shown) {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_logo_is_marigold_and_unread_adds_a_red_dot() {
        let plain = super::logo(32, false).unwrap();
        let unread = super::logo(32, true).unwrap();
        assert_eq!((plain.width(), plain.height()), (32, 32));
        let marigold = |pixel: &slint::Rgba8Pixel| {
            pixel.a == 255 && (pixel.r, pixel.g, pixel.b) == (0xff, 0xbf, 0x47)
        };
        let red = |pixel: &slint::Rgba8Pixel| {
            pixel.a == 255 && (pixel.r, pixel.g, pixel.b) == (0xf0, 0x54, 0x5e)
        };
        assert!(plain.as_slice().iter().any(marigold));
        assert!(!plain.as_slice().iter().any(red));
        assert!(unread.as_slice().iter().any(marigold));
        // The dot is in the bottom-right corner.
        let corner = (24 * 32 + 24) as usize;
        assert!(red(&unread.as_slice()[corner]));
    }
}
