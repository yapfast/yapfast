//! Which stickers hold their pictures: those of the rows in sight and two
//! screens of rows either side, asked for nearest first, and let go of once
//! a scroll leaves them farther (`ask_sticker_pictures`,
//! `src/ui/stickers.rs`: from two screens above to three below the view).
//! Each section's icon is asked for whatever the scroll, as the rail
//! shows it. The pictures are decoded for the tiles and all go when the
//! picker closes.

use std::ops::RangeInclusive;

use super::{Row, SectionIcon, StickerPicker};
use crate::kind::Expected;

/// The rows of the grid that show at once in a picker of the usual height,
/// with one to spare: about 330 px of 104 px rows.
pub const SHOWN_ROWS: usize = 4;

/// The rows either side of those in sight whose pictures are asked for and
/// kept: two screens.
pub const FETCH_ROWS: usize = 2 * SHOWN_ROWS;

impl StickerPicker {
    /// The rows in sight, within the grid; none for an empty one.
    fn in_sight(&self, count: usize) -> Option<(usize, usize)> {
        let last = count.checked_sub(1)?;
        let (first, shown) = self.shown;
        let shown = shown.min(last);
        Some((first.min(shown), shown))
    }

    /// The rows whose stickers may hold their pictures: [`FETCH_ROWS`] either
    /// side of those in sight.
    pub fn held(&self) -> RangeInclusive<usize> {
        let count = self.grid().len();
        match self.in_sight(count) {
            Some((first, last)) => {
                first.saturating_sub(FETCH_ROWS)..=(last + FETCH_ROWS).min(count - 1)
            }
            None => RangeInclusive::new(1, 0),
        }
    }

    /// The ids of the stickers whose pictures the grid wants: those of the
    /// rows in sight and [`FETCH_ROWS`] either side still on their way,
    /// nearest first.
    pub fn wanted(&self) -> Vec<&str> {
        let grid = self.grid();
        let Some((first, last)) = self.in_sight(grid.len()) else {
            return Vec::new();
        };
        let mut order: Vec<usize> = (first..=last).collect();
        for step in 1..=FETCH_ROWS {
            if last + step < grid.len() {
                order.push(last + step);
            }
            if let Some(above) = first.checked_sub(step) {
                order.push(above);
            }
        }
        order
            .into_iter()
            .filter_map(|index| match grid.row(index)? {
                Row::Stickers { stickers, .. } => Some(stickers),
                Row::Heading(..) => None,
            })
            .flatten()
            .filter(|sticker| sticker.picture == Expected::Waiting)
            .map(|sticker| sticker.id.as_str())
            .collect()
    }

    /// The ids of the sections' icons still on their way, which the app asks
    /// for first: a server's id for its icon, a sticker's for a pack's.
    pub fn wanted_icons(&self) -> Vec<&str> {
        self.sections
            .iter()
            .filter_map(|section| match &section.icon {
                SectionIcon::Server { id, picture } | SectionIcon::Sticker { id, picture }
                    if *picture == Expected::Waiting =>
                {
                    Some(id.as_str())
                }
                _ => None,
            })
            .collect()
    }

    /// Lets go of the pictures of the stickers in the rows outside
    /// [`Self::held`], each waiting again: those a scroll left far behind.
    /// Whether any went.
    pub fn let_go_far(&mut self) -> bool {
        let held = self.held();
        let spans: Vec<_> = {
            let grid = self.grid();
            let below = held.end().saturating_add(1)..grid.len();
            (0..*held.start().min(&grid.len()))
                .chain(below)
                .filter_map(|index| grid.span(index))
                .collect()
        };
        let mut gone = false;
        for (section, range) in spans {
            for sticker in &mut self.sections[section].stickers[range] {
                if matches!(sticker.picture, Expected::Ready(_)) {
                    sticker.picture = Expected::Waiting;
                    gone = true;
                }
            }
        }
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::sticker_picker::{Section, Sticker};
    use iced::widget::image;

    /// A server's 100 stickers, one to a row: a heading and 100 rows.
    fn picker(ready: bool) -> StickerPicker {
        let picture = if ready {
            Expected::Ready(image::Handle::from_rgba(1, 1, vec![0; 4]))
        } else {
            Expected::Waiting
        };
        StickerPicker {
            sections: vec![Section {
                id: "9".into(),
                name: "Server".into(),
                icon: SectionIcon::Server {
                    id: "9".into(),
                    picture: Expected::Waiting,
                },
                stickers: (0..100)
                    .map(|n| Sticker {
                        id: n.to_string(),
                        picture: picture.clone(),
                        usable: true,
                        ..Sticker::default()
                    })
                    .collect(),
            }],
            columns: 1,
            ..StickerPicker::default()
        }
    }

    #[test]
    fn those_in_sight_come_first_then_a_row_either_side() {
        let mut picker = picker(false);
        picker.shown = (40, 43);
        let wanted = picker.wanted();
        // Rows 40 to 43 hold stickers 39 to 42 (row 0 is the heading).
        assert_eq!(&wanted[..4], ["39", "40", "41", "42"]);
        assert_eq!(&wanted[4..6], ["43", "38"]);
        assert_eq!(wanted.len(), 4 + 2 * FETCH_ROWS);
        assert_eq!(picker.held(), 32..=51);
        assert_eq!(picker.wanted_icons(), ["9"]);
    }

    #[test]
    fn pictures_far_from_sight_go_and_near_ones_stay() {
        let mut picker = picker(true);
        picker.shown = (0, 3);
        assert!(picker.wanted().is_empty(), "all here");
        assert!(picker.let_go_far());
        let stickers = &picker.sections[0].stickers;
        assert!(matches!(stickers[10].picture, Expected::Ready(_)));
        assert_eq!(stickers[11].picture, Expected::Waiting);
        assert!(!picker.let_go_far(), "nothing more to let go");
    }

    #[test]
    fn an_empty_grid_wants_nothing() {
        let picker = StickerPicker::default();
        assert!(picker.wanted().is_empty());
        assert!(picker.held().is_empty());
    }
}
