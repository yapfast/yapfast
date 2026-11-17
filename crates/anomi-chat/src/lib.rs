//! The conversation of Anomi's iced interface: the timeline, the message and
//! all it holds, the composer, the pickers, threads, forums, search and pins.
//! Each area of chat has its module here, one per task (docs/iced/tasks.md),
//! drawing its view-model from `anomi_vm::chat`. The shell (`anomi-screens`)
//! places them in the window. F01 keeps this list and `shots.rs`; see
//! docs/iced/README.md for the rules every screen follows.

pub mod attachments;
pub mod bot_forms;
pub mod cards;
pub mod components;
pub mod composer;
pub mod embeds;
pub mod forum;
pub mod forward;
pub mod inline_edit;
pub mod menu;
pub mod message;
pub mod pickers;
pub mod pins;
pub mod poll;
pub mod reactions;
pub mod search;
pub mod threads;
pub mod timeline;
pub mod video;
pub mod viewer;
pub mod voice_message;

#[cfg(feature = "shots")]
pub mod shots;
