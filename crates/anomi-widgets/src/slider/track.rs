//! Where a value falls along a slider's track, and where a point falls back
//! on the value: pure arithmetic, apart from the widget so it is tested alone.
//!
//! The Slint sliders (`ui/controls.slint`) do this in their `point` and `at`
//! functions: a position along the track from 0 to 1, rounded to a whole
//! value, with a pull toward a mark.

use iced::{Point, Rectangle};

use anomi_theme::metrics;

/// How far from a mark a drag lets go of the pointer's own value and lands
/// on the mark (`abs(value - 100) <= 4`, `VolumeSlider.point`).
pub const SNAP: i32 = 4;

/// What the upright track keeps clear at each end (`UprightVolumeSlider`'s
/// `y: 8px` and `height: parent.height - 16px`).
pub const UPRIGHT_INSET: f32 = 8.0;

/// The way a track runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Left to right across the widget's width, the volume's and the
    /// sensitivity's.
    Across,
    /// From the bottom up, the voice message player's.
    Up,
}

/// A whole-number range and what a position on the track means in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub min: i32,
    pub max: i32,
}

impl Span {
    pub fn clamp(self, value: i32) -> i32 {
        value.clamp(self.min, self.max)
    }

    /// Where a value falls, from 0 to 1 (`at` in `SensitivitySlider`,
    /// `value / maximum` in `VolumeSlider`).
    pub fn fraction(self, value: i32) -> f32 {
        let width = (self.max - self.min).max(1) as f32;
        ((value - self.min) as f32 / width).clamp(0.0, 1.0)
    }

    /// The whole value at a fraction of the track.
    pub fn value(self, fraction: f32) -> i32 {
        let width = (self.max - self.min) as f32;
        (fraction.clamp(0.0, 1.0) * width).round() as i32 + self.min
    }
}

/// The line a handle runs along inside a widget's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub axis: Axis,
    pub bounds: Rectangle,
}

impl Track {
    /// Where the track starts (value at 0) and its length.
    fn start(self) -> (Point, f32) {
        let b = self.bounds;
        match self.axis {
            Axis::Across => (Point::new(b.x, b.center_y()), b.width.max(1.0)),
            Axis::Up => (
                Point::new(b.center_x(), b.y + b.height - UPRIGHT_INSET),
                (b.height - 2.0 * UPRIGHT_INSET).max(1.0),
            ),
        }
    }

    /// The track's length in pixels.
    pub fn length(self) -> f32 {
        self.start().1
    }

    /// The point at a fraction of the track: the handle's centre.
    pub fn point(self, fraction: f32) -> Point {
        let (start, length) = self.start();
        match self.axis {
            Axis::Across => Point::new(start.x + length * fraction, start.y),
            Axis::Up => Point::new(start.x, start.y - length * fraction),
        }
    }

    /// The fraction a pointer at `at` stands over, kept within the track
    /// however far it strays.
    pub fn fraction(self, at: Point) -> f32 {
        let (start, length) = self.start();
        let along = match self.axis {
            Axis::Across => at.x - start.x,
            Axis::Up => start.y - at.y,
        };
        (along / length).clamp(0.0, 1.0)
    }

    /// The rail, `SLIDER_RAIL` thick, over the whole track.
    pub fn rail(self) -> Rectangle {
        self.filled(1.0)
    }

    /// The part of the rail up to a fraction.
    pub fn filled(self, fraction: f32) -> Rectangle {
        let (start, length) = self.start();
        let thick = metrics::SLIDER_RAIL;
        match self.axis {
            Axis::Across => Rectangle::new(
                Point::new(start.x, start.y - thick / 2.0),
                iced::Size::new(length * fraction, thick),
            ),
            Axis::Up => Rectangle::new(
                Point::new(start.x - thick / 2.0, start.y - length * fraction),
                iced::Size::new(thick, length * fraction),
            ),
        }
    }
}

/// The value a pointer at `at` asks for: the fraction rounded to a whole,
/// and a pull toward `snap` when there is one.
pub fn pick(track: Track, span: Span, snap: Option<i32>, at: Point) -> i32 {
    let value = span.value(track.fraction(at));
    match snap {
        Some(mark) if (value - mark).abs() <= SNAP => mark,
        _ => value,
    }
}

/// Where a microphone level in dBFS falls along a bar from -80 to 0, from 0
/// to 1 (`LevelBar.at`); below -80 shows nothing.
pub fn level_at(decibels: f32) -> f32 {
    ((decibels + 80.0) / 80.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACROSS: Track = Track {
        axis: Axis::Across,
        bounds: Rectangle {
            x: 10.0,
            y: 0.0,
            width: 200.0,
            height: 18.0,
        },
    };
    const UP: Track = Track {
        axis: Axis::Up,
        bounds: Rectangle {
            x: 0.0,
            y: 0.0,
            width: 24.0,
            height: 116.0,
        },
    };
    const VOLUME: Span = Span { min: 0, max: 200 };

    #[test]
    fn a_position_rounds_to_a_whole_value() {
        assert_eq!(pick(ACROSS, VOLUME, None, Point::new(10.0, 9.0)), 0);
        assert_eq!(pick(ACROSS, VOLUME, None, Point::new(110.0, 9.0)), 100);
        assert_eq!(pick(ACROSS, VOLUME, None, Point::new(60.4, 9.0)), 50);
        // Far past either end stays on it.
        assert_eq!(pick(ACROSS, VOLUME, None, Point::new(-500.0, 9.0)), 0);
        assert_eq!(pick(ACROSS, VOLUME, None, Point::new(900.0, 9.0)), 200);
    }

    #[test]
    fn a_drag_near_the_mark_lands_on_it() {
        for x in [106.0, 110.0, 114.0] {
            assert_eq!(pick(ACROSS, VOLUME, Some(100), Point::new(x, 9.0)), 100);
        }
        assert_eq!(pick(ACROSS, VOLUME, Some(100), Point::new(116.0, 9.0)), 106);
        assert_eq!(pick(ACROSS, VOLUME, Some(100), Point::new(60.0, 9.0)), 50);
    }

    #[test]
    fn the_sensitivity_runs_from_minus_80_to_0() {
        let span = Span { min: -80, max: 0 };
        assert_eq!(pick(ACROSS, span, None, Point::new(10.0, 9.0)), -80);
        assert_eq!(pick(ACROSS, span, None, Point::new(210.0, 9.0)), 0);
        assert_eq!(pick(ACROSS, span, None, Point::new(110.0, 9.0)), -40);
        assert_eq!(span.fraction(-60), 0.25);
    }

    #[test]
    fn the_upright_track_fills_from_the_bottom() {
        let span = Span { min: 0, max: 100 };
        // 116 tall with 8 clear at each end: the track runs from y 108 to 8.
        assert_eq!(pick(UP, span, None, Point::new(12.0, 108.0)), 0);
        assert_eq!(pick(UP, span, None, Point::new(12.0, 8.0)), 100);
        assert_eq!(pick(UP, span, None, Point::new(12.0, 58.0)), 50);
        assert_eq!(UP.point(0.5), Point::new(12.0, 58.0));
        let rail = UP.filled(0.25);
        assert_eq!((rail.y, rail.height, rail.width), (83.0, 25.0, 4.0));
    }

    #[test]
    fn levels_below_the_floor_show_nothing() {
        assert_eq!(level_at(-100.0), 0.0);
        assert_eq!(level_at(-80.0), 0.0);
        assert_eq!(level_at(-40.0), 0.5);
        assert_eq!(level_at(0.0), 1.0);
        assert_eq!(level_at(12.0), 1.0);
    }
}
