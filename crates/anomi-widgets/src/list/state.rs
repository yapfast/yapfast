//! Where the list is and what it built. Its place is a row's, by key, not a
//! pixel offset into rows it never measured: rows that go in or grow above
//! the view leave what shows where it is. The Slint side: the anchoring in
//! `vendor/i-slint-core/model/repeater.rs` (`YapfastHold`, `YapfastAnchor`)
//! and `yapfast_anchor.rs`, and the timeline's `follow-latest` in
//! `ui/yapfast.slint`.

use std::time::Instant;

use iced::advanced::widget::{Id, Tree};

use super::autoscroll::Autoscroll;
use super::drag::Gesture;
use super::ghost::Faded;
use super::glide::{Glide, Wheel};
use super::heights::Heights;
use super::place::{Frame, Placed};
use super::report::Reports;
use crate::scrollbar::{Bar, Extent};

/// Where the list holds its rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
    /// At its start: rows that go in at the top show.
    Start,
    /// At its end, following it: rows that come or grow there stay in view.
    End,
    /// A row stays where it is in the view.
    Row(Anchor),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub key: u64,
    /// Where it was, to find it again quickly.
    pub index: usize,
    /// Its top, from the list's top, and its height, as last placed.
    pub top: f32,
    pub height: f32,
    /// The key of the row before it, if known (`Some(None)` for the first).
    pub before: Option<Option<u64>>,
    /// Rows went in or out right above it: it keeps its bottom edge where it
    /// was, as its top part (a date, a loading notice, an author line) can
    /// change with the rows now before it.
    pub keep_bottom: bool,
}

/// Where the next layout puts a row, asked from outside: an operation, the
/// scrollbar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request {
    /// A row, by key, once it's in the list.
    Jump {
        key: u64,
        place: super::Place,
    },
    Start,
    End,
    /// Scrolls by this many pixels, towards the end when positive.
    By(f32),
    /// Scrolls to this offset from the start, by the rows' estimates.
    Offset(f32),
}

/// The rows built at the last layout, in order from `first`; the widget's
/// tree has one child for each.
#[derive(Clone, Debug, Default)]
pub struct Built {
    pub first: usize,
    pub keys: Vec<u64>,
    pub tops: Vec<f32>,
    pub heights: Vec<f32>,
    /// The list's row count then.
    pub count: usize,
}

impl Built {
    pub fn last(&self) -> usize {
        self.first + self.keys.len().saturating_sub(1)
    }

    pub fn bottom(&self) -> f32 {
        match (self.tops.last(), self.heights.last()) {
            (Some(top), Some(height)) => top + height,
            _ => 0.0,
        }
    }
}

/// What a scroll did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scrolled {
    /// Nothing: the list is at that end, or holds no rows.
    Blocked,
    /// It moved within the rows built.
    Moved,
    /// It moved past them: lay out again before drawing.
    Gap,
}

pub struct State {
    pub id: Option<Id>,
    pub position: Position,
    pub request: Option<Request>,
    /// A request came since the last layout: the next frame lays out. One
    /// waiting for its row waits for the next view instead.
    pub asked: bool,
    pub built: Built,
    /// How far the rows moved up since they were laid out.
    pub shift: f32,
    pub frame: Frame,
    pub heights: Heights,
    /// The list's switches as the last layout had them.
    pub from_end: bool,
    pub follow: bool,
    pub glide: Option<Glide>,
    pub wheel: Wheel,
    pub bar: Bar,
    pub autoscroll: Option<Autoscroll>,
    pub gesture: Gesture,
    /// A drag whose row left the list, from its last place: the owner hears
    /// it went back at the next event.
    pub lost: Option<usize>,
    pub reports: Reports,
    /// The dragged row, built apart while it's out of the rows built.
    pub ghost: Option<Tree>,
    /// The theme the dragged row's copy is drawn with, made once a drag.
    pub faded: Faded,
    /// The row kept in view, and its height when it was last revealed.
    pub revealed: Option<(u64, f32)>,
    /// The time of the last frame a drag scrolled the list at its edge.
    pub drag_frame: Option<Instant>,
    /// Where the rows lay ([`State::lie`]) when they last heard the pointer.
    pub pointed: Option<(Option<u64>, f32)>,
    /// The pointer was in the list at its last move.
    pub inside: bool,
}

impl State {
    pub fn new(id: Option<Id>, estimate: f32, from_end: bool, follow: bool) -> Self {
        Self {
            id,
            position: if from_end && follow {
                Position::End
            } else {
                Position::Start
            },
            request: None,
            asked: false,
            built: Built::default(),
            shift: 0.0,
            frame: Frame {
                view: 0.0,
                start: 0.0,
                end: 0.0,
                margin: 0.0,
                gap: 0.0,
                end_aligned: from_end,
            },
            heights: Heights::new(estimate),
            from_end,
            follow,
            glide: None,
            wheel: Wheel::default(),
            bar: Bar::default(),
            autoscroll: None,
            gesture: Gesture::Idle,
            lost: None,
            reports: Reports::default(),
            ghost: None,
            faded: Faded::default(),
            revealed: None,
            drag_frame: None,
            pointed: None,
            inside: false,
        }
    }

    pub fn following(&self) -> bool {
        self.position == Position::End
    }

    /// Where the rows lie: the first built row's key and its top now.
    pub fn lie(&self) -> (Option<u64>, f32) {
        let top = self.built.tops.first().map_or(0.0, |top| top - self.shift);
        (self.built.keys.first().copied(), top)
    }

