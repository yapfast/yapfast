//! The GIF picker's grid: its tiles placed in columns as Discord's masonry
//! places them, and the pictures loaded only for those in sight.

use yapfast_core::application::{GifPicture, GifView};

use super::*;

/// The picker starts 498px wide; the grid, that less 12px each side, in
/// columns about 200px wide, 12px apart, as Discord's. Dragging its left edge
/// wider fits more columns.
const GUTTER: f32 = 12.0;
const DESIRED_WIDTH: f32 = 200.0;
const MAX_COLUMNS: f32 = 8.0;
/// The height of the home's tiles.
const HOME_TILE: f32 = 110.0;
/// How many placeholders a search shows while it loads.
const PLACEHOLDERS: usize = 16;

/// What the grid shows, for loading the pictures in sight.
#[derive(Default)]
pub(super) struct Shown {
    /// Each tile's picture, where it is and the size to decode it at.
    placed: Vec<Placed>,
    /// How far the grid is scrolled and how tall it shows, as it last said.
    viewport: (f32, f32),
    /// The pictures last asked for, with their sizes, and whether the grid
    /// was near its end.
    asked: Vec<(String, u32, u32)>,
    near_end: bool,
    /// The search text the field was last given.
    query: Option<String>,
    /// What the grid says in place of tiles.
    note: String,
    /// The change to the favourites the stars last took.
    favorites_seen: u32,
    /// Asks for the pictures at their new sizes once the picker's width has
    /// held still a moment, rather than at each step of a drag.
    resized: slint::Timer,
}

#[derive(PartialEq)]
struct Placed {
    src: String,
    shape: Shape,
    top: f32,
    bottom: f32,
    width: u32,
    height: u32,
}

/// How tall a tile is in a column of a given width.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// A GIF's: its picture's height over its width.
    Picture(f32),
    /// The home's and placeholders': this tall, the picture cropped to the
    /// column's width.
    Fixed(f32),
}

impl Shape {
    /// A GIF's picture, `width` by `height`; square when unknown.
    fn of(width: u32, height: u32) -> Self {
        if width == 0 || height == 0 {
            Self::Picture(1.0)
        } else {
            Self::Picture(height as f32 / width as f32)
        }
    }

    /// Its height in a column `width` wide.
    fn height(self, width: f32) -> f32 {
        match self {
            Self::Picture(ratio) => width * ratio,
            Self::Fixed(height) => height,
        }
    }

    /// The size to decode its picture at, in a column `width` wide: a home
    /// tile crops a picture to its width, and a placeholder has none.
    fn decoded(self, width: f32) -> (u32, u32) {
        match self {
            Self::Picture(_) => (width as u32, self.height(width) as u32),
            Self::Fixed(height) => (width as u32, height.max(width) as u32),
        }
    }
}

/// Places tiles in the shortest column, leftmost on ties.
struct Masonry {
    columns: Vec<f32>,
    width: f32,
}

impl Masonry {
    /// How many columns a grid `width` wide holds, about 200px each.
    fn column_count(width: f32) -> f32 {
        (width / DESIRED_WIDTH).floor().clamp(1.0, MAX_COLUMNS)
    }

    /// How wide each of those columns is.
    fn column_width(width: f32) -> f32 {
        let count = Self::column_count(width);
        (width - GUTTER * (count - 1.0)) / count
    }

    fn new(width: f32) -> Self {
        let count = Self::column_count(width);
        Self {
            columns: vec![GUTTER; count as usize],
            width: Self::column_width(width),
        }
    }

    /// Where a tile `height` tall goes: its left and top.
    fn place(&mut self, height: f32) -> (f32, f32) {
        let (column, top) = self
            .columns
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(column, top)| (column, *top))
            .unwrap_or((0, GUTTER));
        self.columns[column] = top + height + GUTTER;
        (GUTTER + column as f32 * (self.width + GUTTER), top)
    }

    fn bottom(&self) -> f32 {
        self.columns.iter().copied().fold(0.0, f32::max)
    }
}

/// Grounds for tiles whose picture isn't in yet, in turn.
const GROUNDS: [slint::Color; 3] = [
    slint::Color::from_rgb_u8(0x3b, 0x3f, 0x63),
    slint::Color::from_rgb_u8(0x4a, 0x3b, 0x63),
    slint::Color::from_rgb_u8(0x3b, 0x52, 0x63),
];

