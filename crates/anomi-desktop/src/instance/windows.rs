//! A named auto-reset event for the folder: a later start sets it, and the
//! first Anomi's thread, waiting on it, shows the window. A second,
//! unnamed event stops the thread before a restart starts the next process.

use std::path::Path;

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    System::Threading::{
        CreateEventW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForMultipleObjects,
    },
    UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow},
};

/// The event's name: `Local\yapfast-` and an FNV-1a hash of the folder's
/// absolute path, as Windows compares paths, without case.
fn name(root: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let root = std::path::absolute(root).unwrap_or_else(|_| root.to_owned());
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for unit in root.as_os_str().encode_wide() {
        let unit = char::from_u32(unit.into()).map_or(unit, |c| c.to_ascii_lowercase() as u16);
        for byte in unit.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("Local\\yapfast-{hash:016x}")
        .encode_utf16()
        .chain([0])
        .collect()
}

pub(super) fn ask(root: &Path, launch: &super::Launch) -> std::io::Result<()> {
    // SAFETY: the name is NUL-terminated; the handle is closed once set.
    unsafe {
        let event = OpenEventW(EVENT_MODIFY_STATE, 0, name(root).as_ptr());
        if event.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        if let Some(link) = launch.link.as_deref()
            && let Err(error) = super::queue_link(root, link)
        {
            CloseHandle(event);
            return Err(error);
        }
        // The started process may bring a window forward; it passes that on.
        AllowSetForegroundWindow(ASFW_ANY);
        let set = SetEvent(event);
        CloseHandle(event);
        if set == 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// The two events, as addresses a thread can take.
pub(super) struct Listener {
    event: usize,
    stop: usize,
    root: std::path::PathBuf,
    listening: bool,
}

impl Listener {
    pub(super) fn bind(root: &Path) -> Option<Self> {
        super::clear_link_queue(root);
        // SAFETY: the name is NUL-terminated; null attributes are the defaults.
        let (event, stop) = unsafe {
            (
                CreateEventW(std::ptr::null(), 0, 0, name(root).as_ptr()),
                CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()),
            )
        };
        if event.is_null() || stop.is_null() {
            tracing::warn!(target: crate::LOG,
                error = %std::io::Error::last_os_error(),
                "starting Anomi again can't show this window"
            );
            // SAFETY: each is a handle this made, or null.
            unsafe {
                if !event.is_null() {
                    CloseHandle(event);
                }
                if !stop.is_null() {
                    CloseHandle(stop);
                }
            }
            return None;
        }
        Some(Self {
            event: event as usize,
            stop: stop as usize,
            root: root.to_owned(),
            listening: false,
        })
    }

    pub(super) fn listen(&mut self, opened: impl Fn(super::Launch) + Send + 'static) {
        let (event, stop) = (self.event, self.stop);
        let root = self.root.clone();
        let thread = std::thread::Builder::new()
            .name("anomi-instance".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                let handles = [event as HANDLE, stop as HANDLE];
                // SAFETY: both handles stay open until this thread closes them.
                while unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) }
                    == WAIT_OBJECT_0
                {
                    let links = super::drain_link_queue(&root);
                    if links.is_empty() {
                        opened(super::Launch::default());
                    } else {
                        for link in links {
                            opened(super::Launch {
                                token: None,
                                link: Some(link),
                            });
                        }
                    }
                }
                // SAFETY: Drop only sets `stop` once this thread runs.
                unsafe {
                    CloseHandle(handles[0]);
                    CloseHandle(handles[1]);
                }
            });
        match thread {
            Ok(_) => self.listening = true,
            Err(error) => {
                tracing::warn!(target: crate::LOG, %error, "starting Anomi again can't show this window");
            }
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // SAFETY: the handles are open: the thread closes them only after `stop`.
        unsafe {
            if self.listening {
                SetEvent(self.stop as HANDLE);
            } else {
                CloseHandle(self.event as HANDLE);
                CloseHandle(self.stop as HANDLE);
            }
        }
    }
}
