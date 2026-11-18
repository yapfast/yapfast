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
                SM_CXSMICON, SetForegroundWindow, SetMenuDefaultItem, TPM_NONOTIFY, TPM_RETURNCMD,
                TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL,
                WM_RBUTTONUP, WM_USER, WNDCLASSW, WS_OVERLAPPED,
            },
        },
    },
    w,
};

use super::{Choice, Chosen, Shown};

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
    chosen: Chosen,
}

pub struct Tray {
    window: HWND,
}

impl Tray {
    pub fn start(chosen: Chosen) -> Option<Self> {
        // SAFETY: plain Win32 calls on the UI thread; every pointer passed
        // lives across the call, and the window and icons are released in
        // `drop`.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: w!("AnomiTray"),
                ..std::mem::zeroed()
            };
            RegisterClassW(&class);
            // A hidden top-level window rather than a message-only one,
            // which wouldn't hear that Explorer restarted.
            let window = CreateWindowExW(
                0,
                w!("AnomiTray"),
                w!("Anomi"),
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
                tracing::warn!(target: crate::LOG, "could not make the tray icon's window");
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
            let tip = yapfast_lang::PRODUCT.encode_utf16();
            for (place, unit) in data.szTip.iter_mut().zip(tip) {
                *place = unit;
            }
            let added = notify(NIM_ADD, &data);
            if !added {
                tracing::info!(target: crate::LOG, "no notification area; closing the window quits Anomi");
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
        .pixels()
        .flat_map(|pixel| {
            let [r, g, b, a] = pixel.0;
            [b, g, r, a]
        })
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
    // The toolkit's own `Chosen` takes it out of the window procedure, which
    // the menu's own loop runs.
    if let Some(chosen) = ICON.with_borrow(|icon| icon.as_ref().map(|icon| icon.chosen.clone())) {
        chosen(choice);
    }
}

/// A menu item's text, ended by the null that Windows wants.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The icon's menu, at the pointer.
fn menu(window: HWND) {
    let call = ICON.with_borrow(|icon| icon.as_ref().is_some_and(|icon| icon.shown.call));
    // SAFETY: the menu is made, shown and destroyed here, for a window of
    // this thread.
    let picked = unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(
            menu,
            MF_STRING,
            OPEN,
            wide(yapfast_lang::tr!("Open Yapfast")).as_ptr(),
        );
        if call {
            AppendMenuW(
                menu,
                MF_STRING,
                DISCONNECT,
                wide(yapfast_lang::text_in("Tray", "Disconnect")).as_ptr(),
            );
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(
            menu,
            MF_STRING,
            QUIT,
            wide(yapfast_lang::tr!("Quit")).as_ptr(),
        );
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
