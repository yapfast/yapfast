//! A list's scroll bar: a thumb drawn over what the list holds, at its
//! right edge (its foot, for a list that scrolls across), taking no room
//! from it. Thin and faint until the pointer is over the list's edge or
//! drags it; then thicker and plainer.

use std::time::{Duration, Instant};

use iced::{Color, Point, Rectangle, Vector};

use super::flick::{Room, Rooms};

/// How much of the list's edge takes the pointer for the bar.
pub const EDGE: f32 = 12.0;
/// How thick the thumb is, at rest and under the pointer, and how far in
/// from the list's edge.
pub const THIN: f32 = 4.0;
pub const THICK: f32 = 8.0;
const INSET: f32 = 2.0;
/// The shortest a thumb gets, however much the list holds.
const SHORTEST: f32 = 24.0;
/// How long it takes to thicken or thin.
const GROWS: Duration = Duration::from_millis(150);

/// A scroll bar's inks: at rest, and under the pointer or dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scroller {
    pub rest: Color,
    pub pointed: Color,
}

impl Scroller {
    /// A dark theme's.
    pub const DARK: Self = Self {
        rest: Color::from_rgba(1.0, 1.0, 1.0, 0.16),
        pointed: Color::from_rgba(1.0, 1.0, 1.0, 0.45),
    };
    /// A light theme's.
    pub const LIGHT: Self = Self {
        rest: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
        pointed: Color::from_rgba(0.0, 0.0, 0.0, 0.55),
    };

    /// Its ink `grown` of the way from rest to pointed at.
    pub(super) fn ink(self, grown: f32) -> Color {
        let mix = |rest: f32, pointed: f32| rest + (pointed - rest) * grown;
        Color {
            r: mix(self.rest.r, self.pointed.r),
            g: mix(self.rest.g, self.pointed.g),
            b: mix(self.rest.b, self.pointed.b),
            a: mix(self.rest.a, self.pointed.a),
        }
    }
}

impl Default for Scroller {
    fn default() -> Self {
        Self::DARK
    }
}

/// Which way a bar's list scrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Axis {
    Down,
    Across,
}

/// A thumb held by the pointer: its list's axis, and how far along the
/// thumb the pointer has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Held {
    axis: Axis,
    grip: f32,
}

/// A bar from frame to frame.
#[derive(Default)]
pub(super) struct BarState {
    /// The list's room as of its last frame.
    pub rooms: Rooms,
    /// The pointer is over the list's edge, or holds a thumb.
    pub hot: bool,
    pub held: Option<Held>,
    /// How far it has thickened, from 0 to 1, and since when from where
    /// while it changes.
    pub grown: f32,
    changing: Option<(Instant, f32)>,
}

/// One bar's place in its list: the edge that takes the pointer, where
/// its thumb starts along it and how long the thumb is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Track {
    axis: Axis,
    edge: Rectangle,
    start: f32,
    length: f32,
    room: Room,
}

impl Track {
    /// How long the edge is, and where it starts in the window.
    fn span(&self) -> (f32, f32) {
        match self.axis {
            Axis::Down => (self.edge.y, self.edge.height),
            Axis::Across => (self.edge.x, self.edge.width),
        }
    }

    fn along(&self, point: Point) -> f32 {
        match self.axis {
            Axis::Down => point.y,
            Axis::Across => point.x,
        }
    }

    /// The thumb, `grown` of the way to its thickest.
    pub fn thumb(&self, grown: f32) -> Rectangle {
        let thick = THIN + (THICK - THIN) * grown;
        match self.axis {
            Axis::Down => Rectangle {
                x: self.edge.x + self.edge.width - INSET - thick,
                y: self.start,
                width: thick,
                height: self.length,
            },
            Axis::Across => Rectangle {
                x: self.start,
                y: self.edge.y + self.edge.height - INSET - thick,
                width: self.length,
                height: thick,
            },
        }
    }

