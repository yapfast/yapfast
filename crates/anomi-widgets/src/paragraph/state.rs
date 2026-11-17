//! What a paragraph keeps between views: its blocks shaped, where each
//! lies, what's under the pointer, and a player for each of its server's
//! emoji. A text that comes back equal keeps its shapes; one laid out at
//! another width is laid out again, not shaped again.

use std::cell::RefCell;

use iced::advanced::graphics::text::Paragraph as Shape;
use iced::{Point, Rectangle, Size, Vector};

use anomi_vm::text::{Block, Level, Text};

use super::decorate::{self, Run};
use super::shape::{self, Inks};
use crate::picture::Player;

/// A quote's bar, and the space between it and the text
/// (`TextBlockView`, `ui/messages.slint:2058`, `:2067`).
pub const BAR: f32 = 4.0;
const BAR_GAP: f32 = 12.0;

/// One block shaped and placed.
pub struct Shaped {
    pub shape: Shape,
    /// The size its text is set at.
    pub size: f32,
    /// Its runs that draw something around them or open something, on the
    /// lines it shows.
    pub runs: Vec<Run>,
    /// Where its text starts in the paragraph, and the height it shows.
    pub origin: Point,
    pub shown: f32,
    /// Where it starts below its margin, and ends.
    pub top: f32,
    pub bottom: f32,
    /// Fenced code's box.
    pub boxed: Option<Rectangle>,
}

/// The shapes, and the inks they were shaped with: drawing in another
/// theme shapes them again, which `draw` does through the cell.
pub struct Shapes {
    pub inks: Inks,
    pub blocks: Vec<Shaped>,
}

/// A link under the pointer: its block and its index in the text's links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub block: usize,
    pub link: u32,
}

/// What shaping depends on besides the text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub size: f32,
    pub framed: bool,
    pub max_lines: Option<usize>,
    pub hint_factor: Option<f32>,
}

pub struct State {
    pub text: Text,
    pub look: Option<Look>,
    /// The width its text wraps at, and the width it draws across: less
    /// when it shrinks to its text, or when the room it has is endless.
    pub wrap: f32,
    pub width: f32,
    pub shapes: RefCell<Shapes>,
    /// Its whole height, and the height it shows, folded or not.
    pub full: f32,
    pub shown: f32,
    pub long: bool,
    pub hovered: Option<Target>,
    pub pressed: Option<Target>,
    /// Over a glyph, where a drag selects.
    pub over_text: bool,
    /// One for each of the text's pictures, by index.
    pub players: Vec<Player>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            text: Text::default(),
            look: None,
            wrap: 0.0,
            width: 0.0,
            shapes: RefCell::new(Shapes {
                inks: Inks::last(),
                blocks: Vec::new(),
            }),
            full: 0.0,
            shown: 0.0,
            long: false,
            hovered: None,
            pressed: None,
            over_text: false,
            players: Vec::new(),
        }
    }
}

/// How far a block's text is inset from the paragraph's left and right.
fn insets(block: &Block, size: f32) -> (f32, f32) {
    let left = if block.quoted { BAR + BAR_GAP } else { 0.0 };
    if block.level == Level::Code {
        let padding = size / 2.0;
        (left + padding, padding)
    } else {
        (left, 0.0)
    }
}

impl State {
    /// Shapes `text` when it, or how it's set, changed, and lays it out
    /// to wrap at `width` when that changed too.
    pub fn lay_out(&mut self, text: &Text, look: Look, width: f32) {
        let changed = self.look != Some(look) || (!self.text.same(text) && self.text != *text);
        if !self.text.same(text) {
            // The newest equal text, so the one before can go.
            self.text = text.clone();
        }
        let shapes = self.shapes.get_mut();
        if changed {
            shapes.blocks = shaped(text, look, width, shapes.inks);
            self.players
                .resize_with(text.pictures().len(), Player::default);
            self.players.truncate(text.pictures().len());
            self.hovered = None;
            self.pressed = None;
        } else if width != self.wrap {
            for ((block, spans), shaped) in text.blocks().zip(&mut shapes.blocks) {
                let (left, right) = insets(block, shaped.size);
                shape::resize(&mut shaped.shape, (width - left - right).max(0.0));
                shaped.runs = decorate::runs(&shaped.shape, spans, look.max_lines);
            }
        }
        self.look = Some(look);
        self.wrap = width;
    }

