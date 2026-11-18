//! Files dragged onto the window from a file manager. winit reports them on
//! X11 and Windows; on Wayland, where winit has no drag and drop, a data
//! device of the app's own on winit's connection does. Both clients' windows
//! are winit's, which hand over their handles through raw-window-handle.

use std::path::PathBuf;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// A drag of files over the window.
#[derive(Clone, Debug)]
pub enum Drag {
    /// Files came over the window.
    Entered,
    /// The pointer dragging them is here, in the window's logical pixels as
    /// the platform has them, before Yapfast's zoom.
    Moved { x: f64, y: f64 },
    /// They left it, or the drag ended elsewhere.
    Left,
    /// They were let go over it.
    Dropped(Vec<PathBuf>),
}

/// Where the pointer is over a window, for the drags winit reports without
/// it: on X11 and Windows. One is made for each drag and asked while the
/// files hover.
pub struct Pointer {
    source: Source,
    /// The window's scale, from its physical pixels to its logical ones.
    scale: f64,
}

enum Source {
    /// A connection of its own, and the window.
    #[cfg(target_os = "linux")]
    X11(x11rb::rust_connection::RustConnection, u32),
    #[cfg(windows)]
    Win32(isize),
}

impl Pointer {
    /// The pointer over `window`, whose scale from physical pixels to logical
    /// ones is `scale`.
    pub fn of<W: HasWindowHandle + ?Sized>(window: &W, scale: f64) -> Option<Self> {
        let handle = window.window_handle().ok()?.as_raw();
        let source = match handle {
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xlib(handle) => Self::x11(handle.window as u32)?,
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xcb(handle) => Self::x11(handle.window.get())?,
            #[cfg(windows)]
            RawWindowHandle::Win32(handle) => Source::Win32(handle.hwnd.get()),
            _ => return None,
        };
        Some(Self { source, scale })
    }

    #[cfg(target_os = "linux")]
    fn x11(window: u32) -> Option<Source> {
        match x11rb::connect(None) {
            Ok((connection, _)) => Some(Source::X11(connection, window)),
            Err(error) => {
                tracing::warn!(target: crate::LOG, %error, "could not follow files dragged over the window");
                None
            }
        }
    }

    /// Where it is now, unless the platform won't say.
    pub fn at(&self) -> Option<Drag> {
        let (x, y) = match &self.source {
            #[cfg(target_os = "linux")]
            Source::X11(connection, window) => {
                use x11rb::protocol::xproto::ConnectionExt;
                let pointer = connection.query_pointer(*window).ok()?.reply().ok()?;
                (f64::from(pointer.win_x), f64::from(pointer.win_y))
            }
            #[cfg(windows)]
            Source::Win32(hwnd) => {
                use windows_sys::Win32::Foundation::POINT;
                use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
                use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
                let mut point = POINT { x: 0, y: 0 };
                // SAFETY: the point outlives both calls, and a window gone
                // meanwhile makes ScreenToClient fail rather than misbehave.
                if unsafe { GetCursorPos(&mut point) } == 0
                    || unsafe { ScreenToClient(*hwnd as _, &mut point) } == 0
                {
                    return None;
                }
                (f64::from(point.x), f64::from(point.y))
            }
        };
        Some(Drag::Moved {
            x: x / self.scale,
            y: y / self.scale,
        })
    }
}

#[cfg(target_os = "linux")]
pub mod wayland;
