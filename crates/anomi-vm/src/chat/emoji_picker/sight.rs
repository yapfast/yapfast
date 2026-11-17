//! Which custom emoji hold their pictures: those of the rows in sight and
//! three screens of rows either side, asked for nearest first, and let go
//! of once a scroll leaves them farther (`src/ui/emoji_grid.rs`,
//! `custom_ids_to_fetch` in `src/ui/callbacks.rs:367`). A custom emoji's
//! picture comes from the disk or from Discord and takes a while, so a fast
//! scroll still finds the rows it reaches drawn; each is 16 KB decoded, and
//! all go when the picker closes.

use std::ops::RangeInclusive;

use super::{Emoji, EmojiPicker};
use crate::kind::Expected;

/// The rows of the grid that show at once in a picker of the usual height,
/// with one to spare: about 340 px of 56 px rows (`GRID_ROWS`).
pub const SHOWN_ROWS: usize = 8;

/// The rows either side of those in sight whose custom emoji's pictures are
/// asked for and kept: three screens (`FETCH_ROWS`).
pub const FETCH_ROWS: usize = 3 * SHOWN_ROWS;

impl EmojiPicker {
    /// The rows in sight, within the grid; none for an empty one.
    fn in_sight(&self, count: usize) -> Option<(usize, usize)> {
        let last = count.checked_sub(1)?;
        let (first, shown) = self.shown;
        let shown = shown.min(last);
        Some((first.min(shown), shown))
    }

    /// The rows whose custom emoji may hold their pictures: [`FETCH_ROWS`]
    /// either side of those in sight.
    pub fn held(&self) -> RangeInclusive<usize> {
        let count = self.grid().len();
        match self.in_sight(count) {
            Some((first, last)) => {
                first.saturating_sub(FETCH_ROWS)..=(last + FETCH_ROWS).min(count - 1)
            }
            None => RangeInclusive::new(1, 0),
        }
    }

    /// The ids of the custom emoji whose pictures the grid wants: those of
    /// the rows in sight and [`FETCH_ROWS`] either side still on their way,
    /// nearest first, as Discord's downloads go a few at a time: those in
    /// sight, then a row below and a row above in turn.
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
                super::Row::Emoji(emoji) => Some(emoji),
                super::Row::Heading(..) => None,
            })
            .flatten()
            .filter(|emoji| emoji.picture() == Some(&Expected::Waiting))
            .filter_map(Emoji::id)
            .collect()
    }

    /// Lets go of the pictures of the custom emoji in the rows outside
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
            let emoji = match section {
                Some(section) => &mut self.sections[section].emoji[range],
                None => &mut self.found[range],
            };
            for emoji in emoji {
                if let Emoji::Custom(custom) = emoji
                    && matches!(custom.picture, Expected::Ready(_))
                {
                    custom.picture = Expected::Waiting;
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
    use crate::chat::emoji_picker::{Section, SectionIcon};
    use iced::widget::image;

    /// A server's 100 custom emoji, one to a row: a heading and 100 rows.
    fn picker(ready: bool) -> EmojiPicker {
        let picture = if ready {
            Expected::Ready(image::Handle::from_rgba(1, 1, vec![0; 4]))
        } else {
            Expected::Waiting
        };
        EmojiPicker {
            sections: vec![Section {
                name: "Server".into(),
                icon: SectionIcon::Server {
                    id: "9".into(),
                    picture: Expected::Waiting,
                },
                emoji: (0..100)
                    .map(|n| Emoji::custom(&format!("e{n}"), &n.to_string(), picture.clone(), true))
                    .collect(),
            }],
            columns: 1,
            ..EmojiPicker::default()
        }
    }

    #[test]
    fn those_in_sight_come_first_then_a_row_either_side() {
        let mut picker = picker(false);
        picker.shown = (40, 47);
        let wanted = picker.wanted();
        // Rows 40 to 47 hold emoji 39 to 46 (row 0 is the heading).
        assert_eq!(
            &wanted[..8],
            ["39", "40", "41", "42", "43", "44", "45", "46"]
        );
        assert_eq!(&wanted[8..10], ["47", "38"]);
        assert_eq!(wanted.len(), 8 + 2 * FETCH_ROWS);
        assert_eq!(picker.held(), 16..=71);
    }

    #[test]
    fn pictures_far_from_sight_go_and_near_ones_stay() {
        let mut picker = picker(true);
        picker.shown = (0, 7);
        assert!(picker.wanted().is_empty(), "all here");
        assert!(picker.let_go_far());
        let emoji = &picker.sections[0].emoji;
        assert!(matches!(emoji[30].picture(), Some(Expected::Ready(_))));
        assert_eq!(emoji[31].picture(), Some(&Expected::Waiting));
        assert!(!picker.let_go_far(), "nothing more to let go");
    }

    #[test]
    fn an_empty_grid_wants_nothing() {
        let picker = EmojiPicker::default();
        assert!(picker.wanted().is_empty());
        assert!(picker.held().is_empty());
    }
}
