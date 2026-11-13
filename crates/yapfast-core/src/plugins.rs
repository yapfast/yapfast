//! Plugins, as the application has them: the state a session keeps, the bridge
//! to the helper process, and the places the rest of the application asks what
//! the plugins want of it.
//!
//! What a plugin *is* — its manifest, its settings, the store it is installed
//! into, what it may do and reach, and what the Plugins page shows — is
//! `yapfast-plugins`, which knows about plugins and nothing about Yapfast. What
//! is here is the other half: [`state::PluginState`] holds a session's plugins
//! and answers for them, [`world`] hands a plugin the application's own state as
//! the value the wire carries, [`audio`] gives it the sounds and recordings only
//! the application has, and [`bridge`] runs the helper and its sessions. Both
//! halves are re-exported through this module, so `crate::plugins::page` and
//! `crate::plugins::state` read alike where the application uses them.
//!
//! Nothing in this half runs while no plugin is enabled:
//! [`state::PluginState`] is `None` in the application's state, and every tap
//! point in the send path, the event path and the menus checks that before it
//! looks at anything else.
//!
//! The parent holds the Discord token, the network stack and the consent
//! dialogs; the helper holds only WebAssembly and its scheduler. A guest's host
//! call therefore crosses the pipe to be answered here.
//!
//! ```text
//! application  ──Action::Plugin──▶  PluginState  ──frames──▶  helper
//!      ▲                                │                        │
//!      └──────Effect::Plugin────────────┘◀───────frames───────────┘
//! ```

pub mod audio;
pub mod bridge;
pub mod identity;
pub mod presence;
pub mod retention;
pub mod state;
pub mod view;
pub mod world;

// The plugin system's own knowledge, which the application names through this
// module as well: one import path for both halves.
pub use yapfast_plugins::*;

pub use state::PluginState;

/// Started with `--no-plugins`, the way back from an interface a plugin
/// made unusable: no plugin's `ui` section applies, and which plugins are
/// enabled stays. [`crate::cli::parse_no_plugins`] reads the flag.
pub fn safe_mode() -> bool {
    !crate::cli::plugins_allowed()
}

/// The plugins in the folder [`FOLDER_VARIABLE`] names: none when it names
/// none, or in safe mode.
pub fn load_from_environment() -> Vec<Found> {
    yapfast_plugins::load_from_environment(safe_mode())
}
