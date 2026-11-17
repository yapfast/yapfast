//! Samples of the Advanced settings page for tests and shots, after the fake
//! Discord's data: task S10's.

use crate::Vm;
use crate::fixtures::settings::frame::settings;
use crate::settings::advanced::Advanced;
use crate::settings::frame::Page;

/// The golden screenshot's: drawing on the processor, no system tray, link
/// handling and autostart offered.
pub fn advanced() -> Advanced {
    Advanced {
        gpu_rendering: false,
        gpu_encoding: true,
        close_to_tray: true,
        multiple_windows: false,
        developer_mode: false,
        start_on_sign_in: false,
        discord_links: false,
        gpu_running: false,
        tray_available: false,
        autostart_available: true,
        discord_links_available: true,
    }
}

/// The window with the settings open on Advanced.
pub fn page() -> Vm {
    let mut vm = settings(Page::Advanced);
    vm.settings.advanced = advanced();
    vm
}

/// The GPU switched on, which waits for a restart.
pub fn restart() -> Vm {
    let mut vm = page();
    vm.settings.advanced.gpu_rendering = true;
    vm
}

/// A tray, but no autostart and no link handling: those switches go.
pub fn bare() -> Vm {
    let mut vm = page();
    let page = &mut vm.settings.advanced;
    page.tray_available = true;
    page.autostart_available = false;
    page.discord_links_available = false;
    vm
}
