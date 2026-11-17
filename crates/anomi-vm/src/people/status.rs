//! The custom status dialog and the status menu: what they show and what
//! they ask for. Task P10 (docs/iced/tasks/people.md). The Slint side is
//! `StatusDialog`, `StatusMenu` and `StatusOption` (`ui/dialogs.slint:27`,
//! `:621`, `:567`), filled by `src/ui/refresh.rs` (the `status-*` properties
//! of the window) and answered by `src/ui/callbacks.rs:1619`
//! (`Action::SetCustomStatus`, `ClearCustomStatus`, `CloseStatusEditor`).
//!
//! - **The dialog** ([`Status`], `vm.people.status`) is the layer
//!   `Layer::Status`: an emoji, words, when it clears. Its fields are the
//!   draft the person edits; the app keeps them as the intents come and
//!   fills them from the account's current status as the dialog opens.
//! - **The menu** ([`Menu`]) is the classic style's popup of the four
//!   statuses (`!ClientStyle.enabled`). The style the iced interface draws
//!   shows the account's own card instead (`account::Popover`, N06), so no
//!   layer opens it; it is here for the screens that may.

use iced::widget::image;

use crate::account::{Choice, Game};
use crate::kind::Expected;
use crate::menu::Row;
use crate::menu::Spot;

/// When the custom status clears itself, as `status-clear-after` numbers it
/// (0 to 4: never, 30 minutes, an hour, four hours, today).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClearAfter {
    #[default]
    Never,
    HalfAnHour,
    AnHour,
    FourHours,
    Today,
}

impl ClearAfter {
    pub const ALL: [Self; 5] = [
        Self::Never,
        Self::HalfAnHour,
        Self::AnHour,
        Self::FourHours,
        Self::Today,
    ];

    /// Its place among the options.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|one| *one == self).unwrap_or(0)
    }

    /// The option at `index`; none is "never" (`callbacks.rs:1633`).
    pub fn at(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// The minutes it waits, none for never; today is until midnight, which
    /// the core knows.
    pub fn minutes(self) -> Option<u32> {
        match self {
            Self::HalfAnHour => Some(30),
            Self::AnHour => Some(60),
            Self::FourHours => Some(240),
            Self::Never | Self::Today => None,
        }
    }
}

/// The emoji the status carries (`EmojiCell`, `ui/messages.slint:15`): one
/// of Unicode's, which Twemoji draws from its glyph, or a server's, which
/// shows its picture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Emoji {
    /// What the status stores: the emoji itself, or a server emoji's
    /// `name:id`.
    pub key: String,
    /// The Unicode emoji's text; empty for a server's.
    pub glyph: String,
    /// A server emoji's picture; the room is kept while it is on its way.
    pub picture: Expected,
    /// Its name, for the button's description ("Emoji, thumbs up").
    pub name: String,
}

impl Emoji {
    /// One of Unicode's, as the picker hands it over.
    pub fn unicode(glyph: &str, name: &str) -> Self {
        Self {
            key: glyph.into(),
            glyph: glyph.into(),
            picture: Expected::None,
            name: name.into(),
        }
    }

    /// A server's emoji `name:id`, with its picture.
    pub fn custom(key: &str, name: &str, picture: Option<image::Handle>) -> Self {
        Self {
            key: key.into(),
            glyph: String::new(),
            picture: picture.map_or(Expected::Waiting, Expected::Ready),
            name: name.into(),
        }
    }
}

/// The custom status dialog: what its screen draws. The window holds it as
/// `vm.people.status`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// The words in the field (`status-text`).
    pub text: String,
    /// The emoji chosen, none for no emoji (`status-emoji`).
    pub emoji: Option<Emoji>,
    pub clear_after: ClearAfter,
    /// A custom status is set: "Clear status" shows (`has-custom-status`).
    pub has_custom: bool,
    /// It is being saved or cleared: the buttons wait (`status-saving`).
    pub saving: bool,
    /// What went wrong, as the core wrote it; empty for nothing
    /// (`status-error`).
    pub error: String,
    /// The classic style's menu of statuses.
    pub menu: Menu,
}

impl Status {
    /// What Save asks for now.
    pub fn save(&self) -> Intent {
        Intent::Save {
            emoji: self.emoji.as_ref().map(|emoji| emoji.key.clone()),
            text: self.text.clone(),
            clear_after: self.clear_after,
        }
    }
}

/// The menu of statuses: which is chosen, the rows after them, and
/// the game being played (`StatusMenu`'s `status`, `has-custom-status`,
/// `game-name` and `game-duration`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {
    pub choice: Choice,
    /// The two ways on, under the statuses: the custom status ("Set" or
    /// "Change") and the profile editor, as `MenuItem`s.
    pub rows: Vec<Row<Intent>>,
    /// The game, with how long it has been going ("2 hours"); the duration
    /// stays empty until it is known, and the menu says "Playing now".
    pub game: Option<Game>,
}

/// What the dialog and the menu ask for. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The words changed: the app keeps them (`text <=> status-text`).
    Text(String),
    /// The emoji button: the emoji picker opens beside it
    /// (`pick-emoji-beside`); the app sets the emoji from what it picks.
    PickEmoji(Spot),
    /// The badge on the emoji's corner: no emoji
    /// (`status-emoji = { glyph: "" }`).
    RemoveEmoji,
    /// A "Clear after" option (`status-clear-after`).
    ClearAfter(ClearAfter),
    /// Save the status (`Action::SetCustomStatus`): the button and Enter in
    /// the field, never while one is saving.
    Save {
        emoji: Option<String>,
        text: String,
        clear_after: ClearAfter,
    },
    /// Clear the custom status (`Action::ClearCustomStatus`).
    Clear,
    /// The X and Cancel (`Action::CloseStatusEditor`); Escape and a press
    /// outside are the shell's.
    Close,
    /// The menu: a status chosen (`Action::SetStatus`).
    Choose(Choice),
    /// The menu: "Set a custom status" or "Change custom status"
    /// (`Action::OpenStatusEditor`).
    CustomStatus,
    /// The menu: "Edit Profile".
    EditProfile,
    /// The menu: the game's duration is due again, every 20 seconds while
    /// the menu shows and the window is in front, and as it comes back.
    GameDurationDue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_after_follows_the_numbers_slint_used() {
        assert_eq!(ClearAfter::at(2), ClearAfter::AnHour);
        assert_eq!(ClearAfter::at(9), ClearAfter::Never);
        assert_eq!(ClearAfter::Today.index(), 4);
        assert_eq!(ClearAfter::FourHours.minutes(), Some(240));
        assert_eq!(ClearAfter::Today.minutes(), None);
    }

    #[test]
    fn save_carries_the_draft() {
        let status = Status {
            text: "hi".into(),
            emoji: Some(Emoji::unicode("\u{1f9ea}", "Test tube")),
            clear_after: ClearAfter::AnHour,
            ..Status::default()
        };
        assert_eq!(
            status.save(),
            Intent::Save {
                emoji: Some("\u{1f9ea}".into()),
                text: "hi".into(),
                clear_after: ClearAfter::AnHour
            }
        );
    }
}
