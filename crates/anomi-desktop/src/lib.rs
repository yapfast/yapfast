//! What both of Anomi's clients, the Slint app (`src/`) and the iced one
//! (`crates/anomi-app`), ask of the desktop beyond their window toolkit, in
//! one place rather than a copy in each: the tray icon ([`tray`]), one
//! instance a data folder and bringing its window forward ([`instance`]),
//! the window's icon and app ID ([`icon`]), giving memory back
//! ([`memory`]), desktop notifications ([`notify`]), the browser
//! ([`browser`]), the clipboard's pictures and files ([`copy`], [`paste`]),
//! files dropped on the window ([`drop`]), and the system's links and
//! sign-in ([`protocol`], [`autostart`], `desktop_entry`). Nothing here
//! knows a toolkit: what the desktop says comes back through closures each
//! client takes onto its own event loop, and a window is anything with raw
//! window handles.

/// Where this crate logs: under `yapfast`, which both clients' log filters
/// let through (`yapfast_core::logging` knows only those targets).
const LOG: &str = "yapfast::desktop";

pub mod autostart;
pub mod browser;
pub mod copy;
#[cfg(target_os = "linux")]
pub mod desktop_entry;
pub mod drop;
pub mod icon;
pub mod instance;
pub mod memory;
pub mod notify;
pub mod paste;
pub mod protocol;
pub mod tray;
