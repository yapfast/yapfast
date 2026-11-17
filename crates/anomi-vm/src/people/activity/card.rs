//! One of someone's activities as their profile's card shows it, and the
//! times it counts. The Slint side: `ActivityRow` (`ui/people.slint:364`),
//! `ActivityCard` (`:780`), filled by `src/ui/profile.rs`.

use iced::Color;

use crate::kind::{Expected, Status};
use crate::menu::Spot;

/// One of someone's activities.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    /// "PLAYING", "LISTENING TO SPOTIFY" and the like, as Discord writes it.
    pub heading: String,
    /// The game or app, or a Hang status's words after its emoji's picture
    /// (`doing` stays `Nothing`: the card has its own icons).
    pub name: Status,
    pub details: String,
    pub state: String,
    /// Its large picture (64 px) and small one (22 px, over the large's
    /// corner), and what they name.
    pub large: Expected,
    pub large_text: String,
    pub small: Expected,
    pub small_text: String,
    /// When it started and when it ends, in seconds as [`Card::clock`]'s
    /// `now` counts them (since 2020 began, `ActivityClock.now`); 0 when
    /// Discord didn't say.
    pub start: i64,
    pub end: i64,
    /// A stream's address: the card offers "Watch stream".
    pub url: String,
    /// What the title opens, such as a Spotify song's page.
    pub link: String,
    /// It is a game, whose privacy one's own menu manages.
    pub game: bool,
}

/// What a card's text and ground are, which a profile's theme sets; `None`
/// is the palette's own (text, secondary, muted, sidebar, raised).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    pub ink: Option<Color>,
    pub ink2: Option<Color>,
    pub ink3: Option<Color>,
    /// What the small picture's ring lies on.
    pub ground: Option<Color>,
    /// The unplayed part of a song's bar.
    pub track: Option<Color>,
}

impl Card {
    /// It has a start or an end to count from.
    pub fn timed(&self) -> bool {
        self.start > 0 || self.end > 0
    }

    /// A song, with its start and end, shows how far it has got on a bar.
    pub fn bar(&self) -> bool {
        self.start > 0 && self.end > self.start
    }

    /// How long the song is.
    pub fn length(&self) -> i64 {
        self.end - self.start
    }

    /// How far it has got at `now`, within the song.
    pub fn played(&self, now: i64) -> i64 {
        (now - self.start).clamp(0, self.length().max(0))
    }

    /// How far along the bar is, 0 to 1.
    pub fn progress(&self, now: i64) -> f32 {
        self.played(now) as f32 / self.length().max(1) as f32
    }

    /// Its time still changes at `now`: it has one, and a countdown that
    /// reached its end is not changing any more.
    pub fn counting(&self, now: i64) -> bool {
        self.timed()
            && !(self.end > 0 && now >= self.end && self.start == 0)
            && !(self.bar() && now >= self.end)
    }
}

/// Whether any of `cards` wants the clock each second, for the app's
/// subscription (only while the window is in front).
pub fn wants_clock(cards: &[Card], now: i64) -> bool {
    cards.iter().any(|card| card.counting(now))
}

/// `00:42`, `12:05` or `01:02:03`, padded as Discord's client writes them.
pub fn clock(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes, rest) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes:02}:{rest:02}")
    }
}

/// What a card asks for.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The title or "Watch stream" was clicked: open this address.
    Link(String),
    /// Its more button (only on one's own and others', not a bot's, never in
    /// a preview): where it is. The card is one's own game's when
    /// [`Card::game`] and the profile is one's own.
    More(Spot),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_are_padded_as_discord_writes_them() {
        assert_eq!(clock(42), "00:42");
        assert_eq!(clock(725), "12:05");
        assert_eq!(clock(3723), "01:02:03");
        assert_eq!(clock(-5), "00:00");
    }

    #[test]
    fn a_song_has_a_bar_and_a_game_has_not() {
        let song = Card {
            start: 1000,
            end: 1200,
            ..Card::default()
        };
        assert!(song.bar() && song.timed());
        assert_eq!(song.played(1050), 50);
        assert_eq!(song.played(900), 0);
        assert_eq!(song.played(5000), 200);
        assert!((song.progress(1100) - 0.5).abs() < 1e-6);
        let game = Card {
            start: 1000,
            ..Card::default()
        };
        assert!(!game.bar() && game.timed());
        assert!(!Card::default().timed());
    }

    #[test]
    fn only_cards_with_a_time_want_the_clock() {
        let game = Card {
            start: 10,
            ..Card::default()
        };
        assert!(wants_clock(&[Card::default(), game], 20));
        assert!(!wants_clock(&[Card::default()], 20));
        let song = Card {
            start: 10,
            end: 40,
            ..Card::default()
        };
        assert!(wants_clock(std::slice::from_ref(&song), 39));
        assert!(!wants_clock(&[song], 40));
    }
}
