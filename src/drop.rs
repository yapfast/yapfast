//! Files dragged onto the window from a file manager. winit reports them on
//! X11 and Windows; on Wayland, where winit has no drag and drop, a data
//! device of Yapfast's own on winit's connection does.

use std::path::PathBuf;

/// A drag of files over the window.
pub enum Drag {
    /// Files came over the window.
    Entered,
    /// The pointer dragging them is here, in the window's logical pixels as
    /// the platform has them, before Yapfast's zoom. Only a platform with a
    /// `Pointer` to ask has one; the others' tests still move it around.
    #[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
    Moved { x: f64, y: f64 },
    /// They left it, or the drag ended elsewhere.
    Left,
    /// They were let go over it.
    Dropped(Vec<PathBuf>),
}

/// Where the pointer is over a window, for the drags winit reports without
/// it: on X11 and Windows. One is made for each drag and asked while the
/// files hover.
#[cfg(any(target_os = "linux", windows))]
pub struct Pointer {
    source: Source,
    /// The window's scale, from its physical pixels to its logical ones.
    scale: f64,
}

#[cfg(any(target_os = "linux", windows))]
enum Source {
    /// A connection of its own, and the window.
    #[cfg(target_os = "linux")]
    X11(x11rb::rust_connection::RustConnection, u32),
    #[cfg(windows)]
    Win32(isize),
}

#[cfg(any(target_os = "linux", windows))]
impl Pointer {
    pub fn of(window: &slint::winit_030::winit::window::Window) -> Option<Self> {
        use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
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
        Some(Self {
            source,
            scale: window.scale_factor(),
        })
    }

