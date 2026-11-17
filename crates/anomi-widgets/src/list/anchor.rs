//! Where the next layout starts: what was asked of the list, then the row
//! that holds it, found again by its key wherever rows went in or out; a
//! row gone hands its place to the nearest built row still there. The
//! Slint side: `YapfastHold` and the jump's `estimate-jump` and
//! `settle-jump` (`vendor/i-slint-core/model/repeater.rs`,
//! `ui/yapfast.slint`), which this needs no estimate for: the row is placed
//! where it's asked to be, and the rows around it measured from there.

use super::heights::Heights;
use super::place::{Placed, Seed};
use super::state::{Anchor, Position, Request, State};

impl State {
    /// The row the next layout starts from, after any request.
    pub fn seed(&mut self, count: usize, key: &dyn Fn(usize) -> u64) -> Seed {
        self.asked = false;
        if count == 0 {
            return Seed::Start;
        }
        if let Some(request) = self.request.take() {
            self.apply(request, count, key);
        }
        let Position::Row(anchor) = &mut self.position else {
            return match self.position {
                Position::End => Seed::End,
                _ => Seed::Start,
            };
        };
        let Some(index) = find(key, count, anchor.key, anchor.index) else {
            return self.fallback(count, key);
        };
        let before = Some(index.checked_sub(1).map(key));
        if anchor.before.is_some() && before != anchor.before {
            anchor.keep_bottom = true;
        }
        anchor.index = index;
        anchor.before = before;
        if anchor.keep_bottom {
            Seed::Bottom {
                index,
                bottom: anchor.top + anchor.height,
            }
        } else {
            Seed::Top {
                index,
                top: anchor.top,
            }
        }
    }

    fn apply(&mut self, request: Request, count: usize, key: &dyn Fn(usize) -> u64) {
        let frame = self.frame;
        match request {
            Request::Start => self.position = Position::Start,
            Request::End => self.position = Position::End,
            Request::By(delta) => {
                // Far past the rows built, as the person's scroll does.
                if let Some(offset) = self.far(delta) {
                    return self.apply(Request::Offset(offset), count, key);
                }
                self.reanchor();
                if let Position::Row(anchor) = &mut self.position {
                    anchor.top -= delta;
                }
            }
            Request::Offset(offset) => {
                let step = self.heights.estimate() + frame.gap;
                let index = (((offset - frame.start) / step.max(1.0)).floor().max(0.0) as usize)
                    .min(count - 1);
                let top = frame.start + index as f32 * step - offset;
                self.position = Position::Row(anchor_at(index, top, &self.heights, key));
            }
            Request::Jump { key: wanted, place } => {
                let hint = self.built.first;
                let Some(index) = find(key, count, wanted, hint) else {
                    // Not in the list yet: it waits for the rows to come.
                    self.request = Some(request);
                    return;
                };
                let height = self.heights.height(wanted);
                let shown = self
                    .built
                    .keys
                    .iter()
                    .position(|built| *built == wanted)
                    .map(|at| self.row(at));
                // A row below the view that isn't built is placed by its
                // bottom: its height may be only the estimate until it is.
                let (top, keep_bottom) = match place {
                    super::Place::Top => (frame.start, false),
                    super::Place::Third => (frame.view / 3.0, false),
                    super::Place::Reveal(margin) => match shown {
                        Some((top, height)) => (reveal(top, height, margin, frame.view), false),
                        None if index < self.built.first => (margin, false),
                        None => (frame.view - margin - height, true),
                    },
                };
                self.position = Position::Row(Anchor {
                    keep_bottom,
                    ..anchor_at(index, top, &self.heights, key)
                });
            }
        }
    }

    /// The anchor's row is gone: the nearest row built that's still there
    /// keeps its place, or the list starts afresh.
    fn fallback(&mut self, count: usize, key: &dyn Fn(usize) -> u64) -> Seed {
        let from = match self.position {
            Position::Row(anchor) => anchor.index.saturating_sub(self.built.first),
            _ => 0,
        };
        let order = (from..self.built.keys.len()).chain((0..from.min(self.built.keys.len())).rev());
        for at in order {
            let hint = self.built.first + at;
            if let Some(index) = find(key, count, self.built.keys[at], hint) {
                let (top, _) = self.row(at);
                self.position = Position::Row(anchor_at(index, top, &self.heights, key));
                return Seed::Top { index, top };
            }
        }
        self.position = if self.from_end && self.follow {
            Position::End
        } else {
            Position::Start
        };
        match self.position {
            Position::End => Seed::End,
            _ => Seed::Start,
        }
    }
}

/// A row placed with its top at `top`.
fn anchor_at(index: usize, top: f32, heights: &Heights, key: &dyn Fn(usize) -> u64) -> Anchor {
    let row = key(index);
    Anchor {
        key: row,
        index,
        top,
        height: heights.height(row),
        before: Some(index.checked_sub(1).map(key)),
        keep_bottom: false,
    }
}

/// Where a row `height` tall at `top` goes to show `margin` from the view's
/// edges: its bottom first, as a field opening in it grows downwards.
pub fn reveal(top: f32, height: f32, margin: f32, view: f32) -> f32 {
    if top + height > view - margin {
        view - margin - height
    } else if top < margin {
        margin
    } else {
        top
    }
}

/// Where the row kept in view goes: into view when it's first kept (`known`
/// holds another key, or none) and as its height changes; nowhere new once
/// it was scrolled away, or while it shows unchanged.
pub fn reveal_seed(
    known: Option<(u64, f32)>,
    key: u64,
    index: usize,
    margin: f32,
    placed: &Placed,
    view: f32,
) -> Option<Seed> {
    let known = known.filter(|(seen, _)| *seen == key);
    match placed.row(index) {
        Some((_, height)) if known.is_some_and(|(_, seen)| seen == height) => None,
        Some((top, height)) => {
            let to = reveal(top, height, margin, view);
            ((to - top).abs() > 0.5).then_some(Seed::Top { index, top: to })
        }
        None if known.is_some() => None,
        None if index < placed.first => Some(Seed::Top { index, top: margin }),
        None => Some(Seed::Bottom {
            index,
            bottom: view - margin,
        }),
    }
}

/// The index of the row keyed `wanted`, looked for around `hint` first.
pub fn find(key: &dyn Fn(usize) -> u64, count: usize, wanted: u64, hint: usize) -> Option<usize> {
    const NEAR: usize = 64;
    let hint = hint.min(count.saturating_sub(1));
    for distance in 0..=NEAR {
        if let Some(index) = hint.checked_add(distance).filter(|index| *index < count)
            && key(index) == wanted
        {
            return Some(index);
        }
        if let Some(index) = hint.checked_sub(distance).filter(|_| distance > 0)
            && key(index) == wanted
        {
            return Some(index);
        }
    }
    (0..count).find(|index| key(*index) == wanted)
}
