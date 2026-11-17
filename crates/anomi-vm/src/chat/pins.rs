//! The pins popover: what it shows and what it asks for. Task C22
//! (docs/iced/tasks/chat.md). The Slint side: `PinsPopover`
//! (`ui/header.slint:275`), whose rows the app made in `show_pins`.

use crate::chat::search::Found;
use crate::text::Link;

/// The pins popover: what its screen draws. The window holds it as
/// `vm.chat.pins`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pins {
    /// Said instead of the list: while it loads, when it fails, or when
    /// there are no pins (`note`).
    pub note: String,
    /// The pinned messages as cards, the latest pinned first (`pins`).
    pub pins: Vec<Found>,
}

/// The pins popover: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A card, or its Jump button: show the message in its room (`jump`).
    Jump { room: String, item: String },
    /// A link in a pinned message's text (`open-link`).
    Link(Link),
    /// The cross (`close`).
    Close,
}
