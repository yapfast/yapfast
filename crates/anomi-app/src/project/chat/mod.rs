//! The conversation's areas from the core's state, one module each, its I
//! task's. The integration tasks add their calls here in turn, as in
//! `project/mod.rs`.

mod attachments;
mod bot_forms;
mod cards;
mod components;
mod composer;
mod embeds;
mod forum;
mod message;
mod pickers;
mod pins;
mod poll;
mod reactions;
mod search;
mod threads;
mod timeline;