    #[cfg(target_os = "linux")]
    fn x11(window: u32) -> Option<Source> {
        match x11rb::connect(None) {
            Ok((connection, _)) => Some(Source::X11(connection, window)),
            Err(error) => {
                tracing::warn!(%error, "could not follow files dragged over the window");
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

/// The Mac's winit says where a drag is with the drag itself, so there's
/// nothing to ask and nothing to ask it with. One is still made, so the code
/// that watches a hover doesn't branch on the platform, and it knows no more.
#[cfg(not(any(target_os = "linux", windows)))]
pub struct Pointer;

#[cfg(not(any(target_os = "linux", windows)))]
impl Pointer {
    pub fn of(_window: &slint::winit_030::winit::window::Window) -> Option<Self> {
        None
    }

    pub fn at(&self) -> Option<Drag> {
        None
    }
}

#[cfg(target_os = "linux")]
pub mod wayland {
    //! A `wl_data_device` for the seat, on winit's `wl_display` but on a
    //! queue of its own, which a thread dispatches: winit reads the socket
    //! and libwayland queues these events for the thread. It takes
    //! `text/uri-list` on a drop. There's a device for each seat, not for a
    //! window, so a window made again needs nothing new.

    use std::io::Read;
    use std::sync::atomic::{AtomicBool, Ordering};

    use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
    use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
    use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::protocol::wl_seat::WlSeat;
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};

    use super::Drag;

    const URI_LIST: &str = "text/uri-list";

    /// Watches for files dragged over Yapfast's windows on the connection whose
    /// `wl_display` this is, reporting each step to `report` from a thread.
    ///
    /// # Safety
    ///
    /// `display` is a live `wl_display`, which stays connected while the
    /// process runs, as winit's does for its event loop's life.
    pub unsafe fn watch(
        display: std::ptr::NonNull<std::ffi::c_void>,
        report: impl Fn(Drag) + Send + 'static,
    ) {
        let display = display.as_ptr() as usize;
        let thread = std::thread::Builder::new()
            .name("yapfast-drop".into())
            // It keeps little: the device, an offer, and a dropped list.
            .stack_size(128 * 1024)
            .spawn(move || {
                // SAFETY: the caller keeps the display connected.
                let backend = unsafe {
                    wayland_backend::client::Backend::from_foreign_display(display as *mut _)
                };
                let connection = Connection::from_backend(backend);
                tracing::debug!("watching for files dropped on the window");
                let mut queue = connection.new_event_queue();
                connection.display().get_registry(&queue.handle(), ());
                let mut state = State {
                    report: Box::new(report),
                    connection,
                    manager: None,
                    seats: Vec::new(),
                    offer: None,
                    left: None,
                };
                loop {
                    if let Err(error) = queue.blocking_dispatch(&mut state) {
                        tracing::warn!(%error, "files can no longer be dropped on the window");
                        return;
                    }
                }
            });
        if let Err(error) = thread {
            tracing::warn!(%error, "could not watch for files dropped on the window");
        }
    }

    struct State {
        report: Box<dyn Fn(Drag) + Send>,
        connection: Connection,
        manager: Option<WlDataDeviceManager>,
        /// Seats announced before the manager, waiting for it.
        seats: Vec<WlSeat>,
        /// The offer over a window, and whether it holds files.
        offer: Option<(WlDataOffer, bool)>,
        /// The offer a drag left with, kept until the next drag comes: Weston
        /// cancels a drag whose last offer is destroyed before another
        /// client's takes its place.
        left: Option<WlDataOffer>,
    }

    /// Whether an offer holds files.
    #[derive(Default)]
    struct Offered(AtomicBool);

    impl Dispatch<WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            registry: &WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            queue: &QueueHandle<Self>,
        ) {
            let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            else {
                return;
            };
            match interface.as_str() {
                // Version 3 for the actions a drop needs agreed.
                "wl_data_device_manager" if state.manager.is_none() => {
                    state.manager = Some(registry.bind(name, version.min(3), queue, ()));
                }
                "wl_seat" => state.seats.push(registry.bind(name, 1, queue, ())),
                _ => return,
            }
            if let Some(manager) = &state.manager {
                for seat in state.seats.drain(..) {
                    manager.get_data_device(&seat, queue, ());
                }
            }
        }
    }

    impl Dispatch<WlDataDevice, ()> for State {
        fn event(
            state: &mut Self,
            _: &WlDataDevice,
            event: wl_data_device::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                wl_data_device::Event::Enter {
                    serial,
                    x,
                    y,
                    id: Some(offer),
                    ..
                } => {
                    let files = offer
                        .data::<Offered>()
                        .is_some_and(|offered| offered.0.load(Ordering::Relaxed));
                    if files {
                        offer.accept(serial, Some(URI_LIST.into()));
                        if offer.version() >= 3 {
                            offer.set_actions(DndAction::Copy, DndAction::Copy);
                        }
                        // Where first, so the window shows the right place.
                        (state.report)(Drag::Moved { x, y });
                        (state.report)(Drag::Entered);
                    } else {
                        offer.accept(serial, None);
                    }
                    let old = state.offer.replace((offer, files));
                    for old in [old.map(|(old, _)| old), state.left.take()]
                        .into_iter()
                        .flatten()
                    {
                        old.destroy();
                    }
                }
                wl_data_device::Event::Motion { x, y, .. } => {
                    if state.offer.as_ref().is_some_and(|(_, files)| *files) {
                        (state.report)(Drag::Moved { x, y });
                    }
                }
                wl_data_device::Event::Leave => {
                    if let Some((offer, files)) = state.offer.take() {
                        state.left = Some(offer);
                        if files {
                            (state.report)(Drag::Left);
                        }
                    }
                }
                wl_data_device::Event::Drop => {
                    if let Some((offer, files)) = state.offer.take() {
                        let paths = files.then(|| state.receive(&offer));
                        if files && offer.version() >= 3 {
                            offer.finish();
                        }
                        offer.destroy();
                        if let Some(paths) = paths {
                            (state.report)(Drag::Dropped(paths));
                        }
                    }
                }
                // The clipboard's offers: Yapfast reads the clipboard elsewhere.
                wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),
                _ => {}
            }
        }

        event_created_child!(State, WlDataDevice, [
            wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Offered::default()),
        ]);
    }

    impl State {
        /// The files a dropped offer lists. The source writes them to a pipe,
        /// read here as the source sends them.
        fn receive(&self, offer: &WlDataOffer) -> Vec<std::path::PathBuf> {
            let Ok((mut reader, writer)) = std::io::pipe() else {
                return Vec::new();
            };
            offer.receive(URI_LIST.into(), std::os::fd::AsFd::as_fd(&writer));
            drop(writer);
            if let Err(error) = self.connection.flush() {
                tracing::warn!(%error, "could not ask for the dropped files");
                return Vec::new();
            }
            let mut list = String::new();
            match reader.read_to_string(&mut list) {
                Ok(_) => crate::paste::files(&list),
                Err(error) => {
                    tracing::warn!(%error, "could not read the dropped files");
                    Vec::new()
                }
            }
        }
    }

    impl Dispatch<WlDataOffer, Offered> for State {
        fn event(
            _: &mut Self,
            _: &WlDataOffer,
            event: wl_data_offer::Event,
            offered: &Offered,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_data_offer::Event::Offer { mime_type } = event
                && mime_type == URI_LIST
            {
                offered.0.store(true, Ordering::Relaxed);
            }
        }
    }

    impl Dispatch<WlDataDeviceManager, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlDataDeviceManager,
            _: <WlDataDeviceManager as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlSeat, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlSeat,
            _: <WlSeat as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
}
