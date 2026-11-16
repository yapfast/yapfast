//! Where the window was and how big, kept in the layout, so Yapfast starts as
//! it was left and the window made again from the tray comes back as it was.

use super::*;

/// The window's size and place while not maximized, and whether it was.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct Placement {
    /// Inside the frame, in physical pixels.
    width: u32,
    height: u32,
    /// The frame's top left corner on the desktop. Wayland doesn't say, and
    /// on X11 a monitor gone since could leave it off screen, so only Windows,
    /// which tells where the monitors are, keeps it from one start to the next.
    #[cfg_attr(not(windows), serde(skip))]
    position: Option<[i32; 2]>,
    maximized: bool,
}

/// How long the window holds still before where it is gets written.
const SETTLE: Duration = Duration::from_secs(1);

/// Gives a window not yet shown the size and place, unless they no longer
/// fit the desktop's monitors. Where the monitors can't be asked before the
/// window exists, as on Linux, its size is brought to them once it does.
pub(super) fn place(window: &YapfastWindow, placement: Option<&Placement>) {
    let native = window.window();
    if let Some(placement) = placement {
        let (size, position) = platform::fit(
            native,
            slint::PhysicalSize::new(placement.width, placement.height),
            placement.position,
        );
        native.set_size(size);
        if let Some([x, y]) = position {
            native.set_position(slint::PhysicalPosition::new(x, y));
        }
        // Maximized over the size and place above, which un-maximizing returns to.
        native.set_maximized(placement.maximized);
    }
    platform::fit_once_shown(window.as_weak());
}

/// The window moved, resized, maximized or came back: keeps where it is now,
/// written once it holds still.
pub(super) fn moved(adapter: &mut Adapter, window: &YapfastWindow) {
    let native = window.window();
    // Minimized, Windows puts the window far off screen.
    if native.is_minimized() {
        return;
    }
    let size = native.size();
    let position = native
        .with_winit_window(|winit| winit.outer_position().ok())
        .flatten()
        .map(|position| [position.x, position.y]);
    let maximized = native.is_maximized();
    let now = match adapter.layout.window {
        // Maximized, the window keeps the size and place it goes back to.
        Some(before) if maximized => Placement {
            maximized,
            ..before
        },
        _ => Placement {
            width: size.width,
            height: size.height,
            position,
            maximized,
        },
    };
    if adapter.layout.window == Some(now) {
        return;
    }
    adapter.layout.window = Some(now);
    let adapter_weak = adapter.weak.clone();
    adapter
        .placement_due
        .start(slint::TimerMode::SingleShot, SETTLE, move || {
            if let Some(adapter) = adapter_weak.upgrade() {
                let adapter = adapter.borrow();
                adapter.layout.write(&adapter.root);
            }
        });
}

/// Writes where the window is if it moved since the last write, as Yapfast ends.
pub(super) fn flush(adapter: &Adapter) {
    if adapter.placement_due.running() {
        adapter.placement_due.stop();
        adapter.layout.write(&adapter.root);
    }
}

