//! Yapfast's icon in the system tray, which keeps Yapfast running with its window
//! closed: a StatusNotifierItem on Linux, on the session bus Yapfast is given,
//! and a notification-area icon on Windows. Clicking it opens the window; its
//! menu opens Yapfast, leaves a call, checks for updates, restarts and quits.

use std::cell::Cell;

/// The xdg-activation token of the tray click or Open Yapfast that is
/// opening the window, which lets a Wayland window come to the front.
#[cfg(target_os = "linux")]
static TOKEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The token of the tray click that asked for the window, once.
pub fn take_activation_token() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        TOKEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// What the tray asks for, or says, handed to the [`Chosen`] the tray was
/// started with, from the tray's own thread or window procedure: each window
/// toolkit takes it onto its event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Open,
    Disconnect,
    CheckForUpdates,
    Restart,
    Quit,
    /// A tray came or went, which decides what closing the window does.
    Available,
}

/// What the icon and its menu show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// A mention or a DM waits unread: the icon has a dot.
    pub unread: bool,
    /// In a call: the menu offers Disconnect.
    pub call: bool,
}

/// Where the tray's choices go. It runs off the window's event loop, so it
/// only hands the choice on.
pub type Chosen = std::sync::Arc<dyn Fn(Choice) + Send + Sync>;

pub struct Tray {
    platform: platform::Tray,
    shown: Cell<Shown>,
}

