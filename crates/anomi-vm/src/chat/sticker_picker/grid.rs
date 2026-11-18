//! The grid's rows: each section's heading, then its stickers as many to a
//! row as the picker's columns (`show_stickers`, `src/ui/stickers.rs`). The
//! Slint picker placed each tile at the top its adapter computed; the virtual
//! list measures the rows, so a row here is only what it holds, and the grid
//! knows where each section starts.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use super::{Section, Sticker, StickerPicker};

/// A row key's low bits count the rows within its section; the high bits are
/// the section's own, so a row keeps its key as sections come and go before
/// it.
const WITHIN: u64 = 0xF_FFFF;

/// A row of the grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row<'a> {
    /// A section's heading, with the section's index.
    Heading(usize, &'a Section),
    /// Up to a row's worth of stickers of section `section`; `last` for the
    /// section's last row, which the gap before the next heading follows.
    Stickers {
        section: usize,
        stickers: &'a [Sticker],
        last: bool,
    },
}

/// The grid's rows, as the picker's columns cut its sections.
pub struct Grid<'a> {
    picker: &'a StickerPicker,
    columns: usize,
    /// The first row of each section, and the count of rows after the last.
    starts: Vec<usize>,
}

impl<'a> Grid<'a> {
    pub(super) fn new(picker: &'a StickerPicker) -> Self {
        let columns = picker.columns.max(1);
        let mut starts = Vec::with_capacity(picker.sections.len() + 1);
        starts.push(0);
        let mut rows = 0;
        for section in &picker.sections {
            if !section.stickers.is_empty() {
                rows += 1 + section.stickers.len().div_ceil(columns);
            }
            starts.push(rows);
        }
        Self {
            picker,
            columns,
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

    /// The section a row is in, and how far into it.
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
        let section = &self.picker.sections[part];
        if within == 0 {
            return Some(Row::Heading(part, section));
        }
        let rows = section.stickers.len().div_ceil(self.columns);
        Some(Row::Stickers {
            section: part,
            stickers: &section.stickers[self.cut(within - 1, section.stickers.len())],
            last: within == rows,
        })
    }

    /// The stickers of the `nth` row of stickers in a section of `count`.
    fn cut(&self, nth: usize, count: usize) -> Range<usize> {
        let start = nth * self.columns;
        start..(start + self.columns).min(count)
    }

    /// Where row `index`'s stickers are: the section and their indices in
    /// it. None for a heading.
    pub fn span(&self, index: usize) -> Option<(usize, Range<usize>)> {
        let (part, within) = self.locate(index)?;
        if within == 0 {
            return None;
        }
        let count = self.picker.sections[part].stickers.len();
        Some((part, self.cut(within - 1, count)))
    }

    /// Row `index`'s key for the list: its section's, and its place in it.
    pub fn key(&self, index: usize) -> u64 {
        match self.locate(index) {
            Some((part, within)) => section_key(&self.picker.sections[part]) | within as u64,
            None => index as u64,
        }
    }

    /// The section row `index` is in.
    pub fn section_of(&self, index: usize) -> Option<usize> {
        self.locate(index).map(|(part, _)| part)
    }

    /// The key of a section's heading, which the rail's jump goes to.
    pub fn heading_key(&self, section: usize) -> Option<u64> {
        self.picker.sections.get(section).map(section_key)
    }
}

/// A section's key: its id hashed, its low bits left for its rows.
fn section_key(section: &Section) -> u64 {
    let mut hasher = DefaultHasher::new();
    section.id.hash(&mut hasher);
    hasher.finish() & !WITHIN
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(id: &str, count: usize) -> Section {
        Section {
            id: id.into(),
            name: id.into(),
            stickers: (0..count)
                .map(|n| Sticker {
                    id: format!("{id}{n}"),
                    usable: true,
                    ..Sticker::default()
                })
                .collect(),
            ..Section::default()
        }
    }

    fn picker(columns: usize) -> StickerPicker {
        StickerPicker {
            sections: vec![section("a", 3), section("b", 0), section("c", 5)],
            columns,
            ..StickerPicker::default()
        }
    }

    #[test]
    fn sections_are_a_heading_then_rows_of_stickers() {
        let picker = picker(2);
        let grid = picker.grid();
        // a: heading, two rows; b: nothing; c: heading, three rows.
        assert_eq!(grid.len(), 7);
        assert!(matches!(grid.row(0), Some(Row::Heading(0, _))));
        assert!(matches!(grid.row(3), Some(Row::Heading(2, _))));
        assert!(matches!(
            grid.row(2),
            Some(Row::Stickers { stickers, last: true, .. }) if stickers.len() == 1
        ));
        assert!(matches!(
            grid.row(4),
            Some(Row::Stickers { last: false, .. })
        ));
        assert_eq!(grid.row(7), None);
        assert_eq!(grid.section_of(5), Some(2));
        assert_eq!(grid.span(4), Some((2, 0..2)));
        assert_eq!(grid.span(3), None);
    }

    #[test]
    fn rows_keep_their_keys_as_sections_come_before_them() {
        let picker = picker(2);
        let before = picker.grid().key(5);
        let mut more = picker.clone();
        more.sections.insert(0, section("z", 4));
        assert_eq!(more.grid().key(5 + 3), before);
        let grid = picker.grid();
        let keys: std::collections::HashSet<u64> = (0..grid.len()).map(|at| grid.key(at)).collect();
        assert_eq!(keys.len(), grid.len(), "every row its own key");
    }
}