#[cfg(windows)]
mod platform {
    /// `fit` has already answered for Windows.
    pub(super) fn fit_once_shown(_window: slint::Weak<super::YapfastWindow>) {}

    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL,
            MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, MonitorFromRect,
        },
        UI::WindowsAndMessaging::{AdjustWindowRectEx, WS_OVERLAPPEDWINDOW},
    };

    /// How much of the frame's top must show on a monitor to keep the window
    /// where it was: about the title bar, by which it's moved.
    const TITLE_BAR: u32 = 32;

    /// Forgets the place if the title bar would show on no monitor, as after
    /// one was unplugged, and shrinks the window to the work area of the
    /// monitor it goes on if it no longer fits there.
    pub(super) fn fit(
        _window: &slint::Window,
        size: slint::PhysicalSize,
        position: Option<[i32; 2]>,
    ) -> (slint::PhysicalSize, Option<[i32; 2]>) {
        let rect = |[x, y]: [i32; 2], height: u32| RECT {
            left: x,
            top: y,
            right: x.saturating_add_unsigned(size.width),
            bottom: y.saturating_add_unsigned(height),
        };
        let position = position.filter(|&at| {
            // SAFETY: only reads the rectangle passed.
            !unsafe { MonitorFromRect(&rect(at, TITLE_BAR), MONITOR_DEFAULTTONULL) }.is_null()
        });
        // SAFETY: as above. Without a place, Windows puts the window on the
        // primary monitor.
        let monitor = unsafe {
            match position {
                Some(at) => MonitorFromRect(&rect(at, size.height), MONITOR_DEFAULTTONEAREST),
                None => MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
            }
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `info` is a MONITORINFO with its size set, as the call asks.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return (size, position);
        }
        // The size is inside the frame, so the frame comes off the work area.
        let mut frame = RECT::default();
        // SAFETY: only changes the rectangle passed.
        unsafe { AdjustWindowRectEx(&mut frame, WS_OVERLAPPEDWINDOW, 0, 0) };
        let work = info.rcWork;
        let room = |work: i32, frame: i32| (work - frame).max(0) as u32;
        let size = slint::PhysicalSize::new(
            size.width
                .min(room(work.right - work.left, frame.right - frame.left)),
            size.height
                .min(room(work.bottom - work.top, frame.bottom - frame.top)),
        );
        (size, position)
    }
}

#[cfg(not(windows))]
mod platform {
    use slint::{
        ComponentHandle as _,
        winit_030::{
            WinitWindowAccessor,
            winit::{
                raw_window_handle::{HasWindowHandle, RawWindowHandle},
                window::Window,
            },
        },
    };

    /// The desktop puts the window itself, and on Wayland nothing here may
    /// move it; its size is fitted by `fit_once_shown`.
    pub(super) fn fit(
        _window: &slint::Window,
        size: slint::PhysicalSize,
        position: Option<[i32; 2]>,
    ) -> (slint::PhysicalSize, Option<[i32; 2]>) {
        (size, position)
    }

    /// Slint makes the window, and so its monitor, only once the event loop
    /// runs: brings the size to what the monitor has room for then, after a
    /// few tries if it isn't there yet. A maximized window is the desktop's.
    pub(super) fn fit_once_shown(window: slint::Weak<super::YapfastWindow>) {
        const TRIES: u32 = 25;
        const EVERY: std::time::Duration = std::time::Duration::from_millis(40);
        fn attempt(window: slint::Weak<super::YapfastWindow>, left: u32) {
            slint::Timer::single_shot(EVERY, move || {
                let Some(shown) = window.upgrade() else {
                    return;
                };
                let native = shown.window();
                let size = native.size();
                match native.with_winit_window(|winit| (room(winit), size)) {
                    // Not made yet, or not mapped, with no size.
                    None | Some((_, slint::PhysicalSize { width: 0, .. })) if left > 0 => {
                        attempt(window, left - 1);
                    }
                    Some((Some(area), size)) if !native.is_maximized() => {
                        let fitted = shrunk(size, area);
                        if fitted != size {
                            native.set_size(fitted);
                        }
                    }
                    _ => {}
                }
            });
        }
        attempt(window, TRIES);
    }

