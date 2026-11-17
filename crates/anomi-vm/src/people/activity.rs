//! The Activity panel above the member list and the activity cards of a
//! profile: what they show and what they ask for. The Slint side: the panel
//! of `MemberPanel` (`ui/people.slint:97`, its `activity-*` properties) with
//! `MemberActivityRow` (`:86`), filled by `activity_panel` in
//! `src/ui/profile.rs:53`; the cards are [`card::Card`], `ActivityRow`
//! (`:364`). Task P06 (docs/iced/tasks/people.md).

pub mod card;

use iced::Rectangle;

use crate::kind::{Avatar, Doing, Expected};
use crate::menu::Spot;

/// The cards the panel shows before it is opened.
pub const SHOWN: usize = 3;

/// A card of the panel: what someone is doing or did lately, from Discord's
/// activity feed.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Whose it is: the profile a click opens.
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    /// Which icon the time wears: a game, a song, a show, a stream, a top.
    pub doing: Doing,
    /// Going on now, which its icon and time show in green.
    pub live: bool,
    /// The game, artist or show.
    pub game: String,
    /// The words about it: "42 min", "3 h ago".
    pub since: String,
    /// Its picture, 48 px.
    pub thumb: Expected,
}

/// The panel above the member list. The window holds it as
/// `vm.people.activity`; with no rows it draws nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Activity {
    /// In Discord's order.
    pub rows: Vec<Row>,
    /// Every card shows, not only the first [`SHOWN`].
    pub open: bool,
    /// The header shows alone (the setting the eye button flips).
    pub hidden: bool,
}

impl Activity {
    /// The header opens or closes the rest of the cards: when there are
    /// more than [`SHOWN`], or to bring back cards that were hidden.
    pub fn opens(&self) -> bool {
        self.rows.len() > SHOWN || self.hidden
    }

    /// The cards on screen.
    pub fn showing(&self) -> usize {
        if self.hidden {
            0
        } else if self.open {
            self.rows.len()
        } else {
            self.rows.len().min(SHOWN)
        }
    }

    /// The panel wants its times brought up to date once a minute: while it
    /// has cards, and the app asks only while the window is in front.
    pub fn wants_clock(&self) -> bool {
        !self.rows.is_empty()
    }
}

/// What a click or a key on the panel asks for. Keep `#[non_exhaustive]`:
/// the app matches it with a wildcard until its I task handles every
/// variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The header was clicked while it [`opens`](Activity::opens): show or
    /// fold the cards past the first three. The app also brings back hidden
    /// cards (`Hide(false)` comes instead then).
    Expand(bool),
    /// The eye button: hide the cards or show them (a setting the app keeps).
    Hide(bool),
    /// A card was clicked: whose profile, and where the card is.
    Profile(String, Rectangle),
    /// A card was right-clicked: whose member menu, and where.
    Menu(String, Spot),
    /// A card of a profile asked something (see [`card::Intent`]).
    Card(card::Intent),
}

impl From<card::Intent> for Intent {
    fn from(intent: card::Intent) -> Self {
        Self::Card(intent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(count: usize) -> Activity {
        Activity {
            rows: (0..count)
                .map(|n| Row {
                    id: n.to_string(),
                    name: String::new(),
                    avatar: Avatar {
                        initial: String::new(),
                        tint: iced::Color::BLACK,
                        picture: None,
                    },
                    doing: Doing::Playing,
                    live: true,
                    game: String::new(),
                    since: String::new(),
                    thumb: Expected::None,
                })
                .collect(),
            ..Activity::default()
        }
    }

    #[test]
    fn three_show_until_it_is_opened() {
        let mut activity = rows(5);
        assert_eq!((activity.showing(), activity.opens()), (3, true));
        activity.open = true;
        assert_eq!(activity.showing(), 5);
        activity.hidden = true;
        assert_eq!(activity.showing(), 0);
        assert!(rows(2).showing() == 2 && !rows(2).opens());
    }

    #[test]
    fn hidden_cards_can_be_brought_back_by_the_header() {
        let mut activity = rows(2);
        activity.hidden = true;
        assert!(activity.opens());
    }

    #[test]
    fn no_cards_no_clock() {
        assert!(!Activity::default().wants_clock());
        assert!(rows(1).wants_clock());
    }
}
