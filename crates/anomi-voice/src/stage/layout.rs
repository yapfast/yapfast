//! Where the grid puts each tile: `ParticipantGrid`'s properties and pure
//! functions (`ui/voice.slint:910`-`:982`), from the room it has and
//! [`Arrangement`], which tile is large and what shows.
//!
//! A few people take the columns, and the size, that make their tiles
//! largest within both the width and the height, so a short call above a
//! DM's messages shows everyone without scrolling: side by side wins unless
//! stacking makes tiles a tenth larger, and four sit two by two or in one
//! row, never three over one. More people take tiles at least 240 px wide,
//! and scroll. With a tile large, the others are small, 120 to 192 px
//! wide, in rows beneath it; full screen shows it alone. A short last row
//! is centred under the full ones, as Discord's is.

use iced::{Point, Rectangle, Size};

use anomi_vm::voice::stage::{Placement, Stage};

/// Every tile is 16:9 (`:926`).
pub(super) const RATIO: f32 = 0.5625;
/// Between tiles, and round them (`:910`, `:911`); none full screen.
const GAP: f32 = 12.0;
const INSET: f32 = 24.0;
/// A tile is at most this wide (`:925`), and a few at least the other
/// (`:951`), as tall as a large one is at least (`:945`).
const WIDEST: f32 = 420.0;
const NARROWEST: f32 = 160.0;
/// More than four take columns of at least this (`:928`).
const CROWDED: f32 = 240.0;
/// The tiles beneath a large one (`:937`).
const STRIP_LEAST: f32 = 120.0;
const STRIP_MOST: f32 = 192.0;
/// How much larger stacking must make tiles to win (`:929`).
const LARGER: f32 = 1.1;

/// Which tile is large and what shows, from the stage's tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Arrangement {
    pub count: usize,
    pub focused: Option<usize>,
    pub full: bool,
    pub others_hidden: bool,
}

impl Arrangement {
    pub fn of(stage: &Stage) -> Self {
        Self {
            count: stage.tiles.len(),
            focused: stage.focused(),
            full: stage.full(),
            others_hidden: stage.others_hidden(),
        }
    }

    /// The tile at `slot` shows large (`large`, `:965`).
    pub fn large(&self, slot: usize) -> bool {
        self.focused == Some(slot)
    }

    /// The tile at `slot` shows: full screen and hidden others leave the
    /// large tile alone (`shown`, `:964`).
    pub fn shown(&self, slot: usize) -> bool {
        self.large(slot) || !(self.full || (self.focused.is_some() && self.others_hidden))
    }
}

/// The grid laid out in a room of `size`: every tile's place in the
/// content, which scrolls when taller than the room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    arrangement: Arrangement,
    size: Size,
    gap: f32,
    columns: usize,
    tile: Size,
    focus: Rectangle,
    first_row: f32,
    placed: usize,
    rows: usize,
    /// The content's height, at least the room's.
    pub content: f32,
}

