//! Voice in Anomi's iced interface: the dock, the stage and its tiles, call
//! controls, devices, sharing and the soundboard. Each area of voice has its
//! module here, one per task (docs/iced/tasks.md), drawing its view-model
//! from `anomi_vm::voice`. The shell (`anomi-screens`) places them in the
//! window. F01 keeps this list and `shots.rs`; see docs/iced/README.md for
//! the rules every screen follows.

pub mod connection;
pub mod controls;
pub mod devices;
pub mod dock;
pub mod incoming;
pub mod join;
pub mod participant_menu;
pub mod pip;
pub mod share;
pub mod soundboard;
pub mod stage;
pub mod tiles;

#[cfg(feature = "shots")]
pub mod shots;
