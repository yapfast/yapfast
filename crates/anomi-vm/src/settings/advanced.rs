//! The Advanced settings page: what it shows and what it asks for. Task S10
//! (docs/iced/tasks/settings.md). The Slint side: `AdvancedSettings`
//! (`ui/settings.slint:1881`).

/// The Advanced settings page: what its screen draws. The window holds it as
/// `vm.settings.advanced`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Advanced {
    /// What the switches hold.
    pub gpu_rendering: bool,
    pub gpu_encoding: bool,
    pub close_to_tray: bool,
    pub multiple_windows: bool,
    pub developer_mode: bool,
    pub start_on_sign_in: bool,
    pub discord_links: bool,
    /// What the program runs with now: it differs from `gpu_rendering` until
    /// a restart, which the page then offers.
    pub gpu_running: bool,
    /// A system tray is running: closing keeps the program in it.
    pub tray_available: bool,
    /// This system can start the program at sign-in; the switch shows only then.
    pub autostart_available: bool,
    /// This system can hand `discord://` links to the program.
    pub discord_links_available: bool,
}

/// The Advanced settings page: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    StartOnSignIn(bool),
    DiscordLinks(bool),
    CloseToTray(bool),
    MultipleWindows(bool),
    GpuRendering(bool),
    GpuEncoding(bool),
    DeveloperMode(bool),
    /// Restart to switch between the GPU and the processor.
    Restart,
}
