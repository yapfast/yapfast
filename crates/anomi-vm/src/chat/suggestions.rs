//! The composer's suggestions: what it shows and what it asks for. The Slint
//! side: `SuggestionFrame`, `MentionList` and `EmojiSuggestions`
//! (`ui/composer.slint:104`-`:195`), filled by `src/ui/refresh.rs:4337`
//! (`show_emoji_suggestions`) and the mentions beside it, and chosen by
//! `src/ui/callbacks.rs:2806`-`:2900`.
//!
//! One list at most shows over the field: the people, roles and channels the
//! `@` or `#` word being typed may name, or the emoji the `:name` may mean
//! (Slint shows the emoji only while no mention does, and the app builds
//! them from one word at a time, so [`Offer`] holds one or the other). Up,
//! Down, Tab, Enter and Escape are the field's keys
//! ([`composer::Suggest`](super::composer::Suggest)); a click on a row is
//! [`Intent::Choose`].

use iced::Color;

use super::emoji_picker::Emoji;
use crate::kind::Avatar;

/// The composer's suggestions: what its screen draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Suggestions {
    pub offer: Offer,
}

impl Suggestions {
    /// Something is offered (the list has rows).
    pub fn showing(&self) -> bool {
        match &self.offer {
            Offer::Nothing => false,
            Offer::Mentions { rows, .. } => !rows.is_empty(),
            Offer::Emoji { cells, .. } => !cells.is_empty(),
        }
    }

    /// How many rows there are.
    pub fn len(&self) -> usize {
        match &self.offer {
            Offer::Nothing => 0,
            Offer::Mentions { rows, .. } => rows.len(),
            Offer::Emoji { cells, .. } => cells.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What the word being typed is offered.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Offer {
    #[default]
    Nothing,
    /// `MentionList`: `@` and `#`.
    Mentions {
        rows: Vec<Mention>,
        /// The highlighted row (`mention-index`).
        index: usize,
    },
    /// `EmojiSuggestions`: `:`.
    Emoji {
        /// The name typed so far, without its colon (`emoji-suggestion-query`).
        query: String,
        cells: Vec<Cell>,
        /// The highlighted row (`emoji-suggestion-index`).
        index: usize,
    },
}

/// Someone or something the `@` or `#` word may name (`MentionRow`).
#[derive(Clone, Debug, PartialEq)]
pub struct Mention {
    pub name: String,
    pub mark: Mark,
    /// The name's colour, from a coloured Discord role (`has-color`).
    pub color: Option<Color>,
    /// Their username, or what a group reaches, such as "Everyone in this
    /// channel".
    pub detail: String,
    /// The heading over the first member and the first role, such as
    /// "MEMBERS" (already translated and upper-cased by the app); empty
    /// under no heading.
    pub heading: String,
}

/// What stands before a name.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// A person: their avatar.
    Person(Avatar),
    /// A role or a group such as @everyone, or a channel: a round mark
    /// holding the sigil, "@" or "#".
    Group(String),
    /// A voice channel: a speaker in the round mark instead of a sigil.
    Voice,
}

/// One emoji the `:name` may mean (`EmojiCell`, as the suggestions use it).
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub emoji: Emoji,
    /// ":party:", as the row says it.
    pub name: String,
}

/// The composer's suggestions: what a click on them asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A click on the row at this index (`mention-chosen`,
    /// `emoji-suggestion-chosen`): put it in the text where the word is.
    Choose(usize),
}