/// Adds a tile: its kind, label, page, picture, shape and whether it's a
/// favourite.
type AddTile<'a> = dyn FnMut(i32, &str, &str, &str, Shape, bool) + 'a;

thread_local! {
    /// The favourite GIFs' pages, which the stars on messages' GIFs ask
    /// for as they show.
    static FAVORITES: RefCell<HashSet<String>> = RefCell::default();
}

impl Adapter {
    pub(super) fn show_gifs(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        window.set_can_pick_gifs(state.can_pick_gifs());
        if self.gifs.favorites_seen != state.favorite_gifs_changed {
            self.gifs.favorites_seen = state.favorite_gifs_changed;
            FAVORITES.with_borrow_mut(|set| {
                set.clear();
                set.extend(state.favorite_gifs.iter().flatten().map(|f| f.url.clone()));
            });
            let global = window.global::<GifFavorites>();
            global.set_revision(global.get_revision().wrapping_add(1));
        }
        let picker = &state.gifs;
        if !picker.open {
            if !self.gifs.placed.is_empty() || self.models.gif_tiles.row_count() > 0 {
                self.gifs = Shown {
                    favorites_seen: self.gifs.favorites_seen,
                    ..Shown::default()
                };
                update_model(&self.models.gif_tiles, vec![]);
            }
            return;
        }
        window.set_gif_view(match picker.view {
            GifView::Home => 0,
            GifView::Trending => 1,
            GifView::Favorites => 2,
            GifView::Search => 3,
        });
        if self.gifs.query.as_ref() != Some(&picker.query) {
            self.gifs.query = Some(picker.query.clone());
            window.set_gif_query(picker.query.as_str().into());
        }
        let has_favorites = state.favorite_gifs.as_ref().is_some_and(|f| !f.is_empty());
        window.set_gif_has_favorites(has_favorites);
        let mut masonry = Masonry::new(window.get_gif_grid_width());
        let mut tiles = vec![];
        let mut placed = vec![];
        let mut note = String::new();
        let picture = |src: &str| match picker.pictures.get(src).map(|asked| &asked.picture) {
            Some(GifPicture::Shown(picture)) => {
                let (frames, delays) = pictures::animation(picture);
                (pictures::picture(picture), frames, delays)
            }
            _ => Default::default(),
        };
        let mut tile =
            |kind: i32, label: &str, url: &str, src: &str, shape: Shape, favorite: bool| {
                let width = masonry.width;
                let height = shape.height(width);
                let (x, y) = masonry.place(height);
                let (image, frames, delays) = picture(src);
                let (decoded_width, decoded_height) = shape.decoded(width);
                placed.push(Placed {
                    src: src.to_owned(),
                    shape,
                    top: y,
                    bottom: y + height,
                    width: decoded_width,
                    height: decoded_height,
                });
                tiles.push(GifTile {
                    kind,
                    label: label.into(),
                    url: url.into(),
                    x,
                    y,
                    w: width,
                    h: height,
                    image,
                    frames,
                    delays,
                    favorite,
                    tint: GROUNDS[tiles.len() % GROUNDS.len()],
                });
            };
        let placeholders = |count: usize, tile: &mut AddTile<'_>| {
            for index in 0..count {
                // Heights that vary, the same each time.
                let height = 120.0 + ((index * 37 + 11) % 100) as f32;
                tile(4, "", "", "", Shape::Fixed(height), false);
            }
        };
        match picker.view {
            GifView::Home => {
                if picker.categories.is_empty() && picker.home_loading {
                    for _ in 0..8 {
                        tile(4, "", "", "", Shape::Fixed(HOME_TILE), false);
                    }
                } else {
                    let newest = state.shown_favorite_gifs().first().map(|f| f.src.clone());
                    tile(
                        1,
                        "Favorites",
                        "",
                        newest.as_deref().unwrap_or_default(),
                        Shape::Fixed(HOME_TILE),
                        false,
                    );
                    let trending = picker.trending_preview.as_ref().map(|gif| gif.src.as_str());
                    tile(
                        2,
                        "Trending GIFs",
                        "",
                        trending.unwrap_or_default(),
                        Shape::Fixed(HOME_TILE),
                        false,
                    );
                    for category in &picker.categories {
                        tile(
                            3,
                            &category.name,
                            &category.name,
                            &category.src,
                            Shape::Fixed(HOME_TILE),
                            false,
                        );
                    }
                    if let Some(error) = &picker.home_error {
                        note = error.clone();
                    }
                }
            }
            GifView::Trending | GifView::Search => {
                if picker.loading {
                    placeholders(PLACEHOLDERS, &mut tile);
                } else if let Some(error) = &picker.error {
                    note = error.clone();
                } else if picker.results.is_empty() {
                    note = "No GIFs match your search.".into();
                }
                for gif in picker.results.iter().filter(|_| !picker.loading) {
                    let favorite = state.is_favorite_gif(&gif.url);
                    let shape = Shape::of(gif.width, gif.height);
                    tile(0, "", &gif.url, &gif.src, shape, favorite);
                }
            }
            GifView::Favorites => match &state.favorite_gifs {
                None => placeholders(PLACEHOLDERS / 2, &mut tile),
                Some(all) => {
                    let shown = state.shown_favorite_gifs();
                    if all.is_empty() {
                        note = "Click the star in the corner of a GIF to favorite it. Favorites will show up here!".into();
                    } else if shown.is_empty() {
                        note = "No favorite GIFs match your search.".into();
                    }
                    for favorite in shown {
                        let shape = Shape::of(favorite.width, favorite.height);
                        tile(0, "", &favorite.url, &favorite.src, shape, true);
                    }
                }
            },
        }
        window.set_gif_grid_height(masonry.bottom());
        window.set_gif_note(note.as_str().into());
        window.set_gif_suggestions(ModelRc::new(VecModel::from(
            picker
                .suggestions
                .iter()
                .filter(|_| picker.view == GifView::Search)
                .map(|term| SharedString::from(term.as_str()))
                .collect::<Vec<_>>(),
        )));
        update_model(&self.models.gif_tiles, tiles);
        if placed != self.gifs.placed || note != self.gifs.note {
            self.gifs.placed = placed;
            self.gifs.note = note.clone();
            // New tiles may end sooner: a search with none suggests others.
            self.gifs.near_end = false;
            // The pictures in sight are asked for once the refresh is over.
            let (weak, window) = (self.weak.clone(), self.window.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                if let (Some(adapter), Some(window)) = (weak.upgrade(), window.upgrade()) {
                    adapter.borrow_mut().ask_gif_pictures(&window);
                }
            });
        }
    }

    /// The grid scrolled, or changed its height.
    pub(super) fn gifs_scrolled(&mut self, window: &YapfastWindow, top: f32, height: f32) {
        self.gifs.viewport = (top, height);
        self.ask_gif_pictures(window);
    }

    /// The picker's grid changed width, as its left edge dragged it: the
    /// tiles take new columns, keeping their pictures, and once the width
    /// holds still they're asked for at their new sizes.
    pub(super) fn gifs_resized(&mut self, window: &YapfastWindow) {
        // Skia repaints only what it is told has changed, and for a popup that
        // is its own rectangle: the shadow it draws outside that would keep
        // the pixels it had where the picker was before.
        repaint_popup(window);
        if !self.app.state.gifs.open {
            return;
        }
        let mut masonry = Masonry::new(window.get_gif_grid_width());
        let tiles = &self.models.gif_tiles;
        for (row, placed) in self.gifs.placed.iter_mut().enumerate() {
            let width = masonry.width;
            let height = placed.shape.height(width);
            let (x, y) = masonry.place(height);
            (placed.width, placed.height) = placed.shape.decoded(width);
            (placed.top, placed.bottom) = (y, y + height);
            if let Some(tile) = tiles.row_data(row) {
                tiles.set_row_data(
                    row,
                    GifTile {
                        x,
                        y,
                        w: width,
                        h: height,
                        ..tile
                    },
                );
            }
        }
        window.set_gif_grid_height(masonry.bottom());
        let (weak, window) = (self.weak.clone(), self.window.clone());
        self.gifs.resized.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(250),
            move || {
                if let (Some(adapter), Some(window)) = (weak.upgrade(), window.upgrade()) {
                    adapter.borrow_mut().ask_gif_pictures(&window);
                }
            },
        );
    }

    /// Asks for the pictures of the tiles in sight and a screen either side,
    /// letting go of the rest, and says whether the grid is near its end.
    fn ask_gif_pictures(&mut self, window: &YapfastWindow) {
        if !self.app.state.gifs.open {
            return;
        }
        let (top, height) = self.gifs.viewport;
        let scale = window.window().scale_factor();
        let (from, to) = (top - height, top + 2.0 * height);
        let mut asked = vec![];
        for placed in &self.gifs.placed {
            if placed.src.is_empty()
                || placed.bottom < from
                || placed.top > to
                || asked.iter().any(|(src, _, _)| *src == placed.src)
            {
                continue;
            }
            asked.push((
                placed.src.clone(),
                (placed.width as f32 * scale) as u32,
                (placed.height as f32 * scale) as u32,
            ));
        }
        let bottom = self
            .gifs
            .placed
            .iter()
            .map(|placed| placed.bottom)
            .fold(0.0, f32::max);
        // Discord's client suggests other terms within 1180px of the end.
        let near_end = top + height >= bottom - 1180.0;
        if asked == self.gifs.asked && near_end == self.gifs.near_end {
            return;
        }
        self.gifs.asked.clone_from(&asked);
        self.gifs.near_end = near_end;
        self.action(
            window,
            Action::GifsShown {
                shown: asked,
                near_end,
            },
        );
    }
}

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    macro_rules! act {
        ($callback:ident($($arg:ident),*) => $action:expr) => {{
            let state = adapter.clone();
            let weak = window.as_weak();
            window.$callback(move |$($arg),*| {
                if let Some(window) = weak.upgrade() {
                    state.borrow_mut().action(&window, $action);
                }
            });
        }};
    }
    act!(on_open_gifs() => Action::OpenGifs);
    act!(on_close_gifs() => Action::CloseGifs);
    act!(on_gif_search(text) => Action::SearchGifs(text.into()));
    act!(on_gif_view_chosen(view) => Action::ShowGifs(match view {
        1 => GifView::Trending,
        2 => GifView::Favorites,
        3 => GifView::Search,
        _ => GifView::Home,
    }));
    act!(on_gif_category(name) => Action::SearchGifCategory(
        name.into()
    ));
    act!(on_gif_pick(url, insert) => Action::PickGif {
        url: url.into(),
        insert,
    });
    act!(on_gif_favorite(url) => Action::FavoriteGif(url.into()));
    let favorites = window.global::<GifFavorites>();
    favorites.on_is_favorite(|url, _| FAVORITES.with_borrow(|set| set.contains(url.as_str())));
    let state = adapter.clone();
    let weak = window.as_weak();
    favorites.on_toggle(move |item, index| {
        if let Some(window) = weak.upgrade() {
            let item = item.into();
            let index = index.max(0) as usize;
            state
                .borrow_mut()
                .action(&window, Action::FavoriteMessageGif { item, index });
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    favorites.on_wanted(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(&window, Action::LoadFavoriteGifs);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_gif_picker_resized(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().gifs_resized(&window);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_gifs_scrolled(move |top, height| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().gifs_scrolled(&window, top, height);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masonry_fills_the_shortest_column_first() {
        let mut masonry = Masonry::new(474.0);
        assert_eq!(masonry.columns.len(), 2);
        assert_eq!(masonry.width, 231.0);
        assert_eq!(masonry.place(100.0), (12.0, 12.0));
        assert_eq!(masonry.place(300.0), (255.0, 12.0));
        // The left column is shorter, so it takes the next two.
        assert_eq!(masonry.place(50.0), (12.0, 124.0));
        assert_eq!(masonry.place(50.0), (12.0, 186.0));
        assert_eq!(masonry.bottom(), 324.0);
        assert_eq!(Shape::of(220, 110).height(masonry.width), 115.5);
    }

    /// The picker's left edge fits more columns as it widens, and no more
    /// than 8.
    #[test]
    fn a_wider_grid_fits_more_columns() {
        let mut masonry = Masonry::new(624.0);
        assert_eq!(masonry.columns.len(), 3);
        assert_eq!(masonry.width, 200.0);
        assert_eq!(masonry.place(100.0), (12.0, 12.0));
        assert_eq!(masonry.place(100.0), (224.0, 12.0));
        assert_eq!(masonry.place(100.0), (436.0, 12.0));
        assert_eq!(Shape::of(220, 110).height(masonry.width), 100.0);
        // Eight columns is the most, however far the drag goes.
        assert_eq!(Masonry::new(2400.0).columns.len(), 8);
    }
}
