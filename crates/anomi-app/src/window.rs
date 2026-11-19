//! The main window: how it opens, and what of it reaches `update` (its focus
//! and its close button).

use iced::event::{self, Event};
use iced::{Size, Subscription, Task, window};
use yapfast_core::application::Action;

use anomi_theme::metrics;

use crate::app::App;
use crate::intent;
use crate::intent::chat::composer::Done;
use crate::message::Message;

/// The desktop entry's id, so the desktop finds the window's icon; the Slint
/// app's.
pub const APP_ID: &str = anomi_desktop::icon::APP_ID;

/// How the main window opens: the test display's size under it, else the
/// first start's.
pub fn settings() -> window::Settings {
    window::Settings {
        size: test_size().unwrap_or(Size::new(metrics::WINDOW_WIDTH, metrics::WINDOW_HEIGHT)),
        min_size: Some(Size::new(
            metrics::WINDOW_MIN_WIDTH,
            metrics::WINDOW_MIN_HEIGHT,
        )),
        // The close button asks; `update` decides: to the tray, or quit.
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: APP_ID.into(),
            ..Default::default()
        },
        ..window::Settings::default()
    }
}

/// `YAPFAST_UI_TEST_SIZE`, as `scripts/ui-session` sets it, on its private
/// display alone.
pub fn test_size() -> Option<Size> {
    if std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() != Ok("1") {
        return None;
    }
    let size = std::env::var("YAPFAST_UI_TEST_SIZE").ok()?;
    let (width, height) = size.split_once('x')?;
    let (width, height) = (width.parse::<f32>().ok()?, height.parse::<f32>().ok()?);
    (width >= metrics::WINDOW_MIN_WIDTH && height >= metrics::WINDOW_MIN_HEIGHT)
        .then(|| Size::new(width, height))
}

/// The window's focus, its size, its display's scale, Shift, files dragged
/// over it, its close button, its keys for keybinds (handed on with no
/// message, `platform::keybinds::keys`), and input, which says someone uses it
/// (`clocks::away`; one filter for all, as each wakes iced's executor for
/// every event, frames included). Nothing here ticks: each fires only as
/// the desktop says something happened.
pub fn subscription() -> Subscription<Message> {
    Subscription::batch([
        event::listen_with(focus),
        window::close_requests().map(Message::CloseRequested),
    ])
}

fn focus(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    if let Some(recorded) = crate::platform::keybinds::keys::heard(&event) {
        return Some(recorded);
    }
    if let Some(used) = crate::clocks::away::input(&event) {
        return Some(used);
    }
    match event {
        Event::Window(window::Event::Focused) => Some(Message::Focused(true)),
        Event::Window(window::Event::Unfocused) => Some(Message::Focused(false)),
        Event::Window(window::Event::Rescaled(scale)) => Some(Message::Rescaled(scale)),
        Event::Window(window::Event::Resized(size)) => Some(Message::Resized(size)),
        // Files dragged over the window and dropped, as winit tells of
        // them (X11, Windows): the composer's next message takes them.
        Event::Window(window::Event::FileHovered(_)) => Some(dragged(Done::Hovered(true))),
        Event::Window(window::Event::FilesHoveredLeft) => Some(dragged(Done::Hovered(false))),
        Event::Window(window::Event::FileDropped(path)) => Some(dragged(Done::Dropped(path))),
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::Shift(modifiers.shift()))
        }
        _ => None,
    }
}

fn dragged(done: Done) -> Message {
    Message::Composer(done)
}

/// The window came to the front or went behind another window. What moves
/// follows (`motion.front`: behind another window nothing moves), and the
/// core hears, which asks Discord for the Activity panel's feed only in
/// front, as Discord's client does. The core is told the window is behind
/// as the app starts (`App::new`), until the window says otherwise. Whether
/// the newest messages are being read follows the focus too: the core hears
/// it here as well as with each report of the rows in view. In front, the
/// taskbar stops flashing and every clock says the time again; behind,
/// their waits go (`crate::clocks`), and what hangs off the window closes,
/// menus, popovers, cards and pickers, as in Discord's client
/// (`intent::shell::unfocused`). Only the window's own `Unfocused` brings
/// that, and closing to the tray, which closes them all anyway.
pub fn focused(app: &mut App, front: bool) -> Task<Message> {
    tracing::debug!(front, "the window's focus");
    if !front {
        crate::platform::keybinds::record::unfocused(app);
    }
    app.vm.chrome.focused = front;
    if std::mem::replace(&mut app.motion.front, front) == front {
        return Task::none();
    }
    let task = app.dispatch(Action::WindowFocused(front));
    let flash = if front {
        crate::clocks::front(app);
        // The taskbar's flash for a message that came meanwhile stops.
        crate::effects::notify::front(app)
    } else {
        intent::shell::unfocused(app)
    };
    Task::batch([task, flash, intent::chat::reading(app)])
}

#[cfg(test)]
mod tests;
