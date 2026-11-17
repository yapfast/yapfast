//! A message's reactions, and the Reactions window: what it shows and what it
//! asks for. Task C03 (docs/iced/tasks/chat.md). Ports `ReactionToggle`
//! (`ui/messages.slint:272`), the `ReactionHover` global and
//! `ReactionsDialog` (`ui/dialogs.slint:797`).
//!
//! The chips hold what the Slint `ReactionChip` and `show_reactions` made:
//! the emoji as glyphs, or as its picture for a custom one, and the line of
//! who reacted that the app writes (`messages::reaction_tip`) once the names
//! came. The app asks for the names when it hears [`Intent::Hover`], once the
//! pointer stayed half a second, as Discord's client does.

use iced::Color;

use crate::kind::{Avatar, Expected};

/// What draws a reaction: an emoji's glyphs, drawn by the text as Twemoji,
/// or a custom emoji's picture, whose room stays while it comes.
#[derive(Clone, Debug, PartialEq)]
pub enum Face {
    Glyphs(String),
    Picture(Expected),
}

impl Default for Face {
    fn default() -> Self {
        Self::Glyphs(String::new())
    }
}

/// One reaction under a message (`ReactionChip`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chip {
    /// Discord's key: the glyphs, or `name:id` for a custom emoji.
    pub key: String,
    pub face: Face,
    /// Its shortcode, as a screen reader says it.
    pub name: String,
    pub count: u64,
    /// This account reacted with it.
    pub mine: bool,
    /// A super reaction (Discord's burst): its emoji's colour, which tints
    /// and outlines the chip; none for an ordinary one.
    pub burst: Option<Color>,
    /// Who reacted, as a line, once the names came; empty before.
    pub who: String,
}

/// A message's reactions: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reactions {
    /// The message they are under, which an intent names.
    pub item: u64,
    pub chips: Vec<Chip>,
    /// This account may add and remove its reaction here (`enabled`).
    pub enabled: bool,
}

/// One tab of the Reactions window (`ReactionPill`): a reaction, or a
/// poll's answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pill {
    /// The reaction's key, or the answer's id.
    pub key: String,
    /// Its emoji; none for an answer without one.
    pub face: Option<Face>,
    /// The answer's text; empty for a reaction.
    pub name: String,
    pub count: u64,
}

/// A person who reacted (`ReactorRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reactor {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: Avatar,
    /// This account may remove their reaction: its own, or as a moderator.
    pub removable: bool,
}

/// The Reactions window: who reacted with what. The window holds it as
/// `vm.chat.reactions`. It lists a poll's voters the same way, by answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dialog {
    /// The tabs, the most first.
    pub pills: Vec<Pill>,
    /// A poll's voters rather than reactions: wide tabs with their text.
    pub voters: bool,
    /// The chosen tab's key.
    pub chosen: String,
    pub people: Vec<Reactor>,
    pub loading: bool,
    /// More people than shown: the next page comes near the end.
    pub more: bool,
    pub error: Option<String>,
}

/// A message's reactions, and the Reactions window: what a click or a key on
/// it asks for. Keep `#[non_exhaustive]`: the app matches it with a wildcard
/// until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Add this account's reaction, or take it back (`react`).
    Toggle(u64, String),
    /// The pointer came onto a chip: the app fetches who reacted and fills
    /// its `who` (`hover-reaction`).
    Hover(u64, String),
    /// The pointer left that chip: its card goes, unless another chip is
    /// hovered by now (moving between two, the next one's `Hover` can come
    /// first).
    Left(u64, String),
    /// Open the window at a reaction (`view-reactions`).
    Open(u64, String),
    /// A tab (`select-reaction`).
    Select(String),
    /// The people in view changed: near the list's end, the app asks for
    /// the next page while there is one and none is on its way
    /// (`more-reactors`). Sent only while [`Dialog::more`] and not
    /// [`Dialog::loading`].
    Scrolled {
        near_end: bool,
    },
    /// Take a person's reaction away (`remove-reactor`).
    Remove(String),
    Close,
}