    /// Places its blocks across `width`, and gives the height of them all.
    pub fn arrange(&mut self, width: f32) -> f32 {
        let Some(look) = self.look else {
            return 0.0;
        };
        self.width = width;
        self.full = place(&self.text, &mut self.shapes.get_mut().blocks, look, width);
        self.full
    }

    /// Shapes the blocks again in `inks`, as the theme changed.
    pub fn reink(&self, inks: Inks) {
        let mut shapes = self.shapes.borrow_mut();
        if shapes.inks == inks {
            return;
        }
        shapes.inks = inks;
        let Some(look) = self.look else {
            return;
        };
        for ((block, spans), shaped) in self.text.blocks().zip(&mut shapes.blocks) {
            let (left, right) = insets(block, shaped.size);
            let width = (self.wrap - left - right).max(0.0);
            shaped.shape = shape::shape(
                &self.text,
                block,
                spans,
                shaped.size,
                width,
                inks,
                look.hint_factor,
            );
        }
    }

    /// The link at `point`, in the paragraph's own coordinates, within the
    /// height it shows.
    pub fn link_at(&self, point: Point) -> Option<Target> {
        if point.y > self.shown {
            return None;
        }
        let shapes = self.shapes.borrow();
        for ((_, spans), (index, shaped)) in
            self.text.blocks().zip(shapes.blocks.iter().enumerate())
        {
            let inside = point - Vector::new(shaped.origin.x, shaped.origin.y);
            if inside.y < -decorate::SIDE_PADDING || inside.y > shaped.shown {
                continue;
            }
            let found = shaped.runs.iter().find_map(|run| {
                let link = spans.get(run.span as usize)?.link?;
                run.target().contains(inside).then_some(link)
            });
            if let Some(link) = found {
                return Some(Target { block: index, link });
            }
        }
        None
    }

    /// Whether `point` is over one of its glyphs, where a drag selects.
    pub fn glyph_at(&self, point: Point) -> bool {
        use iced::advanced::text::Paragraph as _;
        let shapes = self.shapes.borrow();
        shapes.blocks.iter().any(|shaped| {
            let inside = point - Vector::new(shaped.origin.x, shaped.origin.y);
            inside.y >= 0.0 && inside.y <= shaped.shown && shaped.shape.hit_glyph(inside)
        })
    }
}

/// Every block of `text` shaped.
fn shaped(text: &Text, look: Look, width: f32, inks: Inks) -> Vec<Shaped> {
    text.blocks()
        .map(|(block, spans)| {
            let size = shape::block_size(block, look.size, look.framed);
            let (left, right) = insets(block, size);
            let shape = shape::shape(
                text,
                block,
                spans,
                size,
                (width - left - right).max(0.0),
                inks,
                look.hint_factor,
            );
            let runs = decorate::runs(&shape, spans, look.max_lines);
            Shaped {
                shape,
                size,
                runs,
                origin: Point::ORIGIN,
                shown: 0.0,
                top: 0.0,
                bottom: 0.0,
                boxed: None,
            }
        })
        .collect()
}

/// Places the blocks one under another, each below its margin, its text
/// inside its padding, its bar and its box; gives the height of them all.
fn place(text: &Text, blocks: &mut [Shaped], look: Look, width: f32) -> f32 {
    let mut y = 0.0;
    for ((block, _), shaped) in text.blocks().zip(blocks) {
        y += block.margin;
        shaped.top = y;
        let inner = y + block.padding;
        let (left, _) = insets(block, shaped.size);
        shaped.shown = decorate::height(&shaped.shape, look.max_lines);
        if block.level == Level::Code {
            let padding = shaped.size / 2.0;
            let x = left - padding;
            let boxed = Rectangle::new(
                Point::new(x, inner),
                Size::new((width - x).max(0.0), shaped.shown + 2.0 * padding),
            );
            shaped.origin = Point::new(left, inner + padding);
            shaped.boxed = Some(boxed);
            y = inner + boxed.height;
        } else {
            shaped.origin = Point::new(left, inner);
            shaped.boxed = None;
            y = inner + shaped.shown;
        }
        shaped.bottom = y;
    }
    y
}

/// The widest its blocks' text runs, with their insets: its width when it
/// shrinks to its text.
pub fn natural_width(text: &Text, blocks: &[Shaped]) -> f32 {
    use iced::advanced::text::Paragraph as _;
    text.blocks()
        .zip(blocks)
        .map(|((block, _), shaped)| {
            let (left, right) = insets(block, shaped.size);
            left + shaped.shape.min_bounds().width + right
        })
        .fold(0.0, f32::max)
}
