//! The GIF picker: what it shows and what it asks for. Task C18
//! (docs/iced/tasks/chat.md), porting `GifPicker` and `GifTile`
//! (`ui/gifs.slint:68`, `:11`) as `show_gifs` (`src/ui/gifs.rs`) fills them.
//!
//! Discord's picker over Klipy: a home of the Favorites tile, "Trending GIFs"
//! and the trending terms, the trending GIFs, the favourites, and what a
//! search found, in columns ([`masonry`]). The app lists the tiles of the
//! page shown, in order, with the pictures it has; the screen places them
//! for the width it shows at ([`GifPicker::placed`]) and builds only those
//! near its view. The app asks for the pictures of the tiles in sight and a
//! screen either side, at the size they show at, and lets go of the rest
//! ([`GifPicker::sight`]).

pub mod masonry;

use crate::kind::Expected;

pub use masonry::{GUTTER, Placed, Shape, Slot};

/// What the picker shows (`gif-view`, `GifView`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    /// The Favorites tile, "Trending GIFs" and the trending terms.
    #[default]
    Home,
    Trending,
    Favorites,
    /// What the search field found.
    Search,
}

/// What a tile is (`GifTile.kind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// A GIF: a click sends it, a star keeps it.
    #[default]
    Gif,
    /// The home's Favorites tile.
    Favorites,
    /// The home's "Trending GIFs" tile.
    Trending,
    /// A trending term, which a click searches for.
    Term,
    /// A grey tile while GIFs load.
    Placeholder,
}

/// A tile of the grid, listed by the app in the order Discord's masonry
/// places them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub kind: Kind,
    /// What a home tile says over its picture.
    pub label: String,
    /// A GIF's page, which a click sends and the star keeps it by; a term's
    /// name.
    pub url: String,
    /// The address its picture was asked by, which the app loads it for;
    /// empty for a tile with none.
    pub src: String,
    pub picture: Expected,
    pub shape: Shape,
    /// The GIF is a favourite.
    pub favorite: bool,
}

/// The GIF picker: what its screen draws. The window holds it as
/// `vm.chat.gif_picker`; the picker above the composer holds its own as
/// `vm.chat.pickers.gif`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GifPicker {
    pub page: Page,
    /// The search field's text: a search, or in Favorites a filter.
    pub query: String,
    pub tiles: Vec<Tile>,
    /// What the grid says in place of tiles, or under them: an error, no
    /// match, or how to favourite a GIF.
    pub note: String,
    /// Terms to try instead, below a search's grid.
    pub suggestions: Vec<String>,
    /// How far the grid is scrolled and how tall it shows, as it last said
    /// ([`Intent::Shown`]).
    pub shown: (f32, f32),
    /// Shift is held: a click puts the GIF in the message instead of
    /// sending it ([`Intent::Shift`]).
    pub shift: bool,
}

/// How tall the grid shows before it has said (the picker's height less its
/// tabs and search).
pub const SHOWN: f32 = 380.0;

/// The pictures asked for: the tiles in sight and a screen either side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sight {
    /// Each picture's address and the size to decode it at, in pixels.
    pub asked: Vec<(String, u32, u32)>,
    /// The grid is near its end: time for Discord's client's other terms
    /// (1180 px, `src/ui/gifs.rs`).
    pub near_end: bool,
}

/// The grid is near its end within this many pixels.
pub const NEAR_END: f32 = 1180.0;

impl GifPicker {
    /// The width the grid's columns fit, in a picker `panel` wide: it keeps
    /// 12 px each side (`gif-grid-width`).
    pub fn grid_width(panel: f32) -> f32 {
        (panel - 2.0 * GUTTER).max(0.0)
    }

    /// The tiles placed in a picker `panel` wide.
    pub fn placed(&self, panel: f32) -> Placed {
        masonry::place(
            self.tiles.iter().map(|tile| tile.shape),
            Self::grid_width(panel),
        )
    }

