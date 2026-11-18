//! `LevelBar` (`ui/controls.slint:1798`): a microphone's level along a bar
//! from -80 to 0 dBFS in a control's face, green, orange from -18 dBFS and
//! red from -6, as the input sensitivity's.
//!
//! It draws the value the app hands it and nothing else: no animation, no
//! frame requested, no state. The meter's smoothing, if any, is the
//! projection's, so a quiet microphone costs no redraws here.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree};
use iced::border::Radius;
use iced::{Border, Color, Length, Rectangle, Size};

use anomi_theme::{Palette, Renderer, Theme, metrics};
use anomi_vm::kind::Level;

use super::track::level_at;

/// Where the bar turns orange and red, in dBFS.
pub const ORANGE_DB: f32 = -18.0;
pub const RED_DB: f32 = -6.0;

/// One run of the bar in one colour, as fractions of its width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub from: f32,
    pub to: f32,
    pub colour: Zone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Fine,
    Loud,
    Clipping,
}

impl Zone {
    fn colour(self, palette: &Palette) -> Color {
        match self {
            Self::Fine => palette.live,
            Self::Loud => palette.loud,
            Self::Clipping => palette.danger,
        }
    }
}

/// The coloured runs a level fills, the empty ones `None`. A fixed array: the
/// meter draws at the microphone's rate and must not allocate each time.
pub fn segments(level: f32) -> [Option<Segment>; 3] {
    let filled = level_at(level);
    let (orange, red) = (level_at(ORANGE_DB), level_at(RED_DB));
    [
        (0.0, filled.min(orange), Zone::Fine),
        (orange, filled.min(red).max(orange), Zone::Loud),
        (red, filled.max(red), Zone::Clipping),
    ]
    .map(|(from, to, colour)| (to > from).then_some(Segment { from, to, colour }))
}

/// Draws the bar into `bounds` (its height is the caller's: `METER_HEIGHT`).
/// The trough is rounded and the fill ends square inside it, as Slint's
/// `clip: true` leaves it, so a run takes the trough's curve only where it
/// touches an end.
pub fn draw(renderer: &mut Renderer, theme: &Theme, bounds: Rectangle, level: f32) {
    let palette = theme.palette();
    let radius = bounds.height / 2.0;
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border::default().rounded(radius),
            ..Quad::default()
        },
        palette.button_default,
    );
    for run in segments(level).into_iter().flatten() {
        let x = bounds.x + bounds.width * run.from;
        let width = bounds.width * (run.to - run.from);
        let mut corners = Radius::default();
        let round = radius.min(width / 2.0);
        if run.from <= 0.0 {
            corners = corners.left(round);
        }
        if run.to >= 1.0 {
            corners = corners.right(round);
        }
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(
                    iced::Point::new(x, bounds.y),
                    Size::new(width, bounds.height),
                ),
                border: Border {
                    radius: corners,
                    ..Border::default()
                },
                ..Quad::default()
            },
            run.colour.colour(palette),
        );
    }
}

/// A level to draw in dBFS: one given, or one the app stores as it measures
/// it, read as the widget draws, so a new level builds no view.
#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    Fixed(f32),
    Live(Level),
}

impl Reading {
    /// The level now, in dBFS.
    pub fn decibels(&self) -> f32 {
        match self {
            Self::Fixed(level) => *level,
            Self::Live(level) => level.decibels(),
        }
    }
}

/// A microphone's level, in dBFS, along a bar as wide as it is given.
pub struct LevelBar {
    level: Reading,
    width: Length,
}

/// A level bar showing `level` dBFS; below -80 it shows nothing.
pub fn level_bar(level: f32) -> LevelBar {
    LevelBar {
        level: Reading::Fixed(level),
        width: Length::Fill,
    }
}

impl LevelBar {
    /// A level bar reading `level` as it draws.
    pub fn live(level: &Level) -> Self {
        Self {
            level: Reading::Live(level.clone()),
            width: Length::Fill,
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
}

impl widget::Meta for LevelBar {}

impl<Message> widget::Widget<Message, Theme, Renderer> for LevelBar {
    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Fixed(metrics::METER_HEIGHT))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(
            self.width,
            Length::Fixed(metrics::METER_HEIGHT),
            Size::new(0.0, metrics::METER_HEIGHT),
        );
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        draw(renderer, theme, layout.bounds(), self.level.decibels());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zones(level: f32) -> Vec<Zone> {
        segments(level)
            .into_iter()
            .flatten()
            .map(|s| s.colour)
            .collect()
    }

    #[test]
    fn a_quiet_microphone_fills_nothing() {
        assert!(zones(-100.0).is_empty());
        assert!(zones(-80.0).is_empty());
    }

    #[test]
    fn colours_turn_at_18_and_6_below_full_scale() {
        assert_eq!(zones(-30.0), [Zone::Fine]);
        assert_eq!(zones(-18.0), [Zone::Fine]);
        assert_eq!(zones(-10.0), [Zone::Fine, Zone::Loud]);
        assert_eq!(zones(-3.0), [Zone::Fine, Zone::Loud, Zone::Clipping]);
        let full: Vec<Segment> = segments(0.0).into_iter().flatten().collect();
        assert_eq!(full.last().map(|s| s.to), Some(1.0));
        assert_eq!(full[0].to, level_at(ORANGE_DB));
    }
}
