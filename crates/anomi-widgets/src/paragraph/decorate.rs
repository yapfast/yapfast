//! Where a shaped block's spans landed, for what draws around their glyphs
//! and what the pointer finds: each run of a span on a line that needs one,
//! as a link, a mention, a spoiler, inline code, a line through or under
//! it, or a server's emoji. Plain text keeps nothing here. Ports the boxes
//! Yapfast's patched Slint measured as it drew (`sharedparley/draw.rs`).

use iced::advanced::graphics::text::Paragraph as Shape;
use iced::advanced::text::Paragraph as _;
use iced::{Point, Rectangle, Size};

use anomi_vm::text::{Look, Span};

/// A span's glyphs on one line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    /// The span's index in its block.
    pub span: u32,
    pub left: f32,
    pub right: f32,
    /// The line's top and height, and its baseline.
    pub top: f32,
    pub height: f32,
    pub baseline: f32,
    /// The size its glyphs are set at.
    pub size: f32,
}

/// Twemoji's pictures, 72 pixels in a font of 53 pixels per em: each is
/// 1.36 em square, its top 1.02 em above the baseline, a pixel in from its
/// glyph's origin (`anomi_fonts::tables::TWEMOJI`). A server's emoji draws
/// in the same box.
const PICTURE_SIDE: f32 = 72.0 / 53.0;
const PICTURE_TOP: f32 = 54.0 / 53.0;
const PICTURE_LEFT: f32 = 1.0 / 53.0;

/// The space a mention's pill and a spoiler's cover take either side of
/// their text (`PADDING`, `sharedparley/draw.rs:451`, `:493`).
pub const SIDE_PADDING: f32 = 2.0;

impl Run {
    /// The line's box around the run.
    pub fn line_box(&self) -> Rectangle {
        Rectangle::new(
            Point::new(self.left, self.top),
            Size::new(self.right - self.left, self.height),
        )
    }

    /// What the pointer finds of it: a pill's or a cover's padding too.
    pub fn target(&self) -> Rectangle {
        self.line_box().expand([0.0, SIDE_PADDING])
    }

    /// The box a server's emoji draws in.
    pub fn picture(&self) -> Rectangle {
        let side = self.size * PICTURE_SIDE;
        Rectangle::new(
            Point::new(
                self.left + self.size * PICTURE_LEFT,
                self.baseline - self.size * PICTURE_TOP,
            ),
            Size::new(side, side),
        )
    }
}

/// Whether a span needs its runs kept.
fn kept(span: &Span) -> bool {
    let style = span.style;
    span.link.is_some() || style.code || style.underline || style.strike || span.look != Look::Text
}

/// The runs of `shape`'s spans that need them, line by line, on its first
/// `lines` lines or on all of them: a block clamped to a few lines keeps
/// none past them, so nothing draws, plays or is found below its cut, as
/// Slint stopped at its last line drawn (`sharedparley/draw.rs:175`).
pub fn runs(shape: &Shape, spans: &[Span], lines: Option<usize>) -> Vec<Run> {
    let scale = 1.0 / shape.hint_factor().unwrap_or(1.0);
    let mut runs = Vec::new();
    if !spans.iter().any(kept) {
        return runs;
    }
    for line in shape
        .buffer()
        .layout_runs()
        .take(lines.unwrap_or(usize::MAX))
    {
        let mut open: Option<Run> = None;
        for glyph in line.glyphs {
            let index = glyph.metadata;
            if !spans.get(index).is_some_and(kept) {
                runs.extend(open.take());
                continue;
            }
            let (left, right) = (glyph.x * scale, (glyph.x + glyph.w) * scale);
            match open.as_mut() {
                Some(run) if run.span as usize == index => {
                    run.left = run.left.min(left);
                    run.right = run.right.max(right);
                }
                _ => {
                    runs.extend(open.take());
                    open = Some(Run {
                        span: index as u32,
                        left,
                        right,
                        top: line.line_top * scale,
                        height: line.line_height * scale,
                        baseline: line.line_y * scale,
                        size: glyph.font_size * scale,
                    });
                }
            }
        }
        runs.extend(open);
    }
    runs
}

/// The height of `shape`'s first `lines` lines, or of all of them.
pub fn height(shape: &Shape, lines: Option<usize>) -> f32 {
    let scale = 1.0 / shape.hint_factor().unwrap_or(1.0);
    match lines {
        None => shape.min_bounds().height,
        Some(lines) => {
            shape
                .buffer()
                .layout_runs()
                .take(lines)
                .map(|line| line.line_height)
                .sum::<f32>()
                * scale
        }
    }
}

/// Runs of one link on one line, side by side, as one box: a pill or a
/// cover across a mention's bold part and its plain one is one pill.
pub fn joined<'a>(
    runs: impl Iterator<Item = &'a Run>,
    link: impl Fn(&Run) -> Option<u32>,
) -> Vec<(Option<u32>, Rectangle)> {
    let mut joined: Vec<(Option<u32>, Rectangle)> = Vec::new();
    for run in runs {
        let this = link(run);
        let area = run.line_box();
        match joined.last_mut() {
            Some((last, joined))
                if this.is_some()
                    && *last == this
                    && joined.y == area.y
                    && (joined.x + joined.width - area.x).abs() < 0.5 =>
            {
                *joined = joined.union(&area);
            }
            _ => joined.push((this, area)),
        }
    }
    joined
}
