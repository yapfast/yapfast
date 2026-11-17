//! The participant grid: what it shows and what it asks for. Task V04
//! (docs/iced/tasks/voice.md). The Slint side is `ParticipantGrid`
//! (`ui/voice.slint:857`), filled by `show_stage` (`src/ui/refresh.rs:4541`)
//! and answered at `src/ui/callbacks.rs:3759` (`call-tiles`) and `:3789`
//! (`focus-tile`); which tile is which, and which shows large, is `Stage`
//! and `Focus` in `src/ui/tiles.rs`.
//!
//! The stage holds the call's tiles in the order of `src/ui/tiles.rs`: this
//! account's shared screen leads, then the streams of those who film too,
//! in tiles of their own, then everyone; a tile's place in it is its slot.
//! The grid lays them out itself from the room it gets, as Slint's does,
//! and tells the app where they are ([`Placement`]) so cameras are decoded
//! only for tiles in sight, at their size. Which tile shows large, over the
//! whole window or not, and whether the others beneath it are hidden, the
//! tiles say themselves ([`Stage::new`] sets it on them); the stage reads
//! it back from them, so the two can't disagree.
//!
//! `src/ui/alone.rs` (leaving a DM call nobody else is in) draws nothing:
//! it is the app's timer, I18's, and the line over the stage that says so
//! is the conversation's (`Conversation::description`).

use crate::voice::tiles::Tile;

/// The participant grid: what its screen draws. The window holds it as
/// `vm.voice.stage`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stage {
    /// The call's tiles, in slot order (see the module's doc).
    pub tiles: Vec<Tile>,
}

impl Stage {
    /// The stage of `tiles`, the one at slot `focused` shown large, over the
    /// whole window while `full`, the others beneath it hidden while
    /// `others_hidden`. Each tile gets these as `ParticipantGrid` hands them
    /// to its `StageTile`s (`:993`, `:1016`, `:1057`): `focused` for the
    /// large one, `full` for all, and whether there are others to hide. A
    /// slot past the tiles focuses none; hiding the others lasts only while
    /// one is large (`changed focusing`, `:907`).
    pub fn new(
        mut tiles: Vec<Tile>,
        focused: Option<usize>,
        full: bool,
        others_hidden: bool,
    ) -> Self {
        let focused = focused.filter(|&slot| slot < tiles.len());
        let others = tiles.len() > 1;
        for (slot, tile) in tiles.iter_mut().enumerate() {
            tile.focused = focused == Some(slot);
            tile.full = full && focused.is_some();
            tile.can_hide_others = others;
            tile.others_hidden = others_hidden && focused.is_some();
        }
        Self { tiles }
    }

    /// The slot of the tile shown large, if any (`focused`, `:881`).
    pub fn focused(&self) -> Option<usize> {
        self.tiles.iter().position(|tile| tile.focused)
    }

    /// The large tile is over the whole window, alone and edge to edge
    /// (`full`, `:883`).
    pub fn full(&self) -> bool {
        self.large().is_some_and(|tile| tile.full)
    }

    /// The tiles beneath the large one are hidden, from its button
    /// (`others-hidden`, `:940`).
    pub fn others_hidden(&self) -> bool {
        self.large().is_some_and(|tile| tile.others_hidden)
    }

    fn large(&self) -> Option<&Tile> {
        self.tiles.iter().find(|tile| tile.focused)
    }
}

/// Where the grid's tiles are, in logical pixels, as `ParticipantGrid`
/// reports it (`tiles`, `:895`): `Tiles` in `src/ui/tiles.rs`. The app
/// decodes a camera only for a tile in sight, at the size it shows at.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    /// How many tiles a row holds; none while the large tile is alone.
    pub columns: u32,
    /// A tile's size in the rows.
    pub width: f32,
    pub height: f32,
    pub gap: f32,
    /// The first row's top, and how far the grid is scrolled down.
    pub top: f32,
    pub scrolled: f32,
    /// The grid's height on screen.
    pub shown: f32,
    /// The slot of the tile shown large, and its size; the rows hold the
    /// others.
    pub focused: Option<usize>,
    pub focus_width: f32,
    pub focus_height: f32,
}

