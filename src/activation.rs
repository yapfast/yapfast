//! Raising the window on Wayland. A compositor only brings a window forward
//! for an xdg-activation token made from user input, and winit's
//! `focus_window` does nothing there. KDE Plasma gives the tray click's
//! token to the tray item (see `tray::take_activation_token`); this spends it.

use std::ffi::c_void;
use std::ptr::NonNull;

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::{self, XdgActivationV1};

#[derive(Default)]
struct State {
    activation: Option<XdgActivationV1>,
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface == "xdg_activation_v1"
        {
            state.activation = Some(registry.bind(name, version.min(1), queue, ()));
        }
    }
}

impl Dispatch<XdgActivationV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &XdgActivationV1,
        _: xdg_activation_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Asks the compositor to activate `surface` with `token`; whether it asked.
///
/// # Safety
///
/// `display` and `surface` are winit's live `wl_display` and the window's
/// `wl_surface`.
pub unsafe fn raise(display: NonNull<c_void>, surface: NonNull<c_void>, token: String) -> bool {
    // SAFETY: the caller's display stays connected; a foreign backend does not
    // close it.
    let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
    let connection = Connection::from_backend(backend);
    let mut queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut state = State::default();
    if let Err(error) = queue.roundtrip(&mut state) {
        tracing::debug!(%error, "could not look for xdg_activation_v1");
        return false;
    }
    let Some(activation) = state.activation.take() else {
        tracing::debug!("the compositor has no xdg_activation_v1");
        return false;
    };
    // SAFETY: the caller's surface is a live `wl_surface` of this display.
    let surface = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.as_ptr().cast()) }
        .ok()
        .and_then(|id| WlSurface::from_id(&connection, id).ok());
    let Some(surface) = surface else {
        return false;
    };
    activation.activate(token, &surface);
    activation.destroy();
    let _ = connection.flush();
    true
}
