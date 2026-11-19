//! Where the window's frame is on the desktop, and putting the next window
//! there, on Windows, which tells where the monitors are.

use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MONITORINFO,
        MonitorFromRect, MonitorFromWindow,
    },
    UI::WindowsAndMessaging::{
        GetWindowRect, IsIconic, IsZoomed, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
    },
};

/// How much of the frame's top must show on a monitor to keep the window
/// where it was: about the title bar, by which it's moved.
const TITLE_BAR: i32 = 32;

/// What a monitor says of itself: its whole rectangle, and the work area
/// the taskbar leaves.
fn monitor_info(monitor: HMONITOR) -> Option<MONITORINFO> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a MONITORINFO with its size set, as the call asks.
    (unsafe { GetMonitorInfoW(monitor, &mut info) } != 0).then_some(info)
}

/// The window's frame, in the desktop's pixels.
fn frame(window: HWND) -> Option<RECT> {
    let mut frame = RECT::default();
    // SAFETY: only writes the rectangle passed.
    (unsafe { GetWindowRect(window, &mut frame) } != 0).then_some(frame)
}

/// The frame's top left corner while the window is neither maximized,
/// minimized, when Windows puts it far off screen, nor full screen, when
/// it covers its monitor.
pub fn read(window: HWND) -> Option<[i32; 2]> {
    // SAFETY: these only read the window's state; the handle is the
    // window's, alive while the window's thread runs this.
    if unsafe { IsIconic(window) != 0 || IsZoomed(window) != 0 } {
        return None;
    }
    let frame = frame(window)?;
    // SAFETY: as above.
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    let covers = monitor_info(monitor).is_some_and(|info| {
        let whole = info.rcMonitor;
        (frame.left, frame.top, frame.right, frame.bottom)
            == (whole.left, whole.top, whole.right, whole.bottom)
    });
    (!covers).then_some([frame.left, frame.top])
}

/// Puts a window not yet shown at `at`, unless its title bar would show on
/// no monitor there, as after one was unplugged: it stays where Windows
/// made it then. Either way it's shrunk to the work area of the monitor
/// it's on if it no longer fits there.
pub fn put(window: HWND, at: [i32; 2]) {
    let Some(made) = frame(window) else {
        return;
    };
    let (width, height) = (made.right - made.left, made.bottom - made.top);
    let rect = |height: i32| RECT {
        left: at[0],
        top: at[1],
        right: at[0].saturating_add(width),
        bottom: at[1].saturating_add(height),
    };
    // SAFETY: these only read the rectangle passed and the window's place.
    let (monitor, [x, y]) = unsafe {
        if MonitorFromRect(&rect(TITLE_BAR), MONITOR_DEFAULTTONULL).is_null() {
            (
                MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
                [made.left, made.top],
            )
        } else {
            (MonitorFromRect(&rect(height), MONITOR_DEFAULTTONEAREST), at)
        }
    };
    let (width, height) = monitor_info(monitor).map_or((width, height), |info| {
        let work = info.rcWork;
        (
            width.min(work.right - work.left),
            height.min(work.bottom - work.top),
        )
    });
    // SAFETY: moves and sizes the window whose handle this is, from its
    // own thread.
    unsafe {
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}