impl Placement {
    /// The size in pixels the tile at slot `index` shows at, if any of it
    /// is in sight, at `scale` pixels a logical one (`Tiles::shown_at`).
    pub fn shown_at(&self, index: usize, scale: f32) -> Option<(u32, u32)> {
        let pixels = |side: f32| ((side * scale).round() as u32).max(2);
        let index = match self.focused {
            Some(focused) if index == focused => {
                return (self.focus_width > 0.0 && self.focus_height > 0.0)
                    .then(|| (pixels(self.focus_width), pixels(self.focus_height)));
            }
            Some(focused) if index > focused => index - 1,
            _ => index,
        };
        if self.columns == 0 || self.width <= 0.0 || self.height <= 0.0 {
            return None;
        }
        let row = (index / self.columns as usize) as f32;
        let top = self.top + row * (self.height + self.gap) - self.scrolled;
        let in_sight = top < self.shown && top + self.height > 0.0;
        in_sight.then(|| (pixels(self.width), pixels(self.height)))
    }
}

/// The participant grid: what a click on it, or its layout, asks for. A
/// tile's own clicks are `tiles::Intent`. Keep `#[non_exhaustive]`: the
/// app matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// A click beside the large tile shows the grid again (`focus-tile(-1)`,
    /// `:986`). Hiding the others ends with it.
    ShowAll,
    /// Where the tiles are now, sent as it changes: laid out, scrolled,
    /// resized (`tiles`, `:895`; `src/ui/callbacks.rs:3759`). The app keeps
    /// it and places the cameras; it changes nothing the grid draws.
    Placed(Placement),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::tiles::Key;

    fn tiles(count: usize) -> Vec<Tile> {
        (0..count)
            .map(|index| Tile {
                key: Key::Person(index.to_string()),
                ..Tile::default()
            })
            .collect()
    }

    #[test]
    fn the_tiles_say_which_is_large() {
        let stage = Stage::new(tiles(3), Some(1), false, true);
        assert_eq!(stage.focused(), Some(1));
        assert!(stage.others_hidden() && !stage.full());
        assert!(stage.tiles.iter().all(|tile| tile.can_hide_others));
        assert_eq!(
            stage.tiles.iter().filter(|tile| tile.focused).count(),
            1,
            "one large tile"
        );
        let full = Stage::new(tiles(3), Some(2), true, false);
        assert!(full.full() && full.tiles[2].full);
        let grid = Stage::new(tiles(3), None, true, true);
        assert_eq!(grid.focused(), None);
        assert!(!grid.full() && !grid.others_hidden(), "only with one large");
        assert_eq!(Stage::new(tiles(2), Some(2), false, false).focused(), None);
        assert!(!Stage::new(tiles(1), Some(0), false, false).tiles[0].can_hide_others);
    }

    #[test]
    fn only_tiles_in_sight_are_shown() {
        let tiles = Placement {
            columns: 2,
            width: 300.0,
            height: 168.75,
            gap: 12.0,
            top: 24.0,
            scrolled: 0.0,
            shown: 380.0,
            ..Placement::default()
        };
        assert_eq!(tiles.shown_at(0, 2.0), Some((600, 338)));
        assert!(tiles.shown_at(3, 1.0).is_some(), "the second row shows");
        assert_eq!(tiles.shown_at(4, 1.0), None, "the third starts below");
        let scrolled = Placement {
            scrolled: 220.0,
            ..tiles
        };
        assert_eq!(scrolled.shown_at(0, 1.0), None, "scrolled away");
        assert!(scrolled.shown_at(4, 1.0).is_some());
        assert_eq!(Placement::default().shown_at(0, 1.0), None);
    }

    #[test]
    fn the_focused_tile_shows_large_and_the_others_in_its_rows() {
        let tiles = Placement {
            columns: 3,
            width: 192.0,
            height: 108.0,
            gap: 12.0,
            top: 500.0,
            scrolled: 0.0,
            shown: 600.0,
            focused: Some(1),
            focus_width: 800.0,
            focus_height: 450.0,
        };
        assert_eq!(tiles.shown_at(1, 1.0), Some((800, 450)));
        assert_eq!(tiles.shown_at(0, 1.0), Some((192, 108)));
        assert_eq!(tiles.shown_at(3, 1.0), Some((192, 108)), "third in the row");
        assert_eq!(tiles.shown_at(4, 1.0), None, "the second row is below");
        let alone = Placement {
            columns: 0,
            ..tiles
        };
        assert_eq!(alone.shown_at(1, 1.0), Some((800, 450)));
        assert_eq!(alone.shown_at(0, 1.0), None, "full screen shows one");
    }
}
