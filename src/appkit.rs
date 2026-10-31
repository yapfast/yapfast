//! The Mac ways of doing the things every desktop has its own of: the menu
//! bar, an icon in the menu bar's right side, notifications, starting at
//! sign-in, and what a copy from Finder leaves on the clipboard. The other
//! half of each is in `native/appkit.m`, which is compiled by `build.rs`.
//!
//! Each callback the system calls comes back to a function set once, so the
//! Objective-C side needs no state of its own beyond what it draws.

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{io, ptr};

use yapfast_core::chat::Picture;

use crate::tray::Choice;

unsafe extern "C" {
    fn yap_app_install(chosen: extern "C" fn(i32));

    fn yap_tray_start(chosen: extern "C" fn(i32), pixels: *const u8, side: u32);
    fn yap_tray_show(pixels: *const u8, side: u32, in_call: c_int);
    fn yap_tray_available() -> c_int;
    fn yap_tray_stop();

    fn yap_notifications_start(opened: extern "C" fn(*const c_char));
    fn yap_notify_show(
        identifier: *const c_char,
        title: *const c_char,
        body: *const c_char,
        picture: *const u8,
        width: u32,
        height: u32,
    );

    fn yap_login_item_supported() -> c_int;
    fn yap_login_item_enabled() -> c_int;
    fn yap_login_item_set(wanted: c_int, err: *mut c_char, length: usize) -> c_int;

    fn yap_pasteboard_files(out: *mut c_char, length: usize) -> i32;
}

/// What the menu bar and the tray were asked for, and what a clicked
/// notification belongs to.
static MENU: OnceLock<fn(Choice)> = OnceLock::new();
static TRAY: OnceLock<fn(Choice)> = OnceLock::new();
static OPENED: OnceLock<fn(String)> = OnceLock::new();

/// The five the menu bar, the tray and the Dock icon offer, by the numbers
/// `appkit.m` uses.
fn choice(number: i32) -> Option<Choice> {
    match number {
        0 => Some(Choice::Open),
        1 => Some(Choice::Quit),
        2 => Some(Choice::Disconnect),
        3 => Some(Choice::Settings),
        4 => Some(Choice::Reopen),
        _ => None,
    }
}

extern "C" fn menu_chosen(number: i32) {
    // The menu lives on AppKit's thread, which is the one Slint's event loop
    // runs on, so this answers directly.
    if let (Some(chosen), Some(choice)) = (MENU.get(), choice(number)) {
        chosen(choice);
    }
}

extern "C" fn tray_chosen(number: i32) {
    if let (Some(chosen), Some(choice)) = (TRAY.get(), choice(number)) {
        chosen(choice);
    }
}

/// A notification was clicked. macOS delivers that on a queue of its own, so
/// it goes to Slint's event loop from here.
extern "C" fn notification_opened(room: *const c_char) {
    if room.is_null() {
        return;
    }
    let room = unsafe { std::ffi::CStr::from_ptr(room) }
        .to_string_lossy()
        .into_owned();
    let Some(opened) = OPENED.get().copied() else {
        return;
    };
    let _ = slint::invoke_from_event_loop(move || opened(room));
}

/// Puts up Yapfast's menu bar and makes it a normal application with a Dock
/// icon. `chosen` takes what the menu asks for, on Slint's event loop.
pub fn install(chosen: fn(Choice)) {
    let _ = MENU.set(chosen);
    unsafe { yap_app_install(menu_chosen) };
}

/// Puts Yapfast's icon in the menu bar's right side. `pixels` is the logo as
/// RGBA, `side` by `side`, drawn at half that so it stays crisp.
pub fn tray_start(chosen: fn(Choice), pixels: &[u8], side: u32) {
    let _ = TRAY.set(chosen);
    unsafe { yap_tray_start(tray_chosen, pixels.as_ptr(), side) };
}

pub fn tray_show(pixels: &[u8], side: u32, in_call: bool) {
    unsafe { yap_tray_show(pixels.as_ptr(), side, c_int::from(in_call)) };
}

pub fn tray_available() -> bool {
    unsafe { yap_tray_available() != 0 }
}

pub fn tray_stop() {
    unsafe { yap_tray_stop() };
}

/// Asks to show notifications, and remembers where a clicked one goes. Does
/// nothing when Yapfast isn't running from a bundle, since macOS won't let a
/// bare executable use the notification service.
pub fn notifications_start(opened: fn(String)) {
    let _ = OPENED.set(opened);
    unsafe { yap_notifications_start(notification_opened) };
}

/// Shows one notification, with the author's picture when there is one.
pub fn notify(identifier: &str, title: &str, body: &str, picture: Option<&Picture>) {
    let (pixels, width, height) = picture.map_or((ptr::null(), 0, 0), |picture| {
        (
            picture.rgba.as_ptr().cast::<u8>(),
            picture.width,
            picture.height,
        )
    });
    let (identifier, title, body) = (text(identifier), text(title), text(body));
    unsafe {
        yap_notify_show(
            identifier.as_ptr(),
            title.as_ptr(),
            body.as_ptr(),
            pixels,
            width,
            height,
        )
    };
}

/// Text for C, which has no use for a zero in the middle of it.
fn text(value: &str) -> CString {
    CString::new(value.replace('\0', " ")).unwrap_or_default()
}

/// Whether this macOS can register a login item at all: 13 and later can.
pub fn login_item_supported() -> bool {
    unsafe { yap_login_item_supported() != 0 }
}

/// Whether Yapfast starts when you sign in.
pub fn login_item_enabled() -> bool {
    unsafe { yap_login_item_enabled() != 0 }
}

/// Starts Yapfast at sign-in, or stops it. macOS asks the person to agree in
/// System Settings, and says no if they don't.
pub fn login_item_set(wanted: bool) -> io::Result<()> {
    let mut reason = [0u8; 256];
    let done = unsafe {
        yap_login_item_set(
            c_int::from(wanted),
            reason.as_mut_ptr().cast(),
            reason.len(),
        )
    };
    if done != 0 {
        return Ok(());
    }
    let end = reason
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(reason.len());
    Err(io::Error::other(
        String::from_utf8_lossy(&reason[..end]).into_owned(),
    ))
}

/// The files copied in Finder, which arboard can't read on macOS.
pub fn pasteboard_files() -> Vec<PathBuf> {
    let mut size = 16 * 1024;
    for _ in 0..4 {
        let mut buffer = vec![0u8; size];
        let count = unsafe { yap_pasteboard_files(buffer.as_mut_ptr().cast(), buffer.len()) };
        if count >= 0 {
            // Each path is its own NUL-terminated string, with an empty one
            // at the end.
            return String::from_utf8_lossy(&buffer)
                .split('\0')
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .collect();
        }
        size *= 4;
    }
    Vec::new()
}