impl Tray {
    /// Puts Yapfast's icon in the tray. `chosen` is handed what the person
    /// picked. `None` where there's no tray to use. On Windows it starts on
    /// the window's thread, whose message loop runs the icon's window.
    pub fn start(chosen: Chosen) -> Option<Self> {
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

enum Badge {
    None,
    Dot,
    #[cfg(windows)]
    Count(u8),
}

impl From<bool> for Badge {
    fn from(unread: bool) -> Self {
        if unread { Self::Dot } else { Self::None }
    }
}

/// Yapfast's marigold logo with the notification badge cut into its corner.
fn logo(size: u32, badge: Badge) -> Option<crate::icon::Rgba> {
    let mut svg = include_str!("../crates/yapfast-iced-kit/icons/logo.svg")
        .replacen(
            r#"width="24" height="24""#,
            &format!(r#"width="{size}" height="{size}""#),
            1,
        )
        .replacen(r##"fill="#fff" mask"##, r##"fill="#ffbf47" mask"##, 1);
    svg = match badge {
        Badge::None => svg,
        Badge::Dot => svg
            .replacen("</mask>", r#"<circle cx="74" cy="74" r="21" /></mask>"#, 1)
            .replacen(
                "</svg>",
                r##"<circle cx="74" cy="74" r="15" fill="#f0545e" /></svg>"##,
                1,
            ),
        #[cfg(windows)]
        Badge::Count(count) => {
            // Stroked digits stay readable at small sizes and need no font.
            const DIGITS: [&str; 9] = [
                "M2 4L7 0V16M3 16H11",
                "M1 4Q1 0 6 0Q11 0 11 5Q11 8 1 16H11",
                "M1 2Q3 0 6 0Q11 0 11 4Q11 8 6 8Q11 8 11 12Q11 16 6 16Q2 16 1 14",
                "M8 0L1 11H11M8 0V16",
                "M11 0H2L1 8Q11 5 11 12Q11 16 6 16Q2 16 1 14",
                "M10 1Q1 -2 1 10Q1 16 6 16Q11 16 11 11Q11 7 6 7Q3 7 1 10",
                "M1 0H11L4 16",
                "M6 8C-1 8 -1 0 6 0S13 8 6 8C-1 8 -1 16 6 16S13 8 6 8Z",
                "M2 15Q11 19 11 6Q11 0 6 0Q1 0 1 5Q1 10 6 10Q9 10 11 7",
            ];
            let digit = DIGITS[usize::from(count.min(9) - 1)];
            let (left, x, plus) = if count > 9 {
                (47, 52.0, "M16 8H27M21.5 2.5V13.5")
            } else {
                (57, 66.5, "")
            };
            let badge = format!(
                r#"<rect x="{left}" y="12" width="{}" height="31" rx="15.5""#,
                90 - left
            );
            svg.replacen("</mask>", &format!(r##"{badge} stroke="#000" stroke-width="6"/></mask>"##), 1)
                .replacen("</svg>", &format!(r##"{badge} fill="#d12c43"/><path transform="translate({x} 18) scale(1.18)" d="{digit}{plus}" fill="none" stroke="#fff" stroke-width="3.6" stroke-linecap="round" stroke-linejoin="round"/></svg>"##), 1)
        }
    };
    crate::icon::rasterize(svg.as_bytes(), size)
}

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(target_os = "linux", windows)))]
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
        pub fn start(chosen: super::Chosen) -> Option<Self> {
            let icons = |unread: bool| {
                SIZES
                    .iter()
                    .filter_map(|&size| super::logo(size, unread.into()))
                    .map(|pixels| ksni::Icon {
                        width: pixels.width as i32,
                        height: pixels.height as i32,
                        // ARGB, in network byte order.
                        data: pixels
                            .pixels
                            .chunks_exact(4)
                            .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
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
        let chosen = item.chosen.clone();
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
        chosen: super::Chosen,
        online: Arc<AtomicBool>,
    }

    fn choose(chosen: &super::Chosen, choice: Choice) {
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
            let mut menu = vec![item("Open Yapfast", Choice::Open)];
            if self.shown.call {
                menu.push(item("Disconnect", Choice::Disconnect));
            }
            menu.push(MenuItem::Separator);
            menu.push(item("Check for updates", Choice::CheckForUpdates));
            menu.push(item("Restart Yapfast", Choice::Restart));
            menu.push(MenuItem::Separator);
            menu.push(item("Quit", Choice::Quit));
            menu
        }

        fn watcher_online(&self) {
            tracing::info!("tray icon shown");
            self.online.store(true, Ordering::Relaxed);
            choose(&self.chosen, Choice::Available);
        }

        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            tracing::info!(?reason, "no tray to show the icon");
            self.online.store(false, Ordering::Relaxed);
            choose(&self.chosen, Choice::Available);
            // Wait for it to come back.
            true
        }
    }
}

#[cfg(windows)]
pub(crate) use windows::taskbar_icon;

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
                    MENUINFO, MF_SEPARATOR, MF_STRING, MIM_STYLE, MNS_NOCHECK, PostMessageW,
                    RegisterClassW, RegisterWindowMessageW, SM_CXSMICON, SetForegroundWindow,
                    SetMenuDefaultItem, SetMenuInfo, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
                    TrackPopupMenu, WM_APP, WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
                    WM_USER, WNDCLASSW, WS_OVERLAPPED,
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
    const CHECK_FOR_UPDATES: usize = 3;
    const RESTART: usize = 4;
    const QUIT: usize = 5;

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
        chosen: super::Chosen,
    }

    pub struct Tray {
        window: HWND,
    }

    impl Tray {
        pub fn start(chosen: super::Chosen) -> Option<Self> {
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
                    w!("Open Yapfast"),
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
                let icons = [
                    icon(size, super::Badge::None),
                    icon(size, super::Badge::Dot),
                ];
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

    pub(crate) fn taskbar_icon(size: u32, count: u8) -> HICON {
        icon(size, super::Badge::Count(count))
    }

    /// The logo as an icon with an alpha channel: colour as BGRA, and an empty
    /// mask, which the alpha replaces.
    fn icon(size: u32, badge: super::Badge) -> HICON {
        let Some(mut pixels) = super::logo(size, badge) else {
            return std::ptr::null_mut();
        };
        for pixel in pixels.pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let (width, height) = (pixels.width, pixels.height);
        // Rows of the 1-bit mask are padded to 16 bits.
        let mask = vec![0u8; width.div_ceil(16) as usize * 2 * height as usize];
        // SAFETY: both buffers hold `height` rows of `width` pixels.
        unsafe {
            CreateIcon(
                GetModuleHandleW(std::ptr::null()),
                width as i32,
                height as i32,
                1,
                32,
                mask.as_ptr(),
                pixels.pixels.as_ptr(),
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
        // Not while `ICON` is borrowed: the choice may come back to it.
        if let Some(chosen) = ICON.with_borrow(|icon| icon.as_ref().map(|icon| icon.chosen.clone()))
        {
            chosen(choice);
        }
    }

    /// The icon's menu, at the pointer.
    fn menu(window: HWND) {
        let call = ICON.with_borrow(|icon| icon.as_ref().is_some_and(|icon| icon.shown.call));
        // SAFETY: the menu is made, shown and destroyed here, for a window of
        // this thread.
        let picked = unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            SetMenuInfo(
                menu,
                &MENUINFO {
                    cbSize: size_of::<MENUINFO>() as u32,
                    fMask: MIM_STYLE,
                    dwStyle: MNS_NOCHECK,
                    ..std::mem::zeroed()
                },
            );
            AppendMenuW(menu, MF_STRING, OPEN, w!("Open Yapfast"));
            if call {
                AppendMenuW(menu, MF_STRING, DISCONNECT, w!("Disconnect"));
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, CHECK_FOR_UPDATES, w!("Check for updates"));
            AppendMenuW(menu, MF_STRING, RESTART, w!("Restart Yapfast"));
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
            CHECK_FOR_UPDATES => choose(Choice::CheckForUpdates),
            RESTART => choose(Choice::Restart),
            QUIT => choose(Choice::Quit),
            _ => {}
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod none {
    use super::{Choice, Shown};

    pub struct Tray;

    impl Tray {
        pub fn start(_: super::Chosen) -> Option<Self> {
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
        let plain = super::logo(32, super::Badge::None).unwrap();
        let unread = super::logo(32, super::Badge::Dot).unwrap();
        assert_eq!((plain.width, plain.height), (32, 32));
        let marigold = |pixel: &[u8]| pixel == [0xff, 0xbf, 0x47, 255];
        let red = |pixel: &[u8]| pixel == [0xf0, 0x54, 0x5e, 255];
        assert!(plain.pixels.chunks_exact(4).any(marigold));
        assert!(!plain.pixels.chunks_exact(4).any(red));
        assert!(unread.pixels.chunks_exact(4).any(marigold));
        // The dot is in the bottom-right corner.
        let corner = (24 * 32 + 24) * 4;
        assert!(red(&unread.pixels[corner..corner + 4]));
    }
}
