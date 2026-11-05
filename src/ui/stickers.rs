//! The sticker picker's grid: 96px tiles in columns under the name of the
//! collection they came from, the collections down its left to jump between,
//! and the pictures of those in sight loaded while the rest are let go.

use yapfast_core::application::StickerPicture;
use yapfast_core::chat::StickerCollection;

use super::*;

/// A tile's side, the gap between them and the grid's edge, as Discord's
/// picker has them.
const TILE: f32 = 96.0;
const GAP: f32 = 8.0;
const PADDING: f32 = 12.0;
/// How tall a collection's heading is.
const HEADING: f32 = 28.0;

/// How many columns the grid `width` wide holds, and where its first one
/// starts, so that they are centered as Discord's are.
fn columns(width: f32) -> (f32, f32) {
    let count = ((width + GAP) / (TILE + GAP)).floor().max(1.0);
    let left = PADDING + (width - (count * TILE + (count - 1.0) * GAP)).max(0.0) / 2.0;
    (count, left)
}

/// What the grid shows, for loading the pictures in sight.
#[derive(Default)]
pub(super) struct Shown {
    /// Each sticker tile's picture and where it is, in the order they show.
    placed: Vec<Placed>,
    /// The first picture of each pack, which its rail button shows.
    heads: Vec<String>,
    /// How far the grid is scrolled and how tall it shows, as it last said.
    viewport: (f32, f32),
    /// The pictures last asked for.
    asked: Vec<String>,
    /// The servers whose icons were asked for this session: one that failed
    /// is asked for again at the next open, and not on every refresh.
    icons_asked: HashSet<String>,
    /// The icons to ask the app's picture pipeline for, gathered as the rail
    /// is laid out.
    icons_wanted: Vec<String>,
    /// What the grid says in place of tiles.
    note: String,
    /// The search text the field was last given.
    query: Option<String>,
    /// What the grid was drawn from, without its pictures: the same inputs
    /// draw the same tiles, so a refresh with only pictures new keeps them.
    structure: Vec<String>,
    /// The tile rows each picture draws, and each heading's, by address, so a
    /// picture that arrives rewrites those rows alone. `section_rows` is the
    /// rail's buttons drawing the same picture.
    picture_rows: HashMap<String, Vec<usize>>,
    icon_rows: HashMap<String, Vec<usize>>,
    section_rows: HashMap<String, Vec<usize>>,
    /// Whether each picture drew as it shows, as it was last set.
    drawn: HashMap<String, bool>,
    /// Every tile laid out, by row: only those in sight are published, so a
    /// grid of every server's stickers stays a screenful of elements.
    tiles: Vec<StickerGridTile>,
    /// The rows of the grid the tiles were published for, top to bottom: a
    /// scroll within them changes no row. None lays them out again, as a
    /// rebuilt grid or one not yet measured does.
    published: Option<(f32, f32)>,
    /// Each picture's converted copy, by the picture and whether it draws as a
    /// still, kept while the picker shows it: a tile laid out again draws the
    /// image it had, not a new copy of it.
    surfaces: HashMap<(usize, bool), Surface>,
}

#[derive(PartialEq)]
struct Placed {
    picture: String,
    top: f32,
    bottom: f32,
}

