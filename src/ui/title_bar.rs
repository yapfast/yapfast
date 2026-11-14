//! The title bar Yapfast draws in place of the desktop's on Linux: whether it
//! does, and moving, sizing, minimizing, maximizing and closing the window
//! from it through winit. Yapfast also uses these actions for its Windows bar.
//!
//! Whether the desktop gives windows a title bar is `crate::frame`'s, until
//! the person chooses in Appearance.
//!
//! Each window is made with the frame or without, rather than given it or
//! losing it once made. Where the desktop draws no frame, as GNOME and
//! Weston, winit draws one, and putting it on a maximized window or taking it
//! off makes the window larger or smaller than maximized, for which Weston
//! ends the connection.

use slint::winit_030::winit::window::{ResizeDirection, WindowAttributes};

thread_local! {
    /// Windows are made without the desktop's frame, for Yapfast's title bar.
    static OWN: Cell<bool> = const { Cell::new(false) };
}

use super::*;

/// The edges and corners ResizeEdges numbers, in its order.
const DIRECTIONS: [ResizeDirection; 8] = [
    ResizeDirection::East,
    ResizeDirection::North,
    ResizeDirection::NorthEast,
    ResizeDirection::NorthWest,
    ResizeDirection::South,
    ResizeDirection::SouthEast,
    ResizeDirection::SouthWest,
    ResizeDirection::West,
];

/// Yapfast also offers the existing window actions on Windows. Upstream
/// presentation and other platforms keep their inherited frame policy.
fn offered() -> bool {
    cfg!(target_os = "linux") || (cfg!(windows) && yapfast::enabled())
}

/// Whether the desktop draws the title bar unless the person chose: on a
/// desktop not known to give windows one, it's the desktop's, which may be
/// none at all.
pub(super) fn system_by_default() -> bool {
    if cfg!(windows) && yapfast::enabled() && !cfg!(test) {
        false
    } else {
        cfg!(test) || !crate::frame::desktop_frames()
    }
}

/// Makes the windows from now on with Yapfast's title bar or the desktop's,
/// as the layout has it, and returns whether Yapfast's.
pub(super) fn choose(system: Option<bool>) -> bool {
    let own = offered() && !system.unwrap_or_else(system_by_default);
    OWN.set(own);
    own
}

/// Each window as Slint makes it: with the desktop's frame unless Yapfast
/// draws its own title bar.
pub(super) fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    attributes.with_decorations(!OWN.get())
}

/// Shows the title bar as the layout has it: the desktop's, or Yapfast's. A
/// window made since the choice changed shows it from before it's shown.
pub(super) fn show(window: &YapfastWindow, system: Option<bool>) {
    let own = choose(system);
    window.set_title_bar_choice(offered());
    window.set_system_title_bar(!own);
    window.set_own_title_bar(own);
}

/// What the title bar and the window's edges do, and the setting.
pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let weak = window.as_weak();
    window.on_move_window(move || {
        winit(&weak, |winit| {
            if let Err(error) = winit.drag_window() {
                tracing::warn!(%error, "could not move the window");
            }
        });
    });
    let weak = window.as_weak();
    window.on_resize_window(move |direction| {
        let Some(&direction) = usize::try_from(direction)
            .ok()
            .and_then(|index| DIRECTIONS.get(index))
        else {
            return;
        };
        winit(&weak, |winit| {
            if let Err(error) = winit.drag_resize_window(direction) {
                tracing::warn!(%error, "could not resize the window");
            }
        });
    });
    let weak = window.as_weak();
    // Straight to winit: Slint keeps whether the window is minimized from
    // winit, which on Wayland never says, so Slint's own may be stale.
    window.on_minimize_window(move || winit(&weak, |winit| winit.set_minimized(true)));
    let weak = window.as_weak();
    window.on_maximize_window(move || {
        if let Some(window) = weak.upgrade() {
            let native = window.window();
            native.set_maximized(!native.is_maximized());
        }
    });
    let weak = window.as_weak();
    window.on_close_window(move || {
        // As the desktop's close button: to the tray, or quit; see Shell::close.
        if let Some(window) = weak.upgrade() {
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        }
    });
    let adapter = Rc::downgrade(adapter);
    let weak = window.as_weak();
    window.on_title_bar_toggled(move || {
        let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade()) else {
            return;
        };
        let mut adapter = adapter.borrow_mut();
        let system = window.get_system_title_bar();
        adapter.layout.system_title_bar = Some(system);
        adapter.layout.write(&adapter.root);
        drop(adapter);
        // A maximized window is made again with the frame or without; see above.
        if window.window().is_maximized() {
            choose(Some(system));
            // Not from inside the window's own callback, which dropping it ends.
            slint::Timer::single_shot(Duration::ZERO, shell::remake_window);
        } else {
            show(&window, Some(system));
        }
    });
}

/// Acts on the window's winit window, once it has one.
fn winit(
    weak: &slint::Weak<YapfastWindow>,
    act: impl FnOnce(&slint::winit_030::winit::window::Window),
) {
    if let Some(window) = weak.upgrade() {
        window.window().with_winit_window(act);
    }
}
