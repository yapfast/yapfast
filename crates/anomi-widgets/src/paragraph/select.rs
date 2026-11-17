//! Selecting a block's text through iced's text operations, as the window
//! selects across widgets while the pointer drags: each block is a text of
//! its own, so a drag across a quote and the text under it selects both,
//! and copying joins them by line breaks. What's copied reads as typed: an
//! emoji's glyph as the emoji, a server's emoji's place as its name, each
//! found by the span its glyph was shaped from, not by the glyph.

use iced::advanced::text::{Fragment, Paragraph as _, Target};
use iced::advanced::widget::operation;
use iced::{Point, Vector};

use anomi_vm::text::{Look, Span, Text};

use super::shape::is_glyph;
use super::state::Shaped;

/// A block as the text operations see it.
pub struct Operand<'a> {
    pub shaped: &'a mut Shaped,
    pub text: &'a Text,
    pub spans: &'a [Span],
    /// Where its text starts in the window.
    pub origin: Vector,
    pub selectable: bool,
}

impl operation::Text for Operand<'_> {
    fn text(&self) -> Fragment<'_> {
        let mut text = String::new();
        for span in self.spans {
            text.push_str(self.text.chars(span));
        }
        text.into()
    }

    fn select(&mut self, start: Point, end: Point, target: Target) {
        if self.selectable {
            self.shaped
                .shape
                .select(start - self.origin, end - self.origin, target);
        }
    }

    fn select_all(&mut self) {
        if self.selectable {
            self.shaped.shape.select_all();
        }
    }

    fn deselect(&mut self) {
        self.shaped.shape.deselect();
    }

    fn copy(&mut self) -> Option<String> {
        let copied = self.shaped.shape.copy()?;
        if !copied.chars().any(is_glyph) {
            return Some(copied);
        }
        let mut glyphs = self.selected_glyphs().into_iter();
        let mut typed = String::with_capacity(copied.len());
        for character in copied.chars() {
            if !is_glyph(character) {
                typed.push(character);
                continue;
            }
            let span = glyphs
                .next()
                .filter(|(glyph, _)| *glyph == character)
                .and_then(|(_, span)| self.spans.get(span))
                .filter(|span| matches!(span.look, Look::Emoji | Look::Picture(_)));
            match span {
                // An emoji's characters as typed; a server's emoji's, its name.
                Some(span) => typed.push_str(self.text.chars(span)),
                None => typed.push(character),
            }
        }
        Some(typed)
    }
}

impl Operand<'_> {
    /// The selected glyphs that stand for an emoji, in the order they're
    /// read, each with the index of the span it's of: those under the
    /// selection's highlight on their line.
    fn selected_glyphs(&self) -> Vec<(char, usize)> {
        let shape = &self.shaped.shape;
        let regions = shape.selection();
        let mut found = Vec::new();
        for run in shape.buffer().layout_runs() {
            let lit: Vec<_> = regions
                .iter()
                .filter(|region| (region.y - run.line_top).abs() < 0.5)
                .collect();
            if lit.is_empty() {
                continue;
            }
            for glyph in run.glyphs {
                let Some(character) = run
                    .text
                    .get(glyph.start..glyph.end)
                    .and_then(|cluster| cluster.chars().next())
                    .filter(|character| is_glyph(*character))
                else {
                    continue;
                };
                let middle = glyph.x + glyph.w / 2.0;
                if lit
                    .iter()
                    .any(|region| region.x <= middle && middle <= region.x + region.width)
                {
                    found.push((run.line_i, glyph.start, character, glyph.metadata));
                }
            }
        }
        found.sort_unstable_by_key(|&(line, start, ..)| (line, start));
        found
            .into_iter()
            .map(|(_, _, character, span)| (character, span))
            .collect()
    }
}
