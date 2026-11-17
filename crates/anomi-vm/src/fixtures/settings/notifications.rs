//! Samples of the Notifications settings page for tests and shots, after the
//! fake Discord's data: task S05's.

use crate::Vm;
use crate::fixtures::settings::frame::settings;
use crate::settings::frame::Page;
use crate::settings::notifications::Notifications;

/// All three on, Discord alone: the golden screenshot's.
pub fn notifications() -> Notifications {
    Notifications {
        notifications: true,
        text: true,
        flash_taskbar: true,
        matrix: false,
        discord: true,
    }
}

/// The window with the settings open on Notifications.
pub fn page() -> Vm {
    let mut vm = settings(Page::Notifications);
    vm.settings.notifications = notifications();
    vm
}

/// Notifications off, which leaves the text's switch unavailable.
pub fn off() -> Vm {
    let mut vm = page();
    vm.settings.notifications.notifications = false;
    vm
}

/// Both networks in use, whose rules both decide.
pub fn both() -> Vm {
    let mut vm = page();
    vm.settings.notifications.matrix = true;
    vm
}
