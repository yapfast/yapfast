//! A call's tiles: what one tile shows, someone's, a stream's or this
//! account's shared screen, and what a click on it asks for. Task V03
//! (docs/iced/tasks/voice.md). The Slint side is `StageTile`, `TileChip`,
//! `LivePill`, `ViewerStack`, `StreamPicture` and `StreamPreview`
//! (`ui/voice.slint:751`, `:819`, `:41`, `:60`, `:840`, `:849`) as the three
//! loops of `ParticipantGrid` fill them (`:988`, `:1008`, `:1051`), and the
//! tiles' identities, `Focus` in `src/ui/tiles.rs`.
//!
//! The stage (V04) and picture in picture (V06) hold [`Tile`]s in the order
//! `src/ui/tiles.rs` gives them: this account's shared screen leads, then
//! the streams of those who film too, in tiles of their own, then everyone.
//! The projection decides what each shows; a tile draws what it's told. A
//! camera's or a stream's frames come through a [`Live`], which the app
//! puts each in as it comes and the tile reads as it draws: a frame builds
//! no view, and is drawn from the core's memory, never copied.

use iced::widget::image;

use crate::kind::{Avatar, Live};
use crate::menu::Spot;

/// Which tile it is, which a click on it shows large: `Focus` in
/// `src/ui/tiles.rs`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// This account's shared screen, which leads the tiles.
    OwnScreen,
    /// Someone's stream in a tile of its own, beside their camera's, by
    /// participant id.
    Stream(String),
    /// Someone's own tile, by participant id: their camera, their picture,
    /// or their stream while they share without a camera.
    Person(String),
}

impl Default for Key {
    fn default() -> Self {
        Self::Person(String::new())
    }
}

impl Key {
    /// The participant it's about, whose volume menu a right click opens;
    /// none for this account's screen.
    pub fn participant(&self) -> Option<&str> {
        match self {
            Self::OwnScreen => None,
            Self::Stream(id) | Self::Person(id) => Some(id),
        }
    }
}

/// What fills a tile. Their picture lies on the raised ground; everything
/// else on black, the stage's in every theme.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    /// Their picture in the middle, at half strength until they connect.
    Avatar { avatar: Avatar, connected: bool },
    /// Their camera's newest picture, whole.
    Camera(Live),
    /// A stream this account watches (`StreamPicture`, `:840`): its newest
    /// picture, whole, or until the first a turning ring over what it waits
    /// for.
    Watched { frame: Option<Live>, status: String },
    /// The preview Discord has of a stream not watched, filling the tile,
    /// dimmed (`StreamPreview`, `:849`). A participant's tile.
    Preview(image::Handle),
    /// A stream not watched, in a tile of its own: the screen icon over a
    /// "Watch stream" pill, on its preview when there is one (`:1033`).
    Offer { preview: Option<image::Handle> },
    /// This account's shared screen: its newest picture, whole, or until
    /// the first the screen icon over how it's going (`:999`).
    OwnScreen { frame: Option<Live>, status: String },
}

impl Default for Fill {
    fn default() -> Self {
        Self::Avatar {
            avatar: Avatar::default(),
            connected: true,
        }
    }
}

impl Fill {
    /// Laid on black rather than on the raised ground.
    pub fn on_black(&self) -> bool {
        !matches!(self, Self::Avatar { .. })
    }
}

/// A tile's outline, over its picture: a participant's has one (`:1091`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Outline {
    /// A stream's tile, or this account's screen.
    #[default]
    None,
    /// A thin line.
    Line,
    /// They speak: a thicker ring in the live colour.
    Speaking,
}

/// The icon before the name on a tile's chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Muted and not deafened: the struck microphone, in red.
    Muted,
    /// Deafened: the struck headphones, in red.
    Deafened,
    /// A stream's own tile: the screen, in white.
    Screen,
}

/// The name at a tile's bottom left (`TileChip`, `:819`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chip {
    pub name: String,
    pub mark: Option<Mark>,
    /// The LIVE tag after the name.
    pub live: bool,
    /// Why the tile may look wrong, such as a lost connection, in the
    /// accent after the name; empty for none.
    pub note: String,
}

