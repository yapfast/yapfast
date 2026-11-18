//! Files from `CF_HDROP`; then text, which the composer pastes, since
//! office suites put a bitmap of the cells or text they copy beside it;
//! then a picture as it was copied, under the names apps register for
//! each type (Snipping Tool and browsers register "PNG"); then a bitmap,
//! which nearly every app copying a picture puts.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW, OpenClipboard,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::UI::Shell::DragQueryFileW;

use super::{Clipboard, PICTURES, bitmap, png};

// The standard formats Yapfast takes, from WinUser.h.
const CF_TEXT: u32 = 1;
const CF_OEMTEXT: u32 = 7;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

pub fn read() -> Clipboard {
    let Some(open) = Open::new() else {
        tracing::debug!(target: crate::LOG, "could not open the clipboard");
        return Clipboard::Other;
    };
    let formats = open.formats();
    tracing::debug!(target: crate::LOG, ?formats, "the clipboard's formats");
    if formats.iter().any(|(format, _)| *format == CF_HDROP)
        && let Some(files) = open.files()
        && !files.is_empty()
    {
        return Clipboard::Files(files);
    }
    let text = [CF_TEXT, CF_OEMTEXT, CF_UNICODETEXT];
    if formats.iter().any(|(format, _)| text.contains(format)) {
        return Clipboard::Other;
    }
    for (kind, extension) in PICTURES {
        let expected = image::ImageFormat::from_mime_type(kind);
        for (format, name) in &formats {
            if !name.as_deref().is_some_and(|name| is_type(name, kind)) {
                continue;
            }
            let Some(bytes) = open.data(*format) else {
                continue;
            };
            // A name is only a convention between apps; the bytes say what they are.
            if image::guess_format(bytes.bytes()).ok() == expected {
                return Clipboard::Picture(bytes.bytes().to_vec(), extension);
            }
        }
    }
    // The one the copying app put, rather than the one Windows makes of it.
    let Some((format, _)) = formats
        .iter()
        .find(|(format, _)| matches!(*format, CF_DIB | CF_DIBV5))
    else {
        return Clipboard::Other;
    };
    let pixels = open.data(*format).and_then(|dib| bitmap(dib.bytes()));
    drop(open);
    match pixels {
        Some(pixels) => png(&pixels),
        None => {
            tracing::debug!(target: crate::LOG, format, "could not read the clipboard's bitmap");
            Clipboard::Other
        }
    }
}

/// Whether `name`, a registered clipboard format, is the picture type `kind`.
fn is_type(name: &str, kind: &str) -> bool {
    let legacy = match kind {
        "image/png" => "PNG",
        "image/gif" => "GIF",
        "image/jpeg" => "JFIF",
        _ => "",
    };
    name.eq_ignore_ascii_case(kind) || name.eq_ignore_ascii_case(legacy)
}

/// The clipboard, open for reading until dropped.
struct Open;

impl Open {
    fn new() -> Option<Self> {
        // Another app may hold it for a moment.
        for _ in 0..10 {
            // SAFETY: no window to own it is needed to read it.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Some(Self);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }

    /// Its formats, in the order the copying app put them, those Windows
    /// makes from them last; with the names of registered ones.
    fn formats(&self) -> Vec<(u32, Option<String>)> {
        let mut formats = Vec::new();
        let mut format = 0;
        loop {
            // SAFETY: the clipboard is open.
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                return formats;
            }
            let mut name = [0u16; 128];
            // SAFETY: the buffer's length is what's passed.
            let length =
                unsafe { GetClipboardFormatNameW(format, name.as_mut_ptr(), name.len() as i32) };
            let name = (length > 0).then(|| String::from_utf16_lossy(&name[..length as usize]));
            formats.push((format, name));
        }
    }

    fn files(&self) -> Option<Vec<PathBuf>> {
        // SAFETY: the clipboard is open, and CF_HDROP's handle is an HDROP.
        let list = unsafe { GetClipboardData(CF_HDROP) };
        if list.is_null() {
            return None;
        }
        // SAFETY: as above; u32::MAX asks for the count.
        let count = unsafe { DragQueryFileW(list, u32::MAX, std::ptr::null_mut(), 0) };
        let files = (0..count)
            .filter_map(|index| {
                // SAFETY: as above; a null buffer asks for the length.
                let length =
                    unsafe { DragQueryFileW(list, index, std::ptr::null_mut(), 0) } as usize;
                let mut path = vec![0u16; length + 1];
                // SAFETY: the buffer holds the path and its terminating null.
                let copied =
                    unsafe { DragQueryFileW(list, index, path.as_mut_ptr(), path.len() as u32) }
                        as usize;
                (copied > 0).then(|| PathBuf::from(OsString::from_wide(&path[..copied])))
            })
            .filter(|path| path.is_file())
            .collect();
        Some(files)
    }

    /// The format's data, read in place while the clipboard stays open.
    fn data(&self, format: u32) -> Option<Locked<'_>> {
        // SAFETY: the clipboard is open.
        let handle = unsafe { GetClipboardData(format) };
        if handle.is_null() {
            return None;
        }
        // SAFETY: the clipboard's data is global memory, which stays
        // while the clipboard is open and the lock is held.
        let start = unsafe { GlobalLock(handle) };
        if start.is_null() {
            return None;
        }
        // SAFETY: as above; the size may be rounded up past the data.
        let length = unsafe { GlobalSize(handle) };
        Some(Locked {
            handle,
            // SAFETY: the lock is held for as long as the slice lives.
            bytes: unsafe { std::slice::from_raw_parts(start.cast(), length) },
        })
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: Open is made only once the clipboard is open.
        unsafe { CloseClipboard() };
    }
}

struct Locked<'clipboard> {
    handle: HANDLE,
    bytes: &'clipboard [u8],
}

impl Locked<'_> {
    fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        // SAFETY: the handle was locked in Open::data.
        unsafe { GlobalUnlock(self.handle) };
    }
}