impl Adapter {
    pub(super) fn show_stickers(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        window.set_can_pick_stickers(state.can_pick_stickers());
        if !state.stickers.open {
            if !self.stickers.placed.is_empty() || self.models.sticker_tiles.row_count() > 0 {
                self.stickers = Shown::default();
                update_model(&self.models.sticker_tiles, vec![]);
                update_model(&self.models.sticker_sections, vec![]);
            }
            return;
        }
        if self.stickers.query.as_deref() != Some(state.stickers.query.as_str()) {
            self.stickers.query = Some(state.stickers.query.clone());
            window.set_sticker_query(state.stickers.query.as_str().into());
        }
        let width = window.get_sticker_grid_width();
        let (columns, left) = columns(width);
        let picker = &state.stickers;
        let pictures = &picker.pictures;
        let favorites = &picker.favorites;
        let collections = picker.collections(self.app.discord_nitro());
        // The tiles' shape: the same inputs draw the same tiles, so a refresh
        // that brings only pictures rewrites the rows they draw.
        let mut structure = vec![format!("{width}x{columns}"), format!("q:{}", picker.query)];
        for collection in &collections {
            structure.push(format!(
                "{}:{}:{}",
                collection.id,
                collection.name,
                collection.stickers.len()
            ));
        }
        for sticker in favorites {
            structure.push(format!("f:{}", sticker.id));
        }
        for used in &picker.uses {
            structure.push(format!("u:{}:{}", used.sticker.id, used.uses));
        }
        if structure == self.stickers.structure && self.models.sticker_tiles.row_count() > 0 {
            self.draw_sticker_pictures();
            return;
        }
        self.stickers.structure = structure;
        self.stickers.picture_rows.clear();
        self.stickers.icon_rows.clear();
        self.stickers.section_rows.clear();
        self.stickers.drawn.clear();
        self.stickers.published = None;
        let surfaces = &mut self.stickers.surfaces;
        let mut tiles = vec![];
        let mut sections = vec![];
        let mut placed = vec![];
        let mut heads = vec![];
        let mut first = String::new();
        let mut top = 0.0;
        for collection in collections {
            let start = top;
            let frequent = collection.id == "frequent";
            let icon = if frequent {
                None
            } else {
                server_icon(&self.app.state.servers, &collection)
            };
            // A server's icon heads its stickers, which the app's own pictures
            // carry; a pack's own first sticker heads it until that comes.
            let mut head = Surface::default();
            if let Some(StickerPicture::Shown(picture)) = collection
                .stickers
                .first()
                .and_then(|sticker| pictures.get(&sticker.picture))
            {
                head = converted(surfaces, picture, false);
            }
            if let Some(url) = &icon {
                if let Some(StickerPicture::Shown(picture)) = pictures.get(url) {
                    head = converted(surfaces, picture, false);
                } else if let Some(picture) = state.pictures.get(url) {
                    head = converted(surfaces, picture, true);
                }
            }
            let heading_row = tiles.len();
            let section_row = sections.len();
            if let Some(url) = &icon {
                let showing = pictures.contains_key(url) || state.pictures.contains_key(url);
                self.stickers
                    .icon_rows
                    .entry(url.clone())
                    .or_default()
                    .push(heading_row);
                self.stickers
                    .section_rows
                    .entry(url.clone())
                    .or_default()
                    .push(section_row);
                self.stickers.drawn.insert(url.clone(), showing);
            }
            tiles.push(StickerGridTile {
                kind: if frequent { 2 } else { 1 },
                label: collection.name.as_str().into(),
                id: Default::default(),
                x: PADDING,
                y: top,
                w: width,
                h: HEADING,
                image: head.image.clone(),
                frames: head.frames.clone(),
                delays: head.delays.clone(),
                favorite: false,
                usable: true,
            });
            top += HEADING;
            for (index, sticker) in collection.stickers.iter().enumerate() {
                let (column, row) = (index % columns as usize, index / columns as usize);
                let x = left + column as f32 * (TILE + GAP);
                let y = top + row as f32 * (TILE + GAP);
                let shown = match pictures.get(&sticker.picture) {
                    Some(StickerPicture::Shown(picture)) => converted(surfaces, picture, false),
                    _ => Surface::default(),
                };
                if first.is_empty() {
                    first = sticker.id.clone();
                }
                self.stickers
                    .picture_rows
                    .entry(sticker.picture.clone())
                    .or_default()
                    .push(tiles.len());
                self.stickers.drawn.insert(
                    sticker.picture.clone(),
                    matches!(
                        pictures.get(&sticker.picture),
                        Some(StickerPicture::Shown(_))
                    ),
                );
                tiles.push(StickerGridTile {
                    kind: 0,
                    label: sticker.name.as_str().into(),
                    id: sticker.id.as_str().into(),
                    x,
                    y,
                    w: TILE,
                    h: TILE,
                    image: shown.image.clone(),
                    frames: shown.frames.clone(),
                    delays: shown.delays.clone(),
                    favorite: favorites.iter().any(|known| known.id == sticker.id),
                    usable: sticker.usable,
                });
                placed.push(Placed {
                    picture: sticker.picture.clone(),
                    top: y,
                    bottom: y + TILE,
                });
            }
            let rows = collection.stickers.len().div_ceil(columns as usize);
            top += rows as f32 * (TILE + GAP) + GAP;
            match icon {
                Some(url) => {
                    // Both ways ask for it: the sticker picker's own decodes it
                    // at the tile's size, and the app's picture pipeline carries
                    // every server's icon for the emoji picker too. Whichever
                    // comes draws.
                    heads.push(url.clone());
                    if !self.app.state.pictures.contains_key(&url)
                        && self.stickers.icons_asked.insert(url.clone())
                    {
                        self.stickers.icons_wanted.push(url);
                    }
                }
                None => {
                    if let Some(sticker) = collection.stickers.first() {
                        heads.push(sticker.picture.clone());
                        let showing = pictures.contains_key(&sticker.picture)
                            || state.pictures.contains_key(&sticker.picture);
                        self.stickers
                            .icon_rows
                            .entry(sticker.picture.clone())
                            .or_default()
                            .push(heading_row);
                        self.stickers
                            .section_rows
                            .entry(sticker.picture.clone())
                            .or_default()
                            .push(section_row);
                        self.stickers.drawn.insert(sticker.picture.clone(), showing);
                    }
                }
            }
            sections.push(RailSection {
                name: collection.name.as_str().into(),
                icon: EmojiCell {
                    name: collection.name.as_str().into(),
                    picture: head.image.clone(),
                    ..Default::default()
                },
                clock: frequent,
                top: start,
                end: top,
            });
        }
        let note = if placed.is_empty() {
            picker.error.clone().unwrap_or_else(|| {
                if picker.query.trim().is_empty() {
                    String::new()
                } else {
                    "No stickers match your search.".into()
                }
            })
        } else {
            String::new()
        };
        if !self.stickers.icons_wanted.is_empty() {
            let wanted = std::mem::take(&mut self.stickers.icons_wanted);
            self.action(window, Action::WantedPictures(wanted));
        }
        window.set_sticker_first_id(first.as_str().into());
        window.set_sticker_grid_height(top);
        window.set_sticker_note(note.as_str().into());
        self.stickers.tiles = tiles;
        self.publish_sticker_tiles(false);
        update_model(&self.models.sticker_sections, sections);
        if placed != self.stickers.placed
            || note != self.stickers.note
            || heads != self.stickers.heads
        {
            self.stickers.placed = placed;
            self.stickers.heads = heads;
            self.stickers.note = note;
            // The pictures in sight are asked for once the refresh is over.
            let (weak, window) = (self.weak.clone(), self.window.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                if let (Some(adapter), Some(window)) = (weak.upgrade(), window.upgrade()) {
                    adapter.borrow_mut().ask_sticker_pictures(&window);
                }
            });
        }
    }

    /// Draws the pictures that came since the grid was laid out, rewriting only
    /// the rows they draw: pictures arrive as the grid scrolls, and laying
    /// every tile out again for each one made the scroll stutter.
    fn draw_sticker_pictures(&mut self) {
        let state = &self.app.state;
        let pictures = &state.stickers.pictures;
        let surfaces = &mut self.stickers.surfaces;
        let showing = |url: &String| {
            matches!(pictures.get(url), Some(StickerPicture::Shown(_)))
                || state.pictures.contains_key(url)
        };
        // Only the pictures a tile or a heading draws can have changed, and
        // those are the ones asked for plus the servers' icons.
        let mut candidates: Vec<String> = pictures.keys().cloned().collect();
        candidates.extend(self.stickers.icon_rows.keys().cloned());
        let mut changed: Vec<String> = vec![];
        for url in candidates {
            if !self.stickers.picture_rows.contains_key(&url)
                && !self.stickers.icon_rows.contains_key(&url)
            {
                continue;
            }
            if self.stickers.drawn.get(&url) != Some(&showing(&url)) {
                changed.push(url);
            }
        }
        for url in changed {
            let now = showing(&url);
            self.stickers.drawn.insert(url.clone(), now);
            let surface = match pictures.get(&url) {
                Some(StickerPicture::Shown(picture)) => converted(surfaces, picture, false),
                _ => match state.pictures.get(&url) {
                    Some(picture) => converted(surfaces, picture, true),
                    None => Surface::default(),
                },
            };
            let rows: Vec<usize> = self
                .stickers
                .picture_rows
                .get(&url)
                .into_iter()
                .flatten()
                .chain(self.stickers.icon_rows.get(&url).into_iter().flatten())
                .copied()
                .collect();
            for row in rows {
                if let Some(tile) = self.stickers.tiles.get_mut(row) {
                    tile.image = surface.image.clone();
                    tile.frames = surface.frames.clone();
                    tile.delays = surface.delays.clone();
                }
            }
            let sections: Vec<usize> = self
                .stickers
                .section_rows
                .get(&url)
                .into_iter()
                .flatten()
                .copied()
                .collect();
            for row in sections {
                if let Some(section) = self.models.sticker_sections.row_data(row) {
                    self.models.sticker_sections.set_row_data(
                        row,
                        RailSection {
                            icon: EmojiCell {
                                picture: surface.image.clone(),
                                ..section.icon
                            },
                            ..section
                        },
                    );
                }
            }
        }
        self.publish_sticker_tiles(true);
    }

    /// Gives the grid the tiles in sight and a screen either side: the rest stay
    /// laid out but unshown, so a grid of thousands costs a few dozen elements.
    /// `forced` lays them out again even where the rows already cover what is
    /// in sight, as a picture that came must.
    fn publish_sticker_tiles(&mut self, forced: bool) {
        let (top, height) = self.stickers.viewport;
        // Until the grid has been laid out its height is none: the whole of it
        // goes, and the first report windows it.
        if height <= 0.0 {
            self.stickers.published = None;
            update_model(&self.models.sticker_tiles, self.stickers.tiles.clone());
            return;
        }
        let (from, to) = (top - height, top + 2.0 * height);
        // What is in sight is inside what was published and no tile changed:
        // the scroll moved and no row did, so the model is left as it is.
        if !forced
            && let Some((published_from, published_to)) = self.stickers.published
            && from >= published_from
            && to <= published_to
            && self.models.sticker_tiles.row_count() > 0
        {
            return;
        }
        self.stickers.published = Some((from, to));
        let shown: Vec<StickerGridTile> = self
            .stickers
            .tiles
            .iter()
            .filter(|tile| tile.y + tile.h >= from && tile.y <= to)
            .cloned()
            .collect();
        update_model(&self.models.sticker_tiles, shown);
    }

    /// The picker's left edge dragged it wider, so its grid takes new
    /// columns, as the GIFs' and the emoji's do.
    pub(super) fn stickers_resized(&mut self, window: &YapfastWindow) {
        // Skia repaints only what it is told has changed, and for a popup that
        // is its own rectangle: the shadow it draws outside that would keep
        // the pixels it had where the picker was before.
        repaint_popup(window);
        if !self.app.state.stickers.open {
            return;
        }
        self.show_stickers(window);
    }

    /// The grid scrolled or changed its height.
    pub(super) fn stickers_scrolled(&mut self, window: &YapfastWindow, top: f32, height: f32) {
        self.stickers.viewport = (top, height);
        self.publish_sticker_tiles(false);
        // The pictures two screens ahead are asked for at once, so a tile that
        // comes into sight is already drawn; asking again is what a new set of
        // them costs, not what a scroll does.
        self.ask_sticker_pictures(window);
    }

    /// Asks for the pictures of the tiles in sight and a screen either side,
    /// and of each pack's first sticker for the rail, letting go of the rest.
    fn ask_sticker_pictures(&mut self, window: &YapfastWindow) {
        if !self.app.state.stickers.open {
            return;
        }
        let (top, height) = self.stickers.viewport;
        // Two screens either side: a fast scroll shows what is decoded, not
        // what is still being decoded.
        let (from, to) = (top - 2.0 * height, top + 3.0 * height);
        let mut asked: Vec<String> = self.stickers.heads.clone();
        for placed in &self.stickers.placed {
            if placed.bottom < from || placed.top > to || asked.contains(&placed.picture) {
                continue;
            }
            asked.push(placed.picture.clone());
        }
        if asked == self.stickers.asked {
            return;
        }
        self.stickers.asked.clone_from(&asked);
        self.action(window, Action::StickersShown(asked));
    }
}

