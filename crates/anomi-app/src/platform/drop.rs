//! Files dragged over the window and dropped on it, on Wayland, where winit
//! has no drag and drop (`watch_wayland_drops` and `dragged` in
//! `src/ui.rs`). A data device of the app's own on winit's connection
//! (`anomi_desktop::drop::wayland`), started as the first window opens,
//! tells of each step through `Heard::Drag`; the device is the seat's, so a
//! window made again needs nothing new. Each step becomes what winit's own
//! events become on X11 and Windows (`crate::window`): the composer's
//! `Done::Hovered` and `Done::Dropped`.

use anomi_desktop::drop::Drag;
use iced::{Task, window};

use crate::app::App;
use crate::intent::chat::composer::{self, Done};
use crate::message::Message;

/// Starts following files dragged over the window, once, where the window
/// is on Wayland.
#[cfg(target_os = "linux")]
pub fn watch(app: &mut App, id: window::Id) -> Task<Message> {
    if app.desktop.drops {
        return Task::none();
    }
    let Some(tell) = app.desktop.tell.clone() else {
        return Task::none();
    };
    app.desktop.drops = true;
    window::run(id, move |window| {
        let Some(display) = anomi_desktop::drop::wayland::display(window) else {
            return;
        };
        let report = move |drag| {
            let _ = tell.send(super::Heard::Drag(drag));
        };
        // SAFETY: winit's display stays connected while its event loop
        // runs, which is as long as the app does.
        unsafe { anomi_desktop::drop::wayland::watch(display, report) };
    })
    .discard()
}

/// winit follows drags itself elsewhere.
#[cfg(not(target_os = "linux"))]
pub fn watch(_app: &mut App, _id: window::Id) -> Task<Message> {
    Task::none()
}

/// A step of a drag over the window: the composer says where the files
/// would go, and takes them when they're let go: the thread beside the
/// channel's while the pointer is over it, else the channel's.
pub fn dragged(app: &mut App, drag: Drag) -> Task<Message> {
    match drag {
        Drag::Entered => {
            app.shown.composer.over_aside = false;
            composer::done(app, Done::Hovered(true))
        }
        Drag::Left => composer::done(app, Done::Hovered(false)),
        Drag::Moved { x, y } => {
            let zoom = f64::from(app.zoom.level());
            let aside = over_aside(app, (x / zoom) as f32, (y / zoom) as f32);
            if aside != app.shown.composer.over_aside {
                app.shown.composer.over_aside = aside;
                if app.vm.shell.drop.is_some() {
                    composer::hovered(app, true);
                }
            }
            Task::none()
        }
        Drag::Dropped(paths) => {
            let tasks: Vec<_> = (paths.into_iter())
                .map(|path| composer::done(app, Done::Dropped(path)))
                .collect();
            Task::batch(tasks)
        }
    }
}

/// Whether a point of the window, in the view's pixels, is over the thread
/// beside the channel, below the header (`over-aside`,
/// `ui/yapfast.slint:727`).
fn over_aside(app: &App, x: f32, y: f32) -> bool {
    let state = &app.core.state;
    let docked =
        state.aside.is_some() && state.new_thread().is_none() && state.search_shown().is_none();
    let (vm, width) = (&app.vm, app.size.width);
    let docking = anomi_screens::shell::Docking::new(&vm.shell, &vm.conversation.beside, width);
    let Some((_, shown)) = docking.aside.filter(|_| docked) else {
        return false;
    };
    let bar = if vm.chrome.framed && !vm.chrome.full_screen {
        anomi_screens::chrome::HEIGHT
    } else {
        0.0
    };
    x >= width - shown && y >= bar + anomi_theme::metrics::HEADER_HEIGHT
}
