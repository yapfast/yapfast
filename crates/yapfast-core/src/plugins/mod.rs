//! The plugin system's parent half: a bridge to the helper process, the
//! registry of what is enabled, permissions, and the network gate.
//!
//! Nothing here runs while no plugin is enabled: [`state::PluginState`] is
//! `None` in the application's state, and every tap point in the send path, the
//! event path and the menus checks that before it looks at anything else.
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

pub mod bridge;
pub mod grants;
pub mod network;
pub mod permissions;
pub mod registry;
pub mod spawn;
pub mod state;
pub mod store;

pub use state::PluginState;