/// A picture's converted copy, by the picture and whether it draws as a still,
/// so a tile laid out again draws the image it had rather than a new copy.
fn converted(
    surfaces: &mut HashMap<(usize, bool), Surface>,
    picture: &Arc<Picture>,
    still: bool,
) -> Surface {
    let key = (Arc::as_ptr(picture) as usize, still);
    surfaces
        .entry(key)
        .or_insert_with(|| {
            if still {
                Surface::still(picture)
            } else {
                Surface::shown(picture)
            }
        })
        .clone()
}

/// The icon of the server a collection of stickers is, which its heading and
/// its rail button draw: none for Discord's packs.
fn server_icon(
    servers: &[Arc<yapfast_core::chat::ChatServer>],
    collection: &StickerCollection,
) -> Option<String> {
    let server = yapfast_core::discord::server_id(&collection.id);
    servers
        .iter()
        .find(|known| known.id == server)?
        .icon
        .clone()
}

/// A sticker's picture as a tile or the rail draws it: the still, and an
/// animation's frames.
#[derive(Clone, Default)]
struct Surface {
    image: slint::Image,
    frames: ModelRc<slint::Image>,
    delays: ModelRc<i32>,
}

impl Surface {
    /// The surface of a picture that may move, as a sticker's may.
    fn shown(picture: &Arc<Picture>) -> Self {
        let (frames, delays) = pictures::animation(picture);
        Self {
            image: pictures::picture(picture),
            frames,
            delays,
        }
    }

    /// A still picture, as a server's icon is.
    fn still(picture: &Picture) -> Self {
        Self {
            image: pictures::picture(picture),
            ..Self::default()
        }
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
    act!(on_open_stickers() => Action::OpenStickers);
    act!(on_close_stickers() => Action::CloseStickers);
    act!(on_sticker_pick(id) => Action::PickSticker(id.into()));
    act!(on_sticker_favorite(id) => Action::FavoriteSticker(id.into()));
    act!(on_sticker_search(text) => Action::SearchStickers(text.into()));
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_stickers_scrolled(move |top, height| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().stickers_scrolled(&window, top, height);
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_sticker_reflow(move || {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().stickers_resized(&window);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_centers_its_columns() {
        // The picker's 498px, less the rail and 12px either side: four 96px
        // tiles, 8px apart, centered.
        let (count, left) = columns(426.0);
        assert_eq!(count, 4.0);
        assert_eq!(left, 21.0);
        // Wide enough for six, and never fewer than one.
        assert_eq!(columns(700.0).0, 6.0);
        assert_eq!(columns(60.0).0, 1.0);
    }
}