    /// How far the list moves for the thumb to start `grip` before
    /// `pointer`, kept to its ends.
    fn pull(&self, pointer: Point, grip: f32) -> f32 {
        let (from, long) = self.span();
        let travel = long - self.length;
        if travel <= 0.0 {
            return 0.0;
        }
        let share = ((self.along(pointer) - grip - from) / travel).clamp(0.0, 1.0);
        share * self.room.max - self.room.offset
    }
}

/// The bars of a list in `bounds` with `rooms`: down its right edge and
/// across its foot, each where the list scrolls that way and holds more
/// than it shows.
pub(super) fn tracks(
    bounds: Rectangle,
    rooms: Rooms,
    down: bool,
    across: bool,
) -> [Option<Track>; 2] {
    // Under a pixel is a fraction's rounding, not something to scroll.
    let down = down && rooms.y.max > 1.0;
    let across = across && rooms.x.max > 1.0;
    let track = |axis, edge: Rectangle, room: Room| {
        let (from, long, shown) = match axis {
            Axis::Down => (edge.y, edge.height, bounds.height),
            Axis::Across => (edge.x, edge.width, bounds.width),
        };
        let length = (long * shown / (shown + room.max)).max(SHORTEST).min(long);
        let share = (room.offset / room.max).clamp(0.0, 1.0);
        Track {
            axis,
            edge,
            start: from + (long - length) * share,
            length,
            room,
        }
    };
    // Where both show, each stops short of the corner.
    let corner = |other: bool| if other { EDGE } else { 0.0 };
    [
        down.then(|| {
            let edge = Rectangle {
                x: bounds.x + bounds.width - EDGE,
                y: bounds.y,
                width: EDGE.min(bounds.width),
                height: (bounds.height - corner(across)).max(0.0),
            };
            track(Axis::Down, edge, rooms.y)
        }),
        across.then(|| {
            let edge = Rectangle {
                x: bounds.x,
                y: bounds.y + bounds.height - EDGE,
                width: (bounds.width - corner(down)).max(0.0),
                height: EDGE.min(bounds.height),
            };
            track(Axis::Across, edge, rooms.x)
        }),
    ]
}

impl BarState {
    /// The bar whose edge `pointer` is over.
    pub fn under(&self, pointer: Point, tracks: &[Option<Track>; 2]) -> Option<Track> {
        tracks
            .iter()
            .flatten()
            .find(|track| track.edge.contains(pointer))
            .copied()
    }

    /// A press on `track`: the thumb is held where it was pressed, or by
    /// its middle after coming to a press on the edge beside it. Says how
    /// far the list moves for that.
    pub fn press(&mut self, track: Track, pointer: Point) -> Vector {
        let along = track.along(pointer);
        let on_thumb = (track.start..=track.start + track.length).contains(&along);
        let grip = if on_thumb {
            along - track.start
        } else {
            track.length / 2.0
        };
        self.held = Some(Held {
            axis: track.axis,
            grip,
        });
        self.drag(pointer, &[Some(track), None])
    }

    /// How far the list moves for the held thumb to follow `pointer`.
    pub fn drag(&self, pointer: Point, tracks: &[Option<Track>; 2]) -> Vector {
        let Some(held) = self.held else {
            return Vector::ZERO;
        };
        let Some(track) = tracks
            .iter()
            .flatten()
            .find(|track| track.axis == held.axis)
        else {
            return Vector::ZERO;
        };
        let by = track.pull(pointer, held.grip);
        match held.axis {
            Axis::Down => Vector::new(0.0, by),
            Axis::Across => Vector::new(by, 0.0),
        }
    }

    /// Says whether the pointer is over the edge or holds a thumb now;
    /// true when that changed, and the bar starts to thicken or thin.
    pub fn point(&mut self, hot: bool, now: Instant) -> bool {
        if hot == self.hot {
            return false;
        }
        self.hot = hot;
        self.changing = Some((now, self.grown));
        true
    }

