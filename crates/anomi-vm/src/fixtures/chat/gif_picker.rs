//! Samples of the GIF picker for tests and shots, after the fake Discord's
//! Klipy: its trending terms (happy, dance, wow, sad, cat, wave) and its GIFs
//! of a word with a bouncing dot. Task C18's. The golden shot is
//! `picker/gif` (the home).
//!
//! The GIFs move in the app; only the widgets can make a moving picture, so
//! the samples take their picture from a [`Moving`], and the shots hand in
//! the bouncing one. The home's tiles are stills.

use iced::widget::image;
use yapfast_lang::tr;

use crate::chat::gif_picker::{GifPicker, Kind, Page, Shape, Tile};
use crate::kind::Expected;

use super::super::pictures;

/// A GIF's picture, by its address.
pub type Moving = fn(&str) -> image::Handle;

/// The fake's trending terms.
pub const TERMS: [&str; 6] = ["happy", "dance", "wow", "sad", "cat", "wave"];

/// The fake's still for a term or a GIF.
pub fn still(src: &str) -> image::Handle {
    pictures::picture(src)
}

fn term_tile(name: &str) -> Tile {
    let src = format!("/gifs/{name}.png");
    Tile {
        kind: Kind::Term,
        label: name.into(),
        url: name.into(),
        picture: Expected::Ready(still(&src)),
        src,
        shape: Shape::Fixed(110.0),
        ..Tile::default()
    }
}

/// The home: the Favorites tile, "Trending GIFs" and the terms.
pub fn home() -> GifPicker {
    let mut tiles = vec![
        Tile {
            kind: Kind::Favorites,
            label: tr!("Favorites").into(),
            shape: Shape::Fixed(110.0),
            picture: Expected::Ready(still("/gifs/favorites.png")),
            src: "/gifs/favorites.png".into(),
            ..Tile::default()
        },
        Tile {
            kind: Kind::Trending,
            label: tr!("Trending GIFs").into(),
            shape: Shape::Fixed(110.0),
            picture: Expected::Ready(still("/gifs/trending.png")),
            src: "/gifs/trending.png".into(),
            ..Tile::default()
        },
    ];
    tiles.extend(TERMS.iter().map(|name| term_tile(name)));
    GifPicker {
        tiles,
        ..GifPicker::default()
    }
}

/// The home while its tiles load.
pub fn home_loading() -> GifPicker {
    GifPicker {
        tiles: (0..8)
            .map(|_| Tile {
                kind: Kind::Placeholder,
                shape: Shape::Fixed(110.0),
                ..Tile::default()
            })
            .collect(),
        ..GifPicker::default()
    }
}

/// The home when Klipy couldn't be reached: the Favorites and Trending tiles
/// and the core's message (`src/ui/gifs.rs:240`).
pub fn home_error() -> GifPicker {
    let mut tiles = home().tiles;
    tiles.truncate(2);
    GifPicker {
        tiles,
        note: tr!("Couldn't load GIFs: {}", "error sending request"),
        ..GifPicker::default()
    }
}

/// A GIF of the fake's: its page, picture and size.
fn gif(index: usize, favorite: bool, moving: Moving) -> Tile {
    // Wide, square and tall ones, the same each time.
    const SIZES: [(u32, u32); 5] = [(220, 124), (200, 200), (180, 260), (240, 135), (200, 150)];
    let (width, height) = SIZES[index % SIZES.len()];
    let src = format!("/gifs/found{index}.gif");
    Tile {
        kind: Kind::Gif,
        url: format!("https://klipy.example/gif-{index}"),
        picture: Expected::Ready(moving(&src)),
        src,
        shape: Shape::of(width, height),
        favorite,
        ..Tile::default()
    }
}

fn gifs(count: usize, favorites: &[usize], moving: Moving) -> Vec<Tile> {
    (0..count)
        .map(|index| gif(index, favorites.contains(&index), moving))
        .collect()
}

/// What a search for `query` found, the third a favourite.
pub fn search_with(query: &str, moving: Moving) -> GifPicker {
    GifPicker {
        page: Page::Search,
        query: query.into(),
        tiles: gifs(12, &[2, 5], moving),
        ..GifPicker::default()
    }
}

/// The trending GIFs.
pub fn trending_with(moving: Moving) -> GifPicker {
    GifPicker {
        page: Page::Trending,
        tiles: gifs(12, &[], moving),
        ..GifPicker::default()
    }
}

/// The favourites, each starred.
pub fn favorites_with(moving: Moving) -> GifPicker {
    GifPicker {
        page: Page::Favorites,
        tiles: gifs(5, &[0, 1, 2, 3, 4], moving)
            .into_iter()
            .map(|tile| Tile {
                favorite: true,
                ..tile
            })
            .collect(),
        ..GifPicker::default()
    }
}

/// No favourites yet: how to make one.
pub fn favorites_empty() -> GifPicker {
    GifPicker {
        page: Page::Favorites,
        note: tr!(
            "Click the star in the corner of a GIF to favorite it. Favorites will show up here!"
        )
        .into(),
        ..GifPicker::default()
    }
}

/// A search that is loading: grey tiles of varying heights.
pub fn searching(query: &str) -> GifPicker {
    GifPicker {
        page: Page::Search,
        query: query.into(),
        tiles: (0..16)
            .map(|index| Tile {
                kind: Kind::Placeholder,
                shape: Shape::Fixed(120.0 + ((index * 37 + 11) % 100) as f32),
                ..Tile::default()
            })
            .collect(),
        ..GifPicker::default()
    }
}

/// A search that found nothing, with terms to try instead.
pub fn nothing(query: &str) -> GifPicker {
    GifPicker {
        page: Page::Search,
        query: query.into(),
        note: tr!("No GIFs match your search.").into(),
        suggestions: TERMS.iter().take(4).map(|term| (*term).into()).collect(),
        ..GifPicker::default()
    }
}
