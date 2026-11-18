//! The main window: how it opens, and what of it reaches `update` (its focus
//! and its close button).

use iced::event::{self, Event};
use iced::{Size, Subscription, window};

use anomi_theme::metrics;

use crate::message::Message;

/// The desktop entry's id, so the desktop finds the window's icon; the Slint
/// app's (`src/icon.rs`).
pub const APP_ID: &str = "io.github.yapfast.Yapfast";

/// How the main window opens: the test display's size under it, else the
/// first start's.
pub fn settings() -> window::Settings {
    window::Settings {
        size: test_size().unwrap_or(Size::new(metrics::WINDOW_WIDTH, metrics::WINDOW_HEIGHT)),
        min_size: Some(Size::new(
            metrics::WINDOW_MIN_WIDTH,
            metrics::WINDOW_MIN_HEIGHT,
        )),
        // The close button asks; `update` decides (the tray, once it's here).
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
fn test_size() -> Option<Size> {
    if std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() != Ok("1") {
        return None;
    }
    let size = std::env::var("YAPFAST_UI_TEST_SIZE").ok()?;
    let (width, height) = size.split_once('x')?;
    let (width, height) = (width.parse::<f32>().ok()?, height.parse::<f32>().ok()?);
    (width >= metrics::WINDOW_MIN_WIDTH && height >= metrics::WINDOW_MIN_HEIGHT)
        .then(|| Size::new(width, height))
}

/// The window's focus, its display's scale and its close button. Nothing
/// here ticks: each fires only as the desktop says something happened.
pub fn subscription() -> Subscription<Message> {
    Subscription::batch([
        event::listen_with(focus),
        window::close_requests().map(Message::CloseRequested),
    ])
}

fn focus(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        Event::Window(window::Event::Focused) => Some(Message::Focused(true)),
        Event::Window(window::Event::Unfocused) => Some(Message::Focused(false)),
        Event::Window(window::Event::Rescaled(scale)) => Some(Message::Rescaled(scale)),
        _ => None,
    }
}