/// Who watches the stream a tile shows (`ViewerStack`, `:60`): an eye and
/// how many, the first three's pictures, and how many more.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Viewers {
    /// The first [`Viewers::FACES`] viewers' pictures.
    pub faces: Vec<Avatar>,
    /// How many watch; none hides the stack.
    pub count: usize,
    /// "Watching: " and their names, for the tooltip.
    pub names: String,
}

impl Viewers {
    /// How many pictures the stack shows.
    pub const FACES: usize = 3;

    /// How many watch beyond the pictures shown.
    pub fn more(&self) -> usize {
        self.count.saturating_sub(Self::FACES)
    }
}

/// A call's tile: what its screen draws. The stage and picture in picture
/// hold these.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub key: Key,
    pub fill: Fill,
    pub outline: Outline,
    pub chip: Chip,
    /// Who watches the stream it shows: this account's screen's viewers,
    /// or a watched stream's.
    pub viewers: Viewers,
    /// A click watches the stream it offers, not watched yet. A
    /// participant's tile says so with a pill at its top right, a stream's
    /// own tile in its middle ([`Fill::Offer`]).
    pub watch: bool,
    /// This account's camera is on its way: a pill at its top left, beating
    /// while the window plays.
    pub starting: bool,
    /// Shown large: its stop button stays, and it has the full screen
    /// button and a double click.
    pub focused: bool,
    /// Over the whole window, alone and square.
    pub full: bool,
    /// Large, with other tiles beneath it, which its button hides.
    pub can_hide_others: bool,
    pub others_hidden: bool,
}

impl Tile {
    /// It shows a stream this account watches, which its top right stops.
    pub fn watched(&self) -> bool {
        matches!(self.fill, Fill::Watched { .. })
    }

    /// What a click on it asks: watching the stream it offers, else showing
    /// it large; nothing once it is (`:1022`, `:1071`).
    pub fn click(&self) -> Option<Intent> {
        match self.key.participant() {
            Some(id) if self.watch => Some(Intent::Watch(id.to_owned())),
            _ if !self.focused => Some(Intent::Focus(self.key.clone())),
            _ => None,
        }
    }
}

/// A call's tile: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Show this tile large (`focus-tile`).
    Focus(Key),
    /// Watch someone's stream, by participant id (`watch-stream`).
    Watch(String),
    /// Stop watching the stream watched (`stop-watching`).
    StopWatching,
    /// The large tile over the whole window, or back (`full-screen`).
    FullScreen(bool),
    /// Hide the tiles beneath the large one, or show them again
    /// (`toggle-others`).
    HideOthers(bool),
    /// Someone's volume menu at the pointer, by participant id
    /// (`open-volume`).
    Menu(String, Spot),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(key: Key) -> Tile {
        Tile {
            key,
            ..Tile::default()
        }
    }

    #[test]
    fn a_click_watches_what_it_offers_else_shows_it_large() {
        let bob = Key::Person("bob".into());
        let offered = Tile {
            watch: true,
            ..tile(bob.clone())
        };
        assert_eq!(offered.click(), Some(Intent::Watch("bob".into())));
        assert_eq!(tile(bob.clone()).click(), Some(Intent::Focus(bob.clone())));
        let large = Tile {
            focused: true,
            ..tile(bob)
        };
        assert_eq!(large.click(), None);
        assert_eq!(
            tile(Key::OwnScreen).click(),
            Some(Intent::Focus(Key::OwnScreen))
        );
    }

    #[test]
    fn the_stack_shows_three_and_counts_the_rest() {
        let viewers = Viewers {
            count: 5,
            ..Viewers::default()
        };
        assert_eq!(viewers.more(), 2);
        assert_eq!(Viewers::default().more(), 0);
    }

    #[test]
    fn only_a_picture_lies_on_the_raised_ground() {
        assert!(!Fill::default().on_black());
        assert!(Fill::Offer { preview: None }.on_black());
        assert_eq!(Key::OwnScreen.participant(), None);
        assert_eq!(Key::Stream("ana".into()).participant(), Some("ana"));
    }
}
