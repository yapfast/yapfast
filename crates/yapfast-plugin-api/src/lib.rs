//! The wire types shared by Yapfast and its plugin helper.
//!
//! Nothing here runs: this crate is the contract between the parent
//! (`yapfast-core`) and `yapfast-plugins`, and between a guest and the host
//! functions it imports. It carries `serde` and no other dependency, because
//! both processes build it.
//!
//! A change here is a change to the plugin ABI. Additive changes keep
//! [`API_VERSION`]; anything that breaks a written plugin raises it, and a
//! manifest that asks for a higher version is refused rather than loaded.

#![forbid(unsafe_code)]

pub mod call;
pub mod capability;
pub mod event;
pub mod frame;
pub mod graph;
pub mod guest;
pub mod hook;
pub mod ids;
pub mod limits;
pub mod manifest;
pub mod message;

/// The version of the guest ABI and the wire protocol.
///
/// This is the `api` field of a manifest. A plugin built for a higher version
/// is refused with a message, never loaded and left to fail at instantiation.
pub const API_VERSION: u32 = 1;

pub use ids::PluginId;
pub use manifest::PluginManifest;
