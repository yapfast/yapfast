//! The iced interface's own widgets, one module each, styled from
//! `anomi-theme`. A widget here knows nothing of an area: it takes plain
//! values and the vocabulary types of `anomi_vm::kind`, and returns a widget
//! of the interface. Screens compose them.
//!
//! Two kinds live here: compositions of iced's widgets (a function returning
//! `impl View<Message>`, as [`avatar::avatar`]) and widgets of our own that
//! implement `iced::advanced::Widget` (as [`rail_item::RailItem`]). Read
//! `docs/iced/iced-notes.md` before writing the second kind.

pub mod avatar;
pub mod badge;
pub mod brand;
pub mod field;
pub mod icon;
pub mod picture;
pub mod rail_item;

pub use anomi_theme::{Element, Theme, View};