    /// The pictures to have for the grid as it's scrolled: those of tiles
    /// within a screen of the view, each once, at `scale` times the size
    /// they show at (`ask_gif_pictures`, `src/ui/gifs.rs:376`).
    pub fn sight(&self, panel: f32, scale: f32) -> Sight {
        let placed = self.placed(panel);
        let (top, height) = self.shown;
        let (from, to) = (top - height, top + 2.0 * height);
        let mut asked: Vec<(String, u32, u32)> = Vec::new();
        for (tile, slot) in self.tiles.iter().zip(&placed.slots) {
            if tile.src.is_empty()
                || slot.bottom() < from
                || slot.y > to
                || asked.iter().any(|(src, ..)| *src == tile.src)
            {
                continue;
            }
            let (width, height) = tile.shape.decoded(placed.column_width);
            asked.push((
                tile.src.clone(),
                (width as f32 * scale) as u32,
                (height as f32 * scale) as u32,
            ));
        }
        let bottom = placed.slots.iter().map(Slot::bottom).fold(0.0, f32::max);
        Sight {
            asked,
            near_end: top + height >= bottom - NEAR_END,
        }
    }
}

/// The GIF picker: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The field's text after an edit, outside Favorites: the app keeps it as
    /// [`GifPicker::query`] and searches once the typing has paused for a
    /// quarter second, as Discord's client does (`pause`, `ui/gifs.slint:101`).
    Edit(String),
    /// A search now (`search`): Enter, or each edit in Favorites, which
    /// filters at once.
    Search(String),
    /// The back button, or Escape away from the home: the query is cleared
    /// and the home shows (`back`).
    Back,
    /// The Favorites or "Trending GIFs" tile (`show-view`).
    Show(Page),
    /// A trending term, or a suggested one: search for it (`category`).
    Category(String),
    /// A GIF was clicked: send its page, or put it in the message with
    /// `insert`, Shift held (`pick`). The picker closes.
    Pick { url: String, insert: bool },
    /// A GIF's star: keep it among the favourites or let it go (`favorite`).
    Favorite(String),
    /// The grid scrolled or changed height (`scrolled-to`): only
    /// [`GifPicker::shown`] changes, and the app asks for the pictures
    /// [`GifPicker::sight`] lists.
    Shown { offset: f32, height: f32 },
    /// Shift was pressed or let go: [`GifPicker::shift`].
    Shift(bool),
    /// Ctrl+G: close the picker (`close`).
    Close,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gif(src: &str, ratio: f32) -> Tile {
        Tile {
            src: src.into(),
            shape: Shape::Picture(ratio),
            ..Tile::default()
        }
    }

    #[test]
    fn the_grid_keeps_its_gutters() {
        assert_eq!(GifPicker::grid_width(498.0), 474.0);
    }

    #[test]
    fn only_the_tiles_near_the_view_are_asked_for_once_each() {
        let mut tiles: Vec<Tile> = (0..40).map(|n| gif(&format!("g{n}"), 1.0)).collect();
        tiles.push(gif("g0", 1.0));
        tiles.push(Tile::default());
        let picker = GifPicker {
            tiles,
            shown: (0.0, 380.0),
            ..GifPicker::default()
        };
        let sight = picker.sight(498.0, 2.0);
        // Two columns of 231 px tiles 243 px apart: a screen and two below
        // the view reach 760 px, the rows through the fourth.
        assert_eq!(sight.asked.len(), 8);
        assert_eq!(sight.asked[0], ("g0".into(), 462, 462));
        assert!(!sight.near_end);
        let far = GifPicker {
            shown: (4000.0, 380.0),
            ..picker
        };
        assert!(far.sight(498.0, 1.0).near_end);
    }

    #[test]
    fn a_home_tile_crops_its_picture_to_the_column() {
        let tile = Tile {
            src: "t".into(),
            shape: Shape::Fixed(110.0),
            ..Tile::default()
        };
        let picker = GifPicker {
            tiles: vec![tile],
            shown: (0.0, 380.0),
            ..GifPicker::default()
        };
        assert_eq!(picker.sight(498.0, 1.0).asked, vec![("t".into(), 231, 231)]);
    }
}
