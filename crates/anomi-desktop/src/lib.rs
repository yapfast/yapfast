//! What both of Anomi's clients, the Slint app (`src/`) and the iced one
//! (`crates/anomi-app`), ask of the desktop beyond their window toolkit, in
//! one place rather than a copy in each: the tray icon ([`tray`]), one
//! instance a data folder and bringing its window forward ([`instance`]),
//! the window's icon and app ID ([`icon`]) and giving memory back
//! ([`memory`]). Nothing here knows a toolkit: what the desktop says comes
//! back through closures each client takes onto its own event loop, and a
//! window is anything with raw window handles.

/// Where this crate logs: under `yapfast`, which both clients' log filters
/// let through (`yapfast_core::logging` knows only those targets).
const LOG: &str = "yapfast::desktop";

pub mod icon;
pub mod instance;
pub mod memory;
pub mod tray;