impl Geometry {
    pub fn new(arrangement: Arrangement, size: Size) -> Self {
        let Arrangement {
            count,
            focused,
            full,
            others_hidden,
        } = arrangement;
        let (width, height) = (size.width, size.height);
        let gap = if full { 0.0 } else { GAP };
        let inset = if full { 0.0 } else { INSET };
        let span = width - 2.0 * inset;
        let fitted = |columns: usize| {
            let rows = count.div_ceil(columns) as f32;
            let columns = columns as f32;
            WIDEST
                .min((span - (columns - 1.0) * gap) / columns)
                .min((height - 2.0 * inset - (rows - 1.0) * gap) / rows / RATIO)
        };
        let fewest = (((span + gap) / (CROWDED + gap)).floor() as usize)
            .min(count)
            .max(1);
        let best_fit = if count >= 2 && fitted(2) * LARGER > fitted(1) {
            match count {
                3 if fitted(3) * LARGER > fitted(2) => 3,
                4 if fitted(4) * LARGER > fitted(2) => 4,
                _ => 2,
            }
        } else {
            1
        };
        let focusing = focused.is_some();
        let others = count.saturating_sub(1);
        let strip_width = if others == 0 {
            0.0
        } else {
            let others = others as f32;
            STRIP_LEAST.max(STRIP_MOST.min((span - (others - 1.0) * gap) / others))
        };
        let fit = ((span + gap) / (strip_width + gap)).floor();
        let strip_columns = (others as f32).min(fit).max(1.0) as usize;
        let strip_rows = if full || others_hidden || others == 0 {
            0
        } else {
            others.div_ceil(strip_columns)
        };
        let strip_height =
            strip_rows as f32 * strip_width * RATIO + strip_rows.saturating_sub(1) as f32 * gap;
        let under_focus = if strip_rows > 0 {
            gap + strip_height
        } else {
            0.0
        };
        let focus_width = if full {
            width
        } else {
            span.min(NARROWEST.max(height - 2.0 * inset - under_focus) / RATIO)
        };
        let focus_height = if full { height } else { focus_width * RATIO };
        let focus_top = if full {
            0.0
        } else {
            inset.max((height - focus_height - under_focus) / 2.0)
        };
        let columns = match () {
            () if focusing && (full || others_hidden) => 0,
            () if focusing => strip_columns,
            () if count <= 4 => best_fit,
            () => fewest,
        };
        let tile_width = if focusing {
            strip_width
        } else if count <= 4 {
            NARROWEST.max(fitted(columns))
        } else {
            let columns = columns as f32;
            WIDEST.min((span - (columns - 1.0) * gap) / columns)
        };
        let tile = Size::new(tile_width, tile_width * RATIO);
        let placed = if focusing { others } else { count };
        let rows = placed.div_ceil(columns.max(1));
        let grid_height = rows as f32 * tile.height + rows.saturating_sub(1) as f32 * gap;
        let (first_row, content) = if focusing {
            (
                focus_top + focus_height + gap,
                height.max(focus_top + focus_height + under_focus + inset),
            )
        } else {
            (
                inset.max((height - grid_height) / 2.0),
                height.max(grid_height + 2.0 * inset),
            )
        };
        Self {
            arrangement,
            size,
            gap,
            columns,
            tile,
            focus: Rectangle::new(
                Point::new((width - focus_width) / 2.0, focus_top),
                Size::new(focus_width, focus_height),
            ),
            first_row,
            placed,
            rows,
            content,
        }
    }

    /// Where the tile at `slot` lies in the content (`tile-x`, `tile-y`,
    /// `:975`-`:982`).
    pub fn rect(&self, slot: usize) -> Rectangle {
        if self.arrangement.large(slot) {
            return self.focus;
        }
        // The rows skip the large tile (`place`, `:967`).
        let place = match self.arrangement.focused {
            Some(focused) if slot > focused => slot - 1,
            _ => slot,
        };
        let columns = self.columns.max(1);
        let (row, column) = (place / columns, place % columns);
        // A short last row is centred under the full ones (`row-start`).
        let in_row = if row + 1 == self.rows {
            self.placed - (self.rows - 1) * columns
        } else {
            columns
        };
        let step = Size::new(self.tile.width + self.gap, self.tile.height + self.gap);
        let start = (self.size.width - in_row as f32 * step.width + self.gap) / 2.0;
        Rectangle::new(
            Point::new(
                start + column as f32 * step.width,
                self.first_row + row as f32 * step.height,
            ),
            self.tile,
        )
    }

