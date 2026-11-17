//! Between the editor's places (a line and a byte in it) and byte offsets
//! in the whole text, which the app speaks: the caret it's told of
//! (`cursor-position-byte-offset`), the caret it places
//! (`set-selection-offsets`), the marks' ranges. The whole text is the lines
//! joined by their endings, a line without one by `\n`, as the editor's
//! `text()` joins them.

use std::ops::Range;

use iced::advanced::text::Position;
use iced::advanced::text::editor::{Editor as _, LineEnding};

use super::state::Field;

/// The byte offset of `position` in the whole text.
pub(super) fn offset_of(editor: &Field, position: Position) -> usize {
    let mut start = 0;
    for (index, line) in editor.lines().enumerate() {
        if index == position.line {
            return start + position.index.min(line.text.len());
        }
        start += line.text.len() + ending(line.ending);
    }
    start
}

/// The place of a byte offset in the whole text: its line, and the byte in
/// it, moved back to a character's start. Past the end, the end.
pub(super) fn position_of(editor: &Field, offset: usize) -> Position {
    let mut start = 0;
    let mut last = Position { line: 0, index: 0 };
    for (index, line) in editor.lines().enumerate() {
        let end = start + line.text.len();
        if offset <= end {
            return Position {
                line: index,
                index: line.text.floor_char_boundary(offset - start),
            };
        }
        last = Position {
            line: index,
            index: line.text.len(),
        };
        start = end + ending(line.ending);
        if offset < start {
            // Inside a line's ending: at the line's end.
            return last;
        }
    }
    last
}

/// The end of the text: its last line's end.
pub(super) fn end_of(editor: &Field) -> Position {
    let lines = editor.line_count();
    let line = lines.saturating_sub(1);
    Position {
        line,
        index: editor.line(line).map_or(0, |text| text.text.len()),
    }
}

/// The parts of the whole-text `range` on each line it touches: the line,
/// and the range within it. Nothing for an empty range.
pub(super) fn split(editor: &Field, range: Range<usize>) -> Vec<(usize, Range<usize>)> {
    let mut parts = Vec::new();
    if range.is_empty() {
        return parts;
    }
    let mut start = 0;
    for (index, line) in editor.lines().enumerate() {
        let end = start + line.text.len();
        let from = range.start.max(start);
        let to = range.end.min(end);
        if from < to {
            parts.push((index, from - start..to - start));
        }
        start = end + ending(line.ending);
        if start >= range.end {
            break;
        }
    }
    parts
}

/// The bytes a line's ending takes in the whole text.
fn ending(ending: LineEnding) -> usize {
    match ending {
        LineEnding::None => LineEnding::default().as_str().len(),
        ending => ending.as_str().len(),
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::text::editor::Editor as _;

    use super::*;

    #[test]
    fn offsets_count_bytes_across_lines() {
        let editor = Field::with_text("hé\nжук @bo");
        let at = |line, index| Position { line, index };
        assert_eq!(offset_of(&editor, at(0, 0)), 0);
        assert_eq!(offset_of(&editor, at(0, 3)), 3);
        assert_eq!(offset_of(&editor, at(1, 0)), 4);
        assert_eq!(offset_of(&editor, at(1, 7)), 11);
        assert_eq!(offset_of(&editor, end_of(&editor)), "hé\nжук @bo".len());
    }

    #[test]
    fn positions_land_on_characters() {
        let editor = Field::with_text("hé\nжук");
        let at = |line, index| Position { line, index };
        assert_eq!(position_of(&editor, 2), at(0, 1), "inside é");
        assert_eq!(position_of(&editor, 3), at(0, 3));
        assert_eq!(position_of(&editor, 4), at(1, 0));
        assert_eq!(position_of(&editor, 6), at(1, 2));
        assert_eq!(position_of(&editor, 1000), at(1, 6), "past the end");
        for offset in [0, 1, 3, 4, 6, 8, 10] {
            assert_eq!(offset_of(&editor, position_of(&editor, offset)), offset);
        }
    }

    #[test]
    fn a_range_splits_at_line_ends() {
        let editor = Field::with_text("ab\ncd\nef");
        assert_eq!(split(&editor, 1..7), [(0, 1..2), (1, 0..2), (2, 0..1)]);
        assert_eq!(split(&editor, 3..5), [(1, 0..2)]);
        assert!(split(&editor, 4..4).is_empty());
    }
}
