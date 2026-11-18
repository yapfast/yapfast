//! The GIF picker from the core's state (`show_gifs`, `src/ui/gifs.rs:151`):
//! the home's Favorites and "Trending GIFs" tiles and the trending terms,
//! the GIFs trending or found, or the favourites, in the order the masonry
//! places them, with grey tiles while they load; and what the grid says in
//! their place. The pictures are the core's, which holds those of the tiles
//! in sight alone, as it's told them (`GifsShown`): one that comes or goes
//! changes its tile's picture, and nothing is listed again.

use std::hash::{DefaultHasher, Hash, Hasher};

use yapfast_core::application::{AppState, GifPicture, GifView};
use yapfast_lang::tr;

use anomi_vm::chat::gif_picker::{GifPicker, Kind, Page, Shape, Sight, Tile};
use anomi_vm::kind::Expected;

use crate::pictures;

/// A home tile's height (`HOME_TILE`, `src/ui/gifs.rs:15`), and how many
/// grey tiles show while GIFs load (`PLACEHOLDERS`).
const HOME_TILE: f32 = 110.0;
const PLACEHOLDERS: usize = 16;

/// What the GIFs' tiles were made from.
#[derive(Default)]
pub(crate) struct Shown {
    key: Option<u64>,
    /// The core's search as last shown: the field takes it only as it
    /// changes, so what is being typed stays.
    query: Option<String>,
    page: Option<Page>,
    /// The tiles were listed again: their pictures in sight are asked for
    /// once the update is over.
    pub(crate) ask: bool,
    /// What was asked last.
    pub(crate) asked: Option<Sight>,
}

fn page(view: GifView) -> Page {
    match view {
        GifView::Home => Page::Home,
        GifView::Trending => Page::Trending,
        GifView::Favorites => Page::Favorites,
        GifView::Search => Page::Search,
    }
}

/// What the core holds that the tiles show, but their pictures.
fn key(state: &AppState) -> u64 {
    let gifs = &state.gifs;
    let mut hasher = DefaultHasher::new();
    (gifs.view as u8, &gifs.query, gifs.loading, &gifs.error).hash(&mut hasher);
    (gifs.home_loading, &gifs.home_error, &gifs.suggestions).hash(&mut hasher);
    for category in &gifs.categories {
        (&category.name, &category.src).hash(&mut hasher);
    }
    gifs.trending_preview
        .as_ref()
        .map(|gif| &gif.src)
        .hash(&mut hasher);
    for gif in &gifs.results {
        (&gif.url, &gif.src, gif.width, gif.height).hash(&mut hasher);
    }
    let favorites = state.favorite_gifs.as_ref().map(Vec::len);
    (favorites, state.favorite_gifs_changed).hash(&mut hasher);
    hasher.finish()
}

/// The picture at `src` as the core holds it for the picker.
fn picture(state: &AppState, src: &str) -> Expected {
    if src.is_empty() {
        return Expected::None;
    }
    match state.gifs.pictures.get(src).map(|asked| &asked.picture) {
        Some(GifPicture::Shown(picture)) => {
            pictures::shown(picture).map_or(Expected::Waiting, Expected::Ready)
        }
        _ => Expected::Waiting,
    }
}

/// Shows the GIFs the core lists, or only the pictures that came or went
/// when it lists the same.
pub(crate) fn show(state: &AppState, picker: &mut GifPicker, shown: &mut Shown) {
    let gifs = &state.gifs;
    if !gifs.open {
        return;
    }
    let now = page(gifs.view);
    if shown.page != Some(now) {
        // Another page starts at its top: the old one's scroll would draw
        // the new tiles empty for a frame.
        shown.page = Some(now);
        picker.page = now;
        picker.shown = (0.0, 0.0);
    }
    if shown.query.as_deref() != Some(gifs.query.as_str()) {
        shown.query = Some(gifs.query.clone());
        picker.query.clone_from(&gifs.query);
    }
    let key = key(state);
    if shown.key == Some(key) {
        for tile in &mut picker.tiles {
            tile.picture = picture(state, &tile.src);
        }
        return;
    }
    shown.key = Some(key);
    shown.ask = true;
    let (tiles, note) = tiles(state);
    picker.tiles = tiles;
    picker.note = note;
    picker.suggestions = match gifs.view {
        GifView::Search => gifs.suggestions.clone(),
        _ => Vec::new(),
    };
}

/// The shapes of `count` grey tiles: heights that vary, the same each time.
fn placeholders(count: usize) -> impl Iterator<Item = Shape> {
    (0..count).map(|index| Shape::Fixed(120.0 + ((index * 37 + 11) % 100) as f32))
}

/// The tiles of the page shown, and what the grid says in their place.
fn tiles(state: &AppState) -> (Vec<Tile>, String) {
    let gifs = &state.gifs;
    let mut tiles = Vec::new();
    let mut note = String::new();
    let mut tile = |kind: Kind, label: &str, url: &str, src: &str, shape: Shape, favorite: bool| {
        tiles.push(Tile {
            kind,
            label: label.to_owned(),
            url: url.to_owned(),
            src: src.to_owned(),
            picture: picture(state, src),
            shape,
            favorite,
        });
    };
    let home = Shape::Fixed(HOME_TILE);
    match gifs.view {
        GifView::Home if gifs.categories.is_empty() && gifs.home_loading => {
            for _ in 0..8 {
                tile(Kind::Placeholder, "", "", "", home, false);
            }
        }
        GifView::Home => {
            let favorites = state.shown_favorite_gifs();
            let newest = favorites
                .first()
                .map_or("", |favorite| favorite.src.as_str());
            tile(Kind::Favorites, tr!("Favorites"), "", newest, home, false);
            let trending = gifs
                .trending_preview
                .as_ref()
                .map_or("", |gif| gif.src.as_str());
            tile(
                Kind::Trending,
                tr!("Trending GIFs"),
                "",
                trending,
                home,
                false,
            );
            for category in &gifs.categories {
                let name = category.name.as_str();
                tile(Kind::Term, name, name, &category.src, home, false);
            }
            if let Some(error) = &gifs.home_error {
                note.clone_from(error);
            }
        }
        GifView::Trending | GifView::Search => {
            if gifs.loading {
                for shape in placeholders(PLACEHOLDERS) {
                    tile(Kind::Placeholder, "", "", "", shape, false);
                }
            } else if let Some(error) = &gifs.error {
                note.clone_from(error);
            } else if gifs.results.is_empty() {
                note = tr!("No GIFs match your search.").into();
            }
            for gif in gifs.results.iter().filter(|_| !gifs.loading) {
                let favorite = state.is_favorite_gif(&gif.url);
                let shape = Shape::of(gif.width, gif.height);
                tile(Kind::Gif, "", &gif.url, &gif.src, shape, favorite);
            }
        }
        GifView::Favorites => match &state.favorite_gifs {
            None => {
                for shape in placeholders(PLACEHOLDERS / 2) {
                    tile(Kind::Placeholder, "", "", "", shape, false);
                }
            }
            Some(all) => {
                let shown = state.shown_favorite_gifs();
                if all.is_empty() {
                    note = tr!(
                        "Click the star in the corner of a GIF to favorite it. Favorites will show up here!"
                    )
                    .into();
                } else if shown.is_empty() {
                    note = tr!("No favorite GIFs match your search.").into();
                }
                for favorite in shown {
                    let shape = Shape::of(favorite.width, favorite.height);
                    tile(Kind::Gif, "", &favorite.url, &favorite.src, shape, true);
                }
            }
        },
    }
    (tiles, note)
}
