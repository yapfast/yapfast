//! Bringing a shown window forward with the token a later start or a tray
//! click brought, on Wayland, where winit can't give one to a window it
//! already made: xdg-activation on the window's own surface.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

/// Activates `window`'s surface with `token`. Nothing on X11, where winit's
/// own focus request does.
pub fn activate<W: HasWindowHandle + HasDisplayHandle + ?Sized>(window: &W, token: &str) {
    use wayland_client::{
        Connection, Dispatch, Proxy, QueueHandle,
        backend::{Backend, ObjectId},
        globals::{GlobalListContents, registry_queue_init},
        protocol::{wl_registry::WlRegistry, wl_surface::WlSurface},
    };
    use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

    struct State;
    impl Dispatch<WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: <WlRegistry as Proxy>::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    impl Dispatch<XdgActivationV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &XdgActivationV1,
            _: <XdgActivationV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    let (Ok(display), Ok(handle)) = (window.display_handle(), window.window_handle()) else {
        return;
    };
    let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) =
        (display.as_raw(), handle.as_raw())
    else {
        return;
    };
    // SAFETY: winit's display and surface live while its window does, which
    // outlives this call.
    let connection = Connection::from_backend(unsafe {
        Backend::from_foreign_display(display.display.as_ptr().cast())
    });
    let surface =
        unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast()) }
            .and_then(|id| WlSurface::from_id(&connection, id));
    let result = surface
        .map_err(|error| error.to_string())
        .and_then(|surface| {
            let (globals, queue) =
                registry_queue_init::<State>(&connection).map_err(|error| error.to_string())?;
            // A compositor without the protocol leaves focus to winit's request.
            let activation: XdgActivationV1 = match globals.bind(&queue.handle(), 1..=1, ()) {
                Ok(activation) => activation,
                Err(wayland_client::globals::BindError::NotPresent) => return Ok(()),
                Err(error) => return Err(error.to_string()),
            };
            activation.activate(token.to_owned(), &surface);
            activation.destroy();
            connection.flush().map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        tracing::warn!(target: crate::LOG, %error, "could not bring the window forward");
    }
}

/// Whether `window` is a Wayland one: there a shown window comes forward
/// only through xdg-activation, as minimizing it can't be undone by asking.
pub fn wayland<W: HasDisplayHandle + ?Sized>(window: &W) -> bool {
    window
        .display_handle()
        .is_ok_and(|display| matches!(display.as_raw(), RawDisplayHandle::Wayland(_)))
}
