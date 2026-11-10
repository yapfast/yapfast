//! What the emoji picker's grid draws of its rows: the emoji and headings of
//! the rows in sight and a row or two around them, each placed where its row
//! starts, on buttons kept from one scroll to the next.
//!
//! The grid was a list view of rows, each a row of buttons, and the list made
//! the row that scrolled into sight anew and dropped the one that scrolled out:
//! a column's worth of buttons made and let go for each row a scroll crossed,
//! so the wider the picker, the more each scroll cost. Here a button whose
//! emoji scrolled out shows one that scrolled in instead, so a scroll sets the
//! data of the buttons whose rows crossed the edge, and makes none.

use super::*;

/// The rows either side of those in sight that the grid holds too, so a row a
/// scroll brings in is drawn before it shows.
const MARGIN: usize = 2;

/// The rows of the picker's grid that fit at once, with one to spare: the
/// grid is about 340px tall and a row is 56px.
pub(super) const GRID_ROWS: usize = 8;

/// The rows either side of those shown whose custom emoji's pictures the
/// picker asks for: three screens, since those come from the disk or from
/// Discord and take a while, so a fast scroll still finds them there rather
/// than placeholders. Each is 16 KB decoded, and they go when the picker closes.
pub(super) const FETCH_ROWS: usize = 3 * GRID_ROWS;

/// Where a row's first emoji starts across the grid, inside its padding, as
/// `EmojiPicker` in ui/messages.slint draws it.
const PADDING: f32 = 8.0;

#[derive(Default)]
pub(super) struct EmojiGrid {
    /// How far the grid is scrolled and how tall it shows, as it last said.
    pub(super) viewport: (f32, f32),
    /// The rows in sight, first and last, as `viewport` puts them.
    pub(super) shown: (usize, usize),
    /// The emoji each button of the grid shows, by its row and column.
    slots: Vec<(usize, usize)>,
    /// The custom emoji whose pictures were asked for and haven't come: each
    /// id's picture's address.
    pub(super) wanted: HashMap<String, String>,
    /// The rows whose custom emoji may hold their pictures, first and last:
    /// those as far as [`FETCH_ROWS`] from the rows in sight when the grid
    /// last scrolled.
    pub(super) custom_held: Option<(usize, usize)>,
}

impl EmojiGrid {
    /// The custom emoji of [`Self::wanted`] whose pictures came, which are no
    /// longer wanted.
    pub(super) fn take_arrived(
        &mut self,
        pictures: &HashMap<String, std::sync::Arc<yapfast_core::chat::Picture>>,
    ) -> Vec<(String, std::sync::Arc<yapfast_core::chat::Picture>)> {
        let mut arrived = Vec::new();
        self.wanted.retain(|id, url| match pictures.get(url) {
            Some(picture) => {
                arrived.push((id.clone(), picture.clone()));
                false
            }
            None => true,
        });
        arrived
    }
}

impl Adapter {
    /// Works out the rows in sight again, from where the grid last said it
    /// was scrolled to: after a scroll, or with the rows built anew.
    pub(super) fn emoji_rows_in_sight(&mut self) -> (usize, usize) {
        let (top, height) = self.emoji_grid.viewport;
        // Until the grid says how tall it is, the 340px it is in a picker of
        // the usual height.
        let height = if height > 0.0 { height } else { 340.0 };
        let shown = emoji::visible_rows(&self.models.emoji_rows, top, height);
        self.emoji_grid.shown = shown;
        shown
    }

    /// Gives the grid the emoji and headings of the rows in sight and
    /// [`MARGIN`] either side, each emoji on the button that showed it last if
    /// one did, and the button of one gone otherwise. Only a button whose emoji
    /// changed, or got its picture, is set.
    pub(super) fn publish_emoji_grid(&mut self) {
        let rows = &self.models.emoji_rows;
        let count = rows.row_count();
        let (first, last) = self.emoji_grid.shown;
        let mut tiles = Vec::new();
        let mut headings = Vec::new();
        if count > 0 && first < count {
            let from = first.saturating_sub(MARGIN);
            let to = last.saturating_add(MARGIN).min(count - 1);
            for index in from..=to {
                let Some(row) = rows.row_data(index) else {
                    continue;
                };
                for (column, cell) in row.cells.iter().enumerate() {
                    let animated = self.animations.animates_in_picker(&cell.key);
                    tiles.push((
                        (index, column),
                        EmojiTile {
                            cell,
                            x: PADDING + column as f32 * emoji::CELL,
                            y: row.top,
                            animated,
                        },
                    ));
                }
                if !row.heading.is_empty() {
                    headings.push(row);
                }
            }
        }
        update_model_by_slot(&self.models.emoji_tiles, &mut self.emoji_grid.slots, tiles);
        update_model(&self.models.emoji_headings, headings);
    }

    /// Keeps the pictures of the wanted custom emoji that are in memory now,
    /// and gives them to the rows near those in sight; see
    /// [`Self::fill_emoji_near_sight`]. True when any came.
    pub(super) fn show_arrived_emoji(&mut self) -> bool {
        let arrived = self.emoji_grid.take_arrived(&self.app.state.pictures);
        let added = crate::custom_emoji::add_images(
            arrived
                .iter()
                .map(|(id, picture)| (id.as_str(), picture.as_ref())),
        );
        if added {
            self.fill_emoji_near_sight();
        }
        added
    }

    /// Gives the custom emoji of the rows near those in sight, as far out as
    /// their pictures are asked for, the pictures that came, and the grid
    /// what changed in sight.
    pub(super) fn fill_emoji_near_sight(&mut self) {
        let (first, last) = self.emoji_grid.shown;
        emoji::fill_pictures_in(
            &self.models.emoji_rows,
            first.saturating_sub(FETCH_ROWS),
            last.saturating_add(FETCH_ROWS),
        );
        emoji::fill_section_pictures(&self.models.emoji_sections);
        super::callbacks::fill_picker_icons(self);
        self.publish_emoji_grid();
    }

    /// The grid went with the picker: its buttons and their pictures go too.
    pub(super) fn clear_emoji_grid(&mut self) {
        self.models.emoji_tiles.set_vec(Vec::new());
        self.models.emoji_headings.set_vec(Vec::new());
        self.emoji_grid = EmojiGrid::default();
    }
}