    /// Row `at` of those built: its top now, and its height.
    pub fn row(&self, at: usize) -> (f32, f32) {
        (self.built.tops[at] - self.shift, self.built.heights[at])
    }

    /// Takes in a layout's rows.
    pub fn placed(&mut self, placed: Placed, keys: Vec<u64>, count: usize, frame: Frame) {
        let anchored = match self.position {
            Position::Row(anchor) => placed.row(anchor.index),
            _ => None,
        };
        self.built = Built {
            first: placed.first,
            keys,
            tops: placed.tops,
            heights: placed.heights,
            count,
        };
        self.shift = 0.0;
        self.frame = frame;
        let lost = match &mut self.position {
            Position::Row(anchor) => match anchored {
                Some((top, height)) => {
                    anchor.top = top;
                    anchor.height = height;
                    false
                }
                None => true,
            },
            Position::Start => self.from_end,
            Position::End => !(self.from_end && self.follow),
        };
        if lost {
            self.reanchor();
        }
        self.settle();
    }

    /// The person moved the list: the first row in view holds it now.
    pub fn reanchor(&mut self) {
        let built = &self.built;
        if built.keys.is_empty() {
            return;
        }
        let at = (0..built.keys.len())
            .find(|at| built.tops[*at] + built.heights[*at] - self.shift > 0.0)
            .unwrap_or(0);
        let index = built.first + at;
        let before = match (at, index) {
            (_, 0) => Some(None),
            (0, _) => None,
            _ => Some(Some(built.keys[at - 1])),
        };
        let (top, height) = self.row(at);
        self.position = Position::Row(Anchor {
            key: built.keys[at],
            index,
            top,
            height,
            before,
            keep_bottom: false,
        });
    }

    /// At an end it follows, the list holds to it again.
    fn settle(&mut self) {
        let built = &self.built;
        if built.keys.is_empty() {
            return;
        }
        let at_start = built.first == 0 && built.tops[0] - self.shift >= self.frame.start - 0.5;
        if self.from_end && self.follow && self.at_end() {
            self.position = Position::End;
        } else if !self.from_end && at_start {
            self.position = Position::Start;
        }
    }

    /// The last row's bottom is at the view's end.
    pub fn at_end(&self) -> bool {
        let built = &self.built;
        !built.keys.is_empty()
            && built.last() + 1 == built.count
            && built.bottom() - self.shift <= self.frame.end + 0.5
    }

    /// Scrolls by `delta`, towards the end when positive, as the person does:
    /// it stops following, held inside the ends the rows built show.
    pub fn scroll(&mut self, delta: f32) -> Scrolled {
        let built = &self.built;
        if built.keys.is_empty() {
            return Scrolled::Blocked;
        }
        let frame = self.frame;
        let top = built.tops[0] - self.shift;
        let bottom = built.bottom() - self.shift;
        let whole = built.first == 0 && built.last() + 1 == built.count;
        if whole && bottom - top <= frame.end - frame.start + 0.5 {
            return Scrolled::Blocked;
        }
        let low = if built.first == 0 {
            (top - frame.start).min(0.0)
        } else {
            f32::NEG_INFINITY
        };
        let high = if built.last() + 1 == built.count {
            (bottom - frame.end).max(0.0)
        } else {
            f32::INFINITY
        };
        let delta = delta.clamp(low, high);
        if delta.abs() < 1e-3 {
            return Scrolled::Blocked;
        }
        if let Some(offset) = self.far(delta) {
            self.ask(Request::Offset(offset));
            return Scrolled::Gap;
        }
        self.shift += delta;
        self.request = None;
        self.reanchor();
        self.settle();
        let built = &self.built;
        let gap_above = built.first > 0 && built.tops[0] - self.shift > 0.0;
        let gap_below = built.last() + 1 < built.count && built.bottom() - self.shift < frame.view;
        if gap_above || gap_below {
            Scrolled::Gap
        } else {
            Scrolled::Moved
        }
    }

    /// A scroll by `delta` lands more than a view and a half past the rows
    /// built: the rows between aren't walked, and the offset it goes to,
    /// found by the rows' estimates, is returned.
    pub fn far(&self, delta: f32) -> Option<f32> {
        let built = &self.built;
        if built.keys.is_empty() {
            return None;
        }
        let view = self.frame.view;
        let top = built.tops[0] - self.shift;
        let bottom = built.bottom() - self.shift;
        let reach = view * 1.5;
        (delta + view < top - reach || delta > bottom + reach)
            .then(|| (self.extent().value + delta).max(0.0))
    }

    /// How far the list scrolls and is scrolled, by the rows' estimates.
    pub fn extent(&self) -> Extent {
        let built = &self.built;
        let frame = self.frame;
        if built.keys.is_empty() {
            return Extent {
                page: frame.view,
                ..Extent::default()
            };
        }
        let step = self.heights.estimate() + frame.gap;
        let top = built.tops[0] - self.shift;
        let above = built.first as f32 * step;
        let below = (built.count - 1 - built.last()) as f32 * step;
        let value = frame.start + above - top;
        let total = frame.start
            + above
            + (built.bottom() - built.tops[0])
            + below
            + (frame.view - frame.end);
        let maximum = (total - frame.view).max(0.0);
        Extent {
            maximum,
            page: frame.view,
            value: value.clamp(0.0, maximum),
        }
    }
}