    /// A frame at `now`: how thick it is by then. True while it's still
    /// changing.
    pub fn frame(&mut self, now: Instant) -> bool {
        let Some((since, from)) = self.changing else {
            return false;
        };
        let t = (now.saturating_duration_since(since).as_secs_f32() / GROWS.as_secs_f32()).min(1.0);
        // Eased out.
        let eased = 1.0 - (1.0 - t) * (1.0 - t);
        let to = if self.hot { 1.0 } else { 0.0 };
        self.grown = from + (to - from) * eased;
        if t >= 1.0 {
            self.changing = None;
        }
        self.changing.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(offset: f32) -> [Option<Track>; 2] {
        // 400 px of a list of 4000, its thumb a tenth of its edge.
        let rooms = Rooms {
            x: Room::default(),
            y: Room {
                offset,
                max: 3600.0,
            },
        };
        tracks(
            Rectangle::new(Point::new(10.0, 20.0), iced::Size::new(300.0, 400.0)),
            rooms,
            true,
            false,
        )
    }

    #[test]
    fn the_thumb_lies_inside_the_lists_edge_and_thickens_towards_it() {
        let [Some(track), None] = list(0.0) else {
            panic!("one bar, down");
        };
        let thin = track.thumb(0.0);
        assert_eq!(
            (thin.x, thin.y, thin.width, thin.height),
            (304.0, 20.0, THIN, 40.0)
        );
        let thick = track.thumb(1.0);
        assert_eq!((thick.x, thick.width), (300.0, THICK));
        // At the list's end it's at the edge's.
        let [Some(track), None] = list(3600.0) else {
            panic!("one bar, down");
        };
        assert_eq!(track.thumb(0.0).y, 20.0 + 400.0 - 40.0);
    }

    #[test]
    fn a_list_that_fits_or_scrolls_the_other_way_has_no_bar() {
        let bounds = Rectangle::with_size(iced::Size::new(300.0, 400.0));
        let fits = Rooms {
            x: Room::default(),
            y: Room {
                offset: 0.0,
                max: 0.5,
            },
        };
        assert_eq!(tracks(bounds, fits, true, false), [None, None]);
        let long = Rooms {
            x: Room::default(),
            y: Room {
                offset: 0.0,
                max: 500.0,
            },
        };
        assert_eq!(tracks(bounds, long, false, true), [None, None]);
        // A very long list's thumb stays long enough to take.
        let endless = Rooms {
            x: Room::default(),
            y: Room {
                offset: 0.0,
                max: 1_000_000.0,
            },
        };
        let [Some(track), None] = tracks(bounds, endless, true, false) else {
            panic!("one bar, down");
        };
        assert_eq!(track.thumb(0.0).height, SHORTEST);
    }

    #[test]
    fn a_held_thumb_follows_the_pointer_and_a_press_beside_it_brings_it() {
        let mut bar = BarState::default();
        let tracks = list(0.0);
        let track = tracks[0].expect("down");
        // Pressed 10 px down the thumb, nothing moves.
        assert_eq!(bar.press(track, Point::new(305.0, 30.0)), Vector::ZERO);
        // 36 px down, a tenth of the thumb's 360 px of travel: a tenth of
        // the list's 3600 px.
        assert_eq!(
            bar.drag(Point::new(305.0, 66.0), &tracks),
            Vector::new(0.0, 360.0)
        );
        // Past the edge's end, to the list's end and no further.
        assert_eq!(
            bar.drag(Point::new(305.0, 900.0), &tracks),
            Vector::new(0.0, 3600.0)
        );
        // A press halfway down the edge brings the thumb's middle there.
        assert_eq!(
            bar.press(track, Point::new(305.0, 220.0)),
            Vector::new(0.0, 1800.0)
        );
    }

    #[test]
    fn it_thickens_over_a_moment_once_pointed_at() {
        let mut bar = BarState::default();
        let start = Instant::now();
        assert!(bar.point(true, start));
        assert!(!bar.point(true, start), "already");
        assert!(bar.frame(start + GROWS / 2));
        assert!(bar.grown > 0.5 && bar.grown < 1.0, "{}", bar.grown);
        assert!(!bar.frame(start + GROWS));
        assert_eq!(bar.grown, 1.0);
        // And thins from where it is when the pointer goes.
        assert!(bar.point(false, start + GROWS));
        assert!(!bar.frame(start + GROWS * 2));
        assert_eq!(bar.grown, 0.0);
    }
}
