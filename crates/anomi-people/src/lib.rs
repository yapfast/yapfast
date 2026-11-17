//! People in Anomi's iced interface: friends, Active Now, the member list,
//! profiles, status and the profile editor. Each area of people has its
//! module here, one per task (docs/iced/tasks.md), drawing its view-model
//! from `anomi_vm::people`. The shell (`anomi-screens`) places them in the
//! window. F01 keeps this list and `shots.rs`; see docs/iced/README.md for
//! the rules every screen follows.

pub mod active_now;
pub mod activity;
pub mod add_people;
pub mod cards;
pub mod editor;
pub mod friends;
pub mod members;
pub mod person;
pub mod profile;
pub mod status;

#[cfg(feature = "shots")]
pub mod shots;
