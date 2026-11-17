//! Settings in Anomi's iced interface: the person's, and a server's and a
//! channel's. Each area of settings has its module here, one per task
//! (docs/iced/tasks.md), drawing its view-model from `anomi_vm::settings`.
//! The shell (`anomi-screens`) places them in the window. F01 keeps this list
//! and `shots.rs`; see docs/iced/README.md for the rules every screen
//! follows.

pub mod about;
pub mod account;
pub mod advanced;
pub mod appearance;
pub mod channel;
pub mod discord;
pub mod frame;
pub mod games;
pub mod keybinds;
mod kit;
pub mod messages;
pub mod mfa;
pub mod notifications;
pub mod server;
pub mod sounds;
pub mod voice;

#[cfg(feature = "shots")]
pub mod shots;
