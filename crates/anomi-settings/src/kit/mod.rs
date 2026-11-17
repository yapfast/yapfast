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
#![allow(dead_code)]

mod area;
mod arrange;
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

#[allow(unused_imports)]
pub use area::area;
#[allow(unused_imports)]
pub use card::{card, chat_ground};
#[allow(unused_imports)]
pub use choices::choices;
#[allow(unused_imports)]
pub use heading::{heading, title};
#[allow(unused_imports)]
pub use leaf::{Ink, Leaf};
#[allow(unused_imports)]
pub use link::link;
#[allow(unused_imports)]
pub use nav::nav_row;
#[allow(unused_imports)]
pub use picker::picker;
#[allow(unused_imports)]
pub use press::mix;
#[allow(unused_imports)]
pub use pressable::pressable;
#[allow(unused_imports)]
pub use sections::{Row, WIDTH, sections};
#[allow(unused_imports)]
pub use switch::switch;
#[allow(unused_imports)]
pub use unsaved::unsaved_bar;

#[cfg(test)]
mod tests;
