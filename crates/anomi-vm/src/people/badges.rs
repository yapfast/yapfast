//! A profile's badges: what it shows and what it asks for. Task P08
//! (docs/iced/tasks/people.md); the profile card's view-model holds a
//! [`Badges`] (`ProfileData.badges`, `ui/people.slint:463`).
//!
//! A badge is its picture, with its description on hover and its page behind
//! a click when Discord gives it one. One that levels up (a Nitro or boost
//! tier, say) carries its [`Level`] too, and hovering shows a larger card
//! with the level's picture, the kind of badge and how far along the person
//! is instead of the plain description.

use crate::kind::Expected;

/// The level of a badge that levels up (`BadgeTip`, `ui/theme.slint:131`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Level {
    /// What it is a level of, as "Server Booster"; empty shows no line.
    pub kind: String,
    /// The level's name: the card writes it in capitals.
    pub title: String,
    /// How far along they are, as "Boosting since Jan 2024"; empty shows no
    /// line.
    pub progress: String,
    /// The level's own, larger picture.
    pub picture: Expected,
}

/// One badge (`ProfileBadgeRow`, `ui/people.slint:411`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Badge {
    /// Its picture, small.
    pub icon: Expected,
    /// What the badge is, said on hover when it has no level; also what a
    /// screen reader reads.
    pub description: String,
    /// The page a click opens; empty is no link.
    pub link: String,
    /// Its level, when it levels up.
    pub level: Option<Level>,
}

impl Badge {
    /// A click opens something.
    pub fn linked(&self) -> bool {
        !self.link.is_empty()
    }

    /// Hovering shows the level's card, not the plain description: Slint
    /// asked for a title.
    pub fn card(&self) -> Option<&Level> {
        self.level.as_ref().filter(|level| !level.title.is_empty())
    }
}

/// A profile's badges, in Discord's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Badges {
    pub badges: Vec<Badge>,
}

/// A profile's badges: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Open a badge's page in the browser.
    OpenLink(String),
}
