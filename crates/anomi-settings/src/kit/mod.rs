//! The parts every settings page is built from (task S01, docs/iced/tasks/
//! settings.md): a switch, a row of exclusive choices, a drop-down, a text
//! of several lines, a link button, the bar for unsaved changes and the card
//! settings open on, with the titles and sections' rows around them. The
//! pages of S02-S10 and the server's and channel's settings (X) draw with
//! these and with the foundation's widgets, never their own. The Slint side
//! is `ui/settings.slint`.
//!
//! The kit is the crate's own (`mod kit`), so what a page doesn't use yet is
//! not dead: the allowance below goes when every part has a user.
#![allow(dead_code, unused_imports)]

mod area;
mod card;
mod choices;
mod heading;
mod leaf;
mod link;
mod nav;
mod picker;
mod press;
mod pressable;
mod sections;
mod switch;
mod unsaved;

pub use area::area;
pub use card::card;
pub use choices::choices;
pub use heading::{heading, title};
pub use leaf::{Ink, Leaf};
pub use link::link;
pub use nav::nav_row;
pub use picker::picker;
pub use press::mix;
pub use pressable::pressable;
pub use sections::{Row, WIDTH, sections};
pub use switch::switch;
pub use unsaved::unsaved_bar;

#[cfg(test)]
mod tests;
