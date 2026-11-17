//! The grid's rows: each section's heading, then its emoji, as many to a
//! row as the picker's columns, or a search's matches under no heading
//! (`rows` in `src/emoji.rs:450`). The Slint picker placed each row by the
//! top it carried; the virtual list measures them, so a row here is only
//! what it holds, and the grid knows where each section starts.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use super::{Emoji, EmojiPicker, Section, SectionIcon};

/// A row key's low bits count the rows within its section; the high bits
/// are the section's own, so a row keeps its key as sections come and go
/// before it (a first favourite, a server joined).
const WITHIN: u64 = 0xF_FFFF;
/// The high bits of a search's matches.
const FOUND: u64 = !WITHIN;

/// A row of the grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row<'a> {
    /// A section's heading, with the section's index.
    Heading(usize, &'a Section),
    /// Up to a row's worth of emoji.
    Emoji(&'a [Emoji]),
}

/// The grid's rows, as the picker's columns cut its sections.
pub struct Grid<'a> {
    picker: &'a EmojiPicker,
    columns: usize,
    searching: bool,
    /// The first row of each section (of the matches, while searching),
    /// and the count of rows after the last.
    starts: Vec<usize>,
}

impl<'a> Grid<'a> {
    pub(super) fn new(picker: &'a EmojiPicker) -> Self {
        let columns = picker.columns.max(1);
        let searching = picker.searching();
        let mut starts = Vec::with_capacity(picker.sections.len() + 1);
        starts.push(0);
        let mut rows = 0;
        if searching {
            rows += picker.found.len().div_ceil(columns);
            starts.push(rows);
        } else {
            for section in &picker.sections {
                if !section.emoji.is_empty() {
                    rows += 1 + section.emoji.len().div_ceil(columns);
                }
                starts.push(rows);
            }
        }
        Self {
            picker,
            columns,
            searching,
            starts,
        }
    }

    /// How many rows there are, headings included.
    pub fn len(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The part a row is in (a section, or the matches), and how far into
    /// it.
    fn locate(&self, index: usize) -> Option<(usize, usize)> {
        if index >= self.len() {
            return None;
        }
        let part = self.starts.partition_point(|&start| start <= index) - 1;
        Some((part, index - self.starts[part]))
    }

    /// Row `index`.
    pub fn row(&self, index: usize) -> Option<Row<'a>> {
        let (part, within) = self.locate(index)?;
        if self.searching {
            return Some(Row::Emoji(
                &self.picker.found[self.cut(within, self.picker.found.len())],
            ));
        }
        let section = &self.picker.sections[part];
        match within {
            0 => Some(Row::Heading(part, section)),
            _ => Some(Row::Emoji(
                &section.emoji[self.cut(within - 1, section.emoji.len())],
            )),
        }
    }

    /// The emoji of the `nth` row of emoji in a part of `count`.
    fn cut(&self, nth: usize, count: usize) -> Range<usize> {
        let start = nth * self.columns;
        start..(start + self.columns).min(count)
    }

    /// Where row `index`'s emoji are: the section (none for the matches)
    /// and their indices in it. None for a heading.
    pub(super) fn span(&self, index: usize) -> Option<(Option<usize>, Range<usize>)> {
        let (part, within) = self.locate(index)?;
        if self.searching {
            Some((None, self.cut(within, self.picker.found.len())))
        } else if within == 0 {
            None
        } else {
            let count = self.picker.sections[part].emoji.len();
            Some((Some(part), self.cut(within - 1, count)))
        }
    }

    /// Row `index`'s key for the list: its section's, and its place in it.
    pub fn key(&self, index: usize) -> u64 {
        match self.locate(index) {
            Some((_, within)) if self.searching => FOUND | within as u64,
            Some((part, within)) => section_key(&self.picker.sections[part]) | within as u64,
            None => index as u64,
        }
    }

    /// The section row `index` is in; none while searching.
    pub fn section_of(&self, index: usize) -> Option<usize> {
        if self.searching {
            return None;
        }
        self.locate(index).map(|(part, _)| part)
    }

    /// The row of a section's heading, while it shows.
    pub fn heading(&self, section: usize) -> Option<usize> {
        let start = *self.starts.get(section)?;
        let shown = !self.searching && self.starts.get(section + 1).is_some_and(|&end| end > start);
        shown.then_some(start)
    }