    /// The width and height the monitor the window goes on has for windows:
    /// on X11 its part of the work area, which leaves out panels; on Wayland
    /// the whole monitor, as a client isn't told where the panels are.
    fn room(window: &Window) -> Option<(u32, u32)> {
        let monitor = window
            .current_monitor()
            .or_else(|| window.primary_monitor())
            .or_else(|| window.available_monitors().next())?;
        let position = monitor.position();
        let size = monitor.size();
        let monitor = [
            i64::from(position.x),
            i64::from(position.y),
            i64::from(size.width),
            i64::from(size.height),
        ];
        let x11 = matches!(
            window.window_handle().ok()?.as_raw(),
            RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)
        );
        let area = if x11 {
            work_area().map_or(monitor, |work| overlap(monitor, work))
        } else {
            monitor
        };
        (area[2] > 0 && area[3] > 0).then_some((area[2] as u32, area[3] as u32))
    }

    /// The part of `a` inside `b`, each as x, y, width, height.
    pub(super) fn overlap(a: [i64; 4], b: [i64; 4]) -> [i64; 4] {
        let left = a[0].max(b[0]);
        let top = a[1].max(b[1]);
        let right = (a[0] + a[2]).min(b[0] + b[2]);
        let bottom = (a[1] + a[3]).min(b[1] + b[3]);
        [left, top, (right - left).max(0), (bottom - top).max(0)]
    }

    /// The X11 work area of the current desktop, `_NET_WORKAREA`, across all
    /// the monitors, as x, y, width, height.
    fn work_area() -> Option<[i64; 4]> {
        use x11rb::{
            connection::Connection,
            protocol::xproto::{AtomEnum, ConnectionExt},
        };
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let atom = |name: &[u8]| connection.intern_atom(false, name).ok()?.reply().ok();
        let areas = atom(b"_NET_WORKAREA")?.atom;
        let current = atom(b"_NET_CURRENT_DESKTOP")?.atom;
        let numbers = |property, length| {
            connection
                .get_property(false, root, property, AtomEnum::CARDINAL, 0, length)
                .ok()?
                .reply()
                .ok()
                .map(|reply| reply.value32().map(Iterator::collect).unwrap_or_default())
        };
        let areas: Vec<u32> = numbers(areas, 4 * 64)?;
        let desktop = numbers(current, 1)
            .and_then(|current: Vec<u32>| current.first().copied())
            .unwrap_or(0) as usize;
        let area = areas.chunks_exact(4).nth(desktop).or(areas.get(..4))?;
        // Signed on the wire: a strut can't push the origin below 0, but be sure.
        Some([
            i64::from(area[0] as i32),
            i64::from(area[1] as i32),
            i64::from(area[2]),
            i64::from(area[3]),
        ])
    }

    /// A size the area has no room for goes to nine tenths of it, as far as it
    /// is too big: the window's frame and the grip to resize it must show.
    pub(super) fn shrunk(
        size: slint::PhysicalSize,
        (width, height): (u32, u32),
    ) -> slint::PhysicalSize {
        let each = |size: u32, room: u32| if size > room { room * 9 / 10 } else { size };
        slint::PhysicalSize::new(each(size.width, width), each(size.height, height))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_window_too_big_for_the_work_area_shrinks_to_nine_tenths() {
            let size = slint::PhysicalSize::new(1307, 887);
            // A 1366x768 laptop with a panel 40px high.
            assert_eq!(
                shrunk(size, (1366, 728)),
                slint::PhysicalSize::new(1307, 655)
            );
            assert_eq!(
                shrunk(size, (1280, 720)),
                slint::PhysicalSize::new(1152, 648)
            );
        }

        #[test]
        fn a_window_that_fits_keeps_its_size() {
            let size = slint::PhysicalSize::new(1307, 887);
            assert_eq!(shrunk(size, (3440, 1400)), size);
            assert_eq!(shrunk(size, (1307, 887)), size);
        }

        #[test]
        fn the_work_area_of_the_screen_is_cut_to_the_monitor() {
            // Two monitors side by side, the panel along the bottom of the
            // second: the first has no panel.
            let work = [0, 0, 3286, 1032];
            assert_eq!(overlap([0, 0, 1920, 1080], work), [0, 0, 1920, 1032]);
            assert_eq!(overlap([1920, 0, 1366, 768], work), [1920, 0, 1366, 768]);
            assert_eq!(overlap([5000, 0, 100, 100], work), [5000, 0, 0, 100]);
        }
    }
}
