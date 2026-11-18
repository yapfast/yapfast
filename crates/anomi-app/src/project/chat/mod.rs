//! The conversation's areas from the core's state, one module each, its I
//! task's. The integration tasks add their calls here in turn, as in
//! `project/mod.rs`.

pub(crate) mod attachments;
mod bot_forms;
mod cards;
mod components;
pub(crate) mod composer;
pub(crate) mod edit;
pub(crate) mod embeds;
mod forum;
pub(crate) mod forward;
pub(crate) mod message;
mod pickers;
mod pins;
mod poll;
pub(crate) mod reactions;
mod search;
mod threads;
pub(crate) mod timeline;
