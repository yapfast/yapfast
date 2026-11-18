//! The iced interface's own widgets, one module each, styled from
//! `anomi-theme`. A widget here knows nothing of an area: it takes plain
//! values and the vocabulary types of `anomi_vm::kind`, and returns a widget
//! of the interface. Screens compose them.
//!
//! Two kinds live here: compositions of iced's widgets (a function returning
//! `impl View<Message>`, as [`avatar::avatar`]) and widgets of our own that
//! implement `iced::advanced::Widget` (as [`rail_item::RailItem`]). Read
//! `docs/iced/iced-notes.md` before writing the second kind. F01 keeps this
//! list; each module is its task's (docs/iced/tasks.md).

pub mod avatar;
pub mod badge;
pub mod brand;
pub mod button;
pub mod clock;
pub mod composer;
#[cfg(test)]
mod counting;
pub mod cover;
pub mod effect;
pub mod field;
pub mod focus;
pub mod frame;
pub mod icon;
pub mod layer;
pub mod list;
pub mod menu;
pub mod name;
pub mod nameplate;
pub mod new_tag;
pub mod paragraph;
pub mod picture;
pub mod presence;
pub mod rail_item;
pub mod scrollbar;
pub mod selection;
pub mod shake;
pub mod skeleton;
pub mod slider;
pub mod tag;
pub mod tip;

pub use anomi_theme::{Element, Theme, View};
