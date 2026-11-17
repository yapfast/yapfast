//! The mentions in the draft, drawn as its message will show them
//! (`vendor/i-slint-core/yapfast_marks.rs`): the text in its colour, set
//! through the editor's highlighting, on a translucent pill of it, as
//! `draw_mention_backgrounds` draws a mention
//! (`vendor/i-slint-core/textlayout/sharedparley/draw.rs:489`).

use std::ops::Range;

use iced::advanced::graphics::text::{cosmic_text, regions};
use iced::advanced::text::Parser;
use iced::{Color, Rectangle};

use super::offset;
use super::state::Field;

/// The pill's alpha of its mention's colour (`color.with-alpha(0.16)`).
pub(super) const ALPHA: f32 = 0.16;
/// Its corner (`RADIUS: 4.0`), the room either side of the text
/// (`PADDING: 2.0`), and how far it stays inside its line, of the line's
/// height, top and bottom (`INSET_RATIO: 0.08`).
pub(super) const RADIUS: f32 = 4.0;
const SIDE: f32 = 2.0;
const INSET: f32 = 0.08;

/// The marks set on the editor's text, and the parser that gives each line
/// its colours as the editor highlights it.
#[derive(Debug, Default)]
pub(super) struct Marker {
    /// The marks, their colours resolved, in the whole text's bytes.
    marks: Vec<(Range<usize>, Color)>,
    /// The version of the text they were split for.
    version: u64,
    /// Each line's parts of them, in the line's bytes.
    lines: Vec<Vec<(Range<usize>, Color)>>,
    /// The next line to colour: the editor colours from here down to the
    /// last line it shows.
    current: usize,
}

impl Marker {
    /// Drops the marks, as the editor's text is replaced: every line takes
    /// its plain colour again.
    pub fn forget(&mut self) {
        self.marks.clear();
        self.lines.clear();
        self.version = u64::MAX;
        self.current = 0;
    }

    /// Sets `marks`, coloured, for the text of `version`, unless they are
    /// set already; then the lines take their colours again.
    pub fn want(
        &mut self,
        marks: impl Iterator<Item = (Range<usize>, Color)> + Clone,
        version: u64,
        editor: &Field,
    ) {
        let same = self.marks.iter().cloned().eq(marks.clone());
        if same && (self.marks.is_empty() || version == self.version) {
            return;
        }
        self.marks = marks.collect();
        self.version = version;
        self.lines.clear();
        for (range, color) in &self.marks {
            for (line, part) in offset::split(editor, range.clone()) {
                if self.lines.len() <= line {
                    self.lines.resize_with(line + 1, Vec::new);
                }
                self.lines[line].push((part, *color));
            }
        }
        self.current = 0;
    }

    /// The pills under the marks, in the editor's place (its text's top
    /// left, scrolled), each with its colour at the pill's alpha.
    pub fn pills(&self, editor: &Field) -> Vec<(Rectangle, Color)> {
        let cursor = |at: usize| {
            let position = offset::position_of(editor, at);
            cosmic_text::Cursor::new(position.line, position.index)
        };
        let mut pills = Vec::new();
        for (range, color) in &self.marks {
            let fill = color.scale_alpha(ALPHA);
            for region in regions(editor.buffer(), cursor(range.start), cursor(range.end)) {
                let inset = region.height * INSET;
                let pill = Rectangle {
                    x: region.x - SIDE,
                    y: region.y + inset,
                    width: region.width + 2.0 * SIDE,
                    height: (region.height - 2.0 * inset).max(1.0),
                };
                pills.push((pill, fill));
            }
        }
        pills
    }

    /// Whether any mark is set.
    pub fn any(&self) -> bool {
        !self.marks.is_empty()
    }
}

impl Parser for Marker {
    type Settings = ();
    type Output = Color;
    type Iterator<'a> = std::iter::Cloned<std::slice::Iter<'a, (Range<usize>, Color)>>;

    fn new(_settings: &Self::Settings) -> Self {
        Self::default()
    }

    fn update(&mut self, _settings: &Self::Settings) {}

    fn change_line(&mut self, line: usize) {
        self.current = self.current.min(line);
    }

    fn parse_line(&mut self, _line: &str) -> Self::Iterator<'_> {
        let line = self.current;
        self.current += 1;
        self.lines
            .get(line)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .cloned()
    }

    fn current_line(&self) -> usize {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::text::editor::Editor as _;

    use super::*;

    const BLUE: Color = Color::from_rgb(0.3, 0.4, 0.9);

    #[test]
    fn each_line_gets_its_part_of_the_marks() {
        let editor = Field::with_text("hi @Bob\n@Carol x");
        let mut marker = Marker::default();
        let marks = [(3..7, BLUE), (8..14, BLUE)];
        marker.want(marks.iter().cloned(), 1, &editor);
        assert_eq!(
            marker.parse_line("hi @Bob").collect::<Vec<_>>(),
            [(3..7, BLUE)]
        );
        assert_eq!(
            marker.parse_line("@Carol x").collect::<Vec<_>>(),
            [(0..6, BLUE)]
        );
        assert_eq!(marker.current_line(), 2);
    }

    #[test]
    fn the_same_marks_colour_nothing_again_until_the_text_changes() {
        let editor = Field::with_text("@Bob");
        let mut marker = Marker::default();
        let marks = [(0..4, BLUE)];
        marker.want(marks.iter().cloned(), 1, &editor);
        let _ = marker.parse_line("@Bob");
        marker.want(marks.iter().cloned(), 1, &editor);
        assert_eq!(marker.current_line(), 1, "nothing to colour again");
        marker.want(marks.iter().cloned(), 2, &editor);
        assert_eq!(marker.current_line(), 0, "a new text splits them again");
        marker.change_line(0);
        marker.forget();
        assert!(!marker.any());
        assert_eq!(marker.parse_line("@Bob").count(), 0);
    }
}
