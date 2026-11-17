//! The effects drawn from a name's outlines, as `StyledName` stacks its
//! texts (`ui/controls.slint:761-824`): gradients across the name's box,
//! and edges drawn as the Slint renderer draws a text's stroke
//! (`sharedparley/draw.rs:798`): an edge outside the letters is a stroke
//! twice as wide under them, with square ends and mitred corners. Discord's
//! edges and offsets scale with the text: 0.04em and so on.
//!
//! Neon's halo isn't drawn: `StyledName` stacks it as a text with a clear
//! fill and a stroke, and the Slint renderer draws nothing of a text whose
//! fill is clear (`sharedparley.rs:234`), so it never showed; the
//! reference shots have none.

use iced::advanced::text::Shaping;
use iced::widget::canvas::{Fill, Frame, LineCap, LineJoin, Stroke, Style, Text, gradient::Linear};
use iced::{Color, Pixels, Point, Size, Vector};

use anomi_theme::{Renderer, fonts};

use super::colours::Colours;
use super::outline::Glyphs;
use super::style::Effect;

/// How far an effect reaches past the name's box at most, with a pixel to
/// spare: pop's lower copy, `0.12em + 1.2px` down, with its edge.
pub(super) fn reach(em: f32) -> f32 {
    em * 0.16 + 2.4 + 1.0
}

/// What an effect draws with.
pub(super) struct Paint<'a> {
    pub glyphs: &'a Glyphs,
    pub colours: &'a Colours,
    /// The name's box, whose width a gradient spans and whose height
    /// toon's bands do.
    pub size: Size,
    /// The text's size.
    pub em: f32,
    pub line_height: f32,
    /// What each fill is faded to: the name's opacity, or 1 for a name
    /// faded as a whole (`stacked`).
    pub opacity: f32,
}

impl Paint<'_> {
    fn faded(&self, color: Color) -> Color {
        color.scale_alpha(self.opacity)
    }

    fn fill(&self, frame: &mut Frame<Renderer>, style: impl Into<Style>) {
        let fill = Fill {
            style: style.into(),
            ..Fill::default()
        };
        frame.fill(&self.glyphs.path, fill);
    }

    /// An edge `width` wide outside the letters: a stroke twice that under
    /// where they'll be filled.
    fn outside(&self, frame: &mut Frame<Renderer>, color: Color, width: f32) {
        frame.stroke(
            &self.glyphs.path,
            Stroke {
                style: Style::Solid(self.faded(color)),
                width: width * 2.0,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                ..Stroke::default()
            },
        );
    }

    /// Across the name's width, left to right.
    fn across(&self, stops: impl IntoIterator<Item = (f32, Color)>) -> Style {
        let line = Linear::new(Point::ORIGIN, Point::new(self.size.width, 0.0));
        Style::Gradient(self.stops(line, stops).into())
    }

    /// Down the name's height, top to bottom.
    fn down(&self, stops: impl IntoIterator<Item = (f32, Color)>) -> Style {
        let line = Linear::new(Point::ORIGIN, Point::new(0.0, self.size.height));
        Style::Gradient(self.stops(line, stops).into())
    }

    fn stops(&self, line: Linear, stops: impl IntoIterator<Item = (f32, Color)>) -> Linear {
        stops.into_iter().fold(line, |line, (offset, color)| {
            line.add_stop(offset, self.faded(color))
        })
    }
}

/// White, as neon's and pop's letters are.
const WHITE: Color = Color::WHITE;

/// Draws `effect` into `frame`, the name's box at its origin, then the
/// name's emoji over it.
pub(super) fn effect(frame: &mut Frame<Renderer>, effect: Effect, paint: &Paint<'_>) {
    let colours = paint.colours;
    let em = paint.em;
    let given = &colours.colors[..colours.count.clamp(1, 5)];
    match effect {
        // The colours across the name, from 10% to 90%.
        Effect::Gradient | Effect::Glow if given.len() > 1 => {
            let last = (given.len() - 1) as f32;
            let stops = given
                .iter()
                .enumerate()
                .map(|(index, color)| (0.1 + 0.8 * index as f32 / last, *color));
            paint.fill(frame, paint.across(stops));
        }
        // Round once, back to the first.
        Effect::Prism if given.len() > 1 => {
            let count = given.len() as f32;
            let stops = given
                .iter()
                .enumerate()
                .map(|(index, color)| (index as f32 / count, *color))
                .chain([(1.0, given[0])]);
            paint.fill(frame, paint.across(stops));
        }
        // White letters edged in a lighter shade of the colour.
        Effect::Neon => {
            paint.outside(frame, colours.neon, em * 0.04 + 1.0);
            paint.fill(frame, paint.faded(WHITE));
        }
        // The light shades in bands, edged in the dark one.
        Effect::Toon => {
            paint.outside(frame, colours.toon, em * 0.04 + 1.6);
            let bands = [
                (0.0, colours.light2),
                (0.45, colours.light1),
                (0.55, colours.main),
                (1.0, colours.dark1),
            ];
            paint.fill(frame, paint.down(bands));
        }
        // White letters edged in the darkest shade, over a copy in the
        // colour a little lower, which looks like a shadow.
        Effect::Pop => {
            let edge = em * 0.04 + 1.2;
            frame.with_save(|frame| {
                frame.translate(Vector::new(0.0, em * 0.12 + 1.2));
                paint.outside(frame, colours.dark2, edge);
                paint.fill(frame, paint.faded(colours.main));
            });
            paint.outside(frame, colours.dark2, edge);
            paint.fill(frame, paint.faded(WHITE));
        }
        // One colour: drawn as plain text, so only reached when a
        // two-colour effect has one.
        Effect::Solid | Effect::Gradient | Effect::Glow | Effect::Prism | Effect::Gummy => {
            paint.fill(frame, paint.faded(given[0]));
        }
    }
    for emoji in &paint.glyphs.emoji {
        frame.fill_text(Text {
            content: emoji.text.clone(),
            position: emoji.at,
            color: paint.faded(WHITE),
            size: Pixels(em),
            line_height: Pixels(paint.line_height).into(),
            font: fonts::EMOJI,
            shaping: Shaping::Advanced,
            ..Text::default()
        });
    }
}

/// Whether `effect` draws one thing over another, an edge or a copy under
/// its letters, which fading each alone lets through: such a name is faded
/// as a whole.
pub(super) fn stacked(effect: Effect) -> bool {
    matches!(effect, Effect::Neon | Effect::Toon | Effect::Pop)
}

/// Whether `effect` with `count` colours is drawn from outlines; the rest
/// are plain text in one colour, or letters.
pub(super) fn outlined(effect: Effect, count: usize) -> bool {
    match effect {
        Effect::Gradient | Effect::Glow | Effect::Prism => count > 1,
        Effect::Neon | Effect::Toon | Effect::Pop => count > 0,
        Effect::Solid | Effect::Gummy => false,
    }
}