    /// Where the tiles are, scrolled down by `scrolled`, for the app
    /// (`report`, `:896`).
    pub fn placement(&self, scrolled: f32) -> Placement {
        let focused = self.arrangement.focused;
        let focus = focused.map_or(Size::ZERO, |_| self.focus.size());
        Placement {
            columns: self.columns as u32,
            width: self.tile.width,
            height: self.tile.height,
            gap: self.gap,
            top: self.first_row,
            scrolled,
            shown: self.size.height,
            focused,
            focus_width: focus.width,
            focus_height: focus.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The goldens' room for the grid: the conversation beside a 376 px
    /// sidebar, between the call's line (85.5 px down) and the pill's 80.
    const GOLDEN: Size = Size::new(1024.0, 694.5);
    const LEFT: f32 = 376.0;
    const TOP: f32 = 85.5;

    fn grid(count: usize, focused: Option<usize>) -> Geometry {
        let arrangement = Arrangement {
            count,
            focused,
            ..Arrangement::default()
        };
        Geometry::new(arrangement, GOLDEN)
    }

    /// The tile at `slot` in the window, as V03's tests measured the
    /// goldens: left, top, width, height.
    fn window(geometry: &Geometry, slot: usize) -> (f32, f32, f32, f32) {
        let rect = geometry.rect(slot);
        (rect.x + LEFT, rect.y + TOP, rect.width, rect.height)
    }

    #[test]
    fn four_sit_two_by_two() {
        // `voice/stage.png`.
        let four = grid(4, None);
        assert_eq!(window(&four, 0), (462.0, 190.5, 420.0, 236.25));
        assert_eq!(window(&four, 3), (894.0, 438.75, 420.0, 236.25));
        assert_eq!(four.content, GOLDEN.height, "nothing to scroll");
    }

    #[test]
    fn a_short_last_row_is_centred() {
        // `light/voice_stage.png`: two over one, not three in a row.
        let three = grid(3, None);
        assert_eq!(window(&three, 1).0, 894.0);
        assert_eq!(window(&three, 2), (678.0, 438.75, 420.0, 236.25));
    }

    #[test]
    fn a_large_tile_has_the_others_in_a_row_beneath() {
        // `voice/watching_stream.png`: Bob's tile, the second, large.
        let watching = grid(4, Some(1));
        assert_eq!(window(&watching, 1), (420.0, 109.5, 936.0, 526.5));
        assert_eq!(window(&watching, 0), (588.0, 648.0, 192.0, 108.0));
        assert_eq!(window(&watching, 2).0, 792.0, "the rows skip it");
        assert_eq!(window(&watching, 3).0, 996.0);
        // `voice/with_camera.png`: four beneath.
        let camera = grid(5, Some(0));
        assert_eq!(window(&camera, 1).0, 486.0);
        assert_eq!(window(&camera, 4).0, 1098.0);
    }

    #[test]
    fn a_short_call_fits_its_room() {
        // `small/voice_stage.png`: 624 by 475 keeps two columns.
        let small = Geometry::new(
            Arrangement {
                count: 4,
                ..Arrangement::default()
            },
            Size::new(624.0, 475.0),
        );
        assert_eq!(small.rect(0).width, 282.0);
        // `dmcall/connected_bob_dm.png`: two side by side above the messages.
        let dm = Geometry::new(
            Arrangement {
                count: 2,
                ..Arrangement::default()
            },
            Size::new(704.0, 359.0),
        );
        assert_eq!((dm.rect(0).width, dm.rect(1).y), (322.0, dm.rect(0).y));
        // One alone is as large as fits, at most 420 px.
        assert_eq!(grid(1, None).rect(0).width, 420.0);
    }

    #[test]
    fn many_take_columns_of_at_least_240_and_scroll() {
        // Three of 317 px fit the 976 px between the insets; a fourth of
        // 240 doesn't.
        let ten = grid(10, None);
        let rect = ten.rect(0);
        assert_eq!(ten.rect(2).y, rect.y, "three to a row");
        assert_eq!(ten.rect(3).y, rect.y + rect.height + GAP);
        assert!(rect.width >= CROWDED && rect.width <= WIDEST);
        assert_eq!(window(&ten, 9).0, 376.0 + (1024.0 - rect.width) / 2.0);
        assert!(ten.content > GOLDEN.height, "four rows scroll");
        assert_eq!(rect.y, INSET);
    }

    #[test]
    fn full_screen_shows_the_large_tile_alone_edge_to_edge() {
        let arrangement = Arrangement {
            count: 4,
            focused: Some(1),
            full: true,
            others_hidden: false,
        };
        let full = Geometry::new(arrangement, GOLDEN);
        assert_eq!(full.rect(1), Rectangle::new(Point::ORIGIN, GOLDEN));
        assert!(arrangement.shown(1) && !arrangement.shown(0));
        let placement = full.placement(0.0);
        assert_eq!((placement.columns, placement.focused), (0, Some(1)));
        assert_eq!(placement.shown_at(0, 1.0), None);
    }

    #[test]
    fn hidden_others_give_the_large_tile_their_room() {
        let arrangement = Arrangement {
            count: 4,
            focused: Some(1),
            full: false,
            others_hidden: true,
        };
        let alone = Geometry::new(arrangement, GOLDEN);
        assert!(alone.rect(1).height > grid(4, Some(1)).rect(1).height);
        assert!(!arrangement.shown(0) && arrangement.shown(1));
        assert_eq!(alone.placement(0.0).columns, 0);
    }

    #[test]
    fn the_placement_is_what_the_app_reads() {
        let four = grid(4, None).placement(12.0);
        assert_eq!((four.columns, four.width, four.gap), (2, 420.0, GAP));
        assert_eq!((four.top, four.scrolled, four.shown), (105.0, 12.0, 694.5));
        assert_eq!((four.focused, four.focus_width), (None, 0.0));
        let large = grid(4, Some(1)).placement(0.0);
        assert_eq!((large.columns, large.focus_height), (3, 526.5));
        assert_eq!(large.shown_at(1, 2.0), Some((1872, 1053)));
        assert_eq!(large.shown_at(3, 1.0), Some((192, 108)));
        assert_eq!(grid(0, None).placement(0.0).focused, None);
    }
}
