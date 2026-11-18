//! The conversation's areas from the core's state, one module each, its I
//! task's. The integration tasks add their calls here in turn, as in
//! `project/mod.rs`.

pub(crate) mod attachments;
pub(crate) mod bot_forms;
pub(crate) mod cards;
mod components;
pub(crate) mod composer;
pub(crate) mod edit;
pub(crate) mod embeds;
mod forum;
pub(crate) mod forward;
pub(crate) mod kept;
pub(crate) mod message;
pub(crate) mod pickers;
mod pins;
pub(crate) mod poll;
pub(crate) mod reactions;
mod search;
mod threads;
pub(crate) mod timeline;