    /// The key of a section's heading, which the rail's jump goes to: the
    /// same while a search shows, so the app can clear the search and jump.
    pub fn heading_key(&self, section: usize) -> Option<u64> {
        self.picker.sections.get(section).map(section_key)
    }
}

/// A section's key: its kind and what it is of, hashed, its low bits left
/// for its rows.
fn section_key(section: &Section) -> u64 {
    let mut hasher = DefaultHasher::new();
    match &section.icon {
        SectionIcon::Favorites => 0u8.hash(&mut hasher),
        SectionIcon::Frequent => 1u8.hash(&mut hasher),
        SectionIcon::Category(emoji) => (2u8, emoji.as_str()).hash(&mut hasher),
        SectionIcon::Server { id, .. } => (3u8, id).hash(&mut hasher),
    }
    let key = hasher.finish() & !WITHIN;
    if key == FOUND { 0 } else { key }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::Expected;

    fn picker(columns: usize) -> EmojiPicker {
        let unicode = |glyphs: &[&str]| -> Vec<Emoji> {
            glyphs
                .iter()
                .map(|glyph| Emoji::unicode(glyph).unwrap())
                .collect()
        };
        EmojiPicker {
            sections: vec![
                Section {
                    name: "Favorites".into(),
                    icon: SectionIcon::Favorites,
                    emoji: unicode(&["☠️"]),
                },
                Section {
                    name: "Empty".into(),
                    icon: SectionIcon::Server {
                        id: "1".into(),
                        picture: Expected::None,
                    },
                    emoji: Vec::new(),
                },
                Section {
                    name: "Frequently Used".into(),
                    icon: SectionIcon::Frequent,
                    emoji: unicode(&["🔥", "💯", "👍", "❤️", "😂"]),
                },
            ],
            found: unicode(&["🔥", "💯", "👍"]),
            columns,
            ..EmojiPicker::default()
        }
    }

    #[test]
    fn sections_are_a_heading_then_rows_of_emoji() {
        let picker = picker(2);
        let grid = picker.grid();
        // Favorites: heading, one row; the empty section: nothing;
        // Frequently Used: heading, three rows.
        assert_eq!(grid.len(), 6);
        assert!(matches!(grid.row(0), Some(Row::Heading(0, _))));
        assert!(matches!(grid.row(2), Some(Row::Heading(2, _))));
        let Some(Row::Emoji(last)) = grid.row(5) else {
            panic!("a row of emoji");
        };
        assert_eq!(last.len(), 1, "the last row holds what's left");
        assert_eq!(grid.row(6), None);
        assert_eq!(grid.section_of(4), Some(2));
        assert_eq!(grid.heading(2), Some(2));
        assert_eq!(grid.heading(1), None, "an empty section shows nothing");
        assert_eq!(grid.span(3), Some((Some(2), 0..2)));
    }

    #[test]
    fn a_search_shows_its_matches_under_no_heading() {
        let mut picker = picker(2);
        picker.query = "fire".into();
        let grid = picker.grid();
        assert_eq!(grid.len(), 2);
        assert!(matches!(grid.row(0), Some(Row::Emoji(row)) if row.len() == 2));
        assert_eq!(grid.section_of(0), None);
        assert_eq!(grid.heading(0), None);
        assert!(grid.heading_key(0).is_some(), "the rail still jumps");
        assert_eq!(picker.marked(), None);
    }

    #[test]
    fn rows_keep_their_keys_as_sections_come_before_them() {
        let picker = picker(2);
        let before = picker.grid().key(3);
        let mut more = picker.clone();
        more.sections.insert(
            0,
            Section {
                name: "Server".into(),
                icon: SectionIcon::Server {
                    id: "2".into(),
                    picture: Expected::Waiting,
                },
                emoji: picker.found.clone(),
            },
        );
        assert_eq!(more.grid().key(6), before);
        let grid = picker.grid();
        let keys: std::collections::HashSet<u64> = (0..grid.len()).map(|at| grid.key(at)).collect();
        assert_eq!(keys.len(), grid.len(), "every row its own key");
    }
}
