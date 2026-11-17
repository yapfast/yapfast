//! Samples of the Messages settings page for tests and shots, after the fake
//! Discord's data: task S04's.

use crate::Vm;
use crate::fixtures::settings::frame;
use crate::settings::frame::Page;
use crate::settings::messages::Messages;

/// The page as the golden screenshot has it: cozy, default size, 24-hour
/// clock, link previews on, Discord only.
pub fn page() -> Messages {
    Messages {
        link_previews: true,
        boost_gems: true,
        share_typing: true,
        reply_ping: true,
        strip_tracking: true,
        discord: true,
        ..Messages::default()
    }
}

/// The window with the settings open on Messages.
pub fn messages() -> Vm {
    with(page())
}

/// The window with Messages as `page` has it.
pub fn with(page: Messages) -> Vm {
    let mut vm = frame::settings(Page::Messages);
    vm.settings.messages = page;
    vm
}

/// Compact display with the 12-hour clock, seconds and large text.
pub fn compact() -> Messages {
    use crate::settings::messages::TextSize;
    Messages {
        compact: true,
        twelve_hour: true,
        seconds: true,
        text_size: TextSize::Large,
        matrix: true,
        ..page()
    }
}
