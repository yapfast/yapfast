//! Where the rows go: from one row whose place is known (the first, the
//! last, or the anchor), the rows around it are built and measured until
//! they cover the view and a margin beyond, then the list is held inside its
//! ends. The Slint side: `update_visible_instances` and `yapfast_layout_end`
//! in `vendor/i-slint-core/model/repeater.rs`, which seek from an estimated
//! `content-y` and correct it; seeking from a row instead needs no estimate
//! to place what shows.
//!
//! Positions are from the top of the list's bounds, in logical pixels.

/// The rows, as the placement sees them.
pub trait Rows {
    fn count(&self) -> usize;
    /// Builds row `index` if it isn't yet and returns its height.
    fn height(&mut self, index: usize) -> f32;
}

/// The view the rows are placed in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The view's height.
    pub view: f32,
    /// Where the first row's top stops at the list's start: the top padding.
    pub start: f32,
    /// Where the last row's bottom stops at its end: the view less the
    /// bottom padding.
    pub end: f32,
    /// How far past the view's edges rows are built.
    pub margin: f32,
    /// The space between two rows.
    pub gap: f32,
    /// Rows shorter than the view all together sit at its end rather than
    /// its start, as a short conversation sits against the composer.
    pub end_aligned: bool,
}

/// The row placement starts from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seed {
    /// The first row, at the start.
    Start,
    /// The last row, at the end.
    End,
    /// A row with its top here.
    Top { index: usize, top: f32 },
    /// A row with its bottom here.
    Bottom { index: usize, bottom: f32 },
}

/// The rows built, in order from `first`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placed {
    pub first: usize,
    pub tops: Vec<f32>,
    pub heights: Vec<f32>,
}

impl Placed {
    /// The index of the last row built.
    pub fn last(&self) -> usize {
        self.first + self.tops.len().saturating_sub(1)
    }

    pub fn top(&self) -> f32 {
        self.tops.first().copied().unwrap_or_default()
    }

    pub fn bottom(&self) -> f32 {
        match (self.tops.last(), self.heights.last()) {
            (Some(top), Some(height)) => top + height,
            _ => 0.0,
        }
    }

    /// Row `index`'s top and height, if it was built.
    pub fn row(&self, index: usize) -> Option<(f32, f32)> {
        let at = index.checked_sub(self.first)?;
        Some((*self.tops.get(at)?, self.heights[at]))
    }

    fn shift(&mut self, by: f32) {
        for top in &mut self.tops {
            *top += by;
        }
    }
}

/// Builds and places the rows around `seed`, kept inside the list's ends.
pub fn place(rows: &mut impl Rows, frame: &Frame, seed: Seed) -> Placed {
    let count = rows.count();
    if count == 0 {
        return Placed::default();
    }
    let index = match seed {
        Seed::Start => 0,
        Seed::End => count - 1,
        Seed::Top { index, .. } | Seed::Bottom { index, .. } => index.min(count - 1),
    };
    let height = rows.height(index);
    let top = match seed {
        Seed::Start => frame.start,
        Seed::End => frame.end - height,
        Seed::Top { top, .. } => top,
        Seed::Bottom { bottom, .. } => bottom - height,
    };
    let mut placed = Placed {
        first: index,
        tops: vec![top],
        heights: vec![height],
    };
    fill(rows, &mut placed, frame, count);
    // Held inside the ends: a gap at either end is closed, which can bring
    // more rows into view at the other.
    for _ in 0..3 {
        let (top, bottom) = (placed.top(), placed.bottom());
        let whole = placed.first == 0 && placed.last() == count - 1;
        if whole && bottom - top <= frame.end - frame.start + 0.5 {
            let by = if frame.end_aligned {
                frame.end - bottom
            } else {
                frame.start - top
            };
            placed.shift(by);
            break;
        }
        if placed.first == 0 && top > frame.start {
            placed.shift(frame.start - top);
        } else if placed.last() == count - 1 && bottom < frame.end {
            placed.shift(frame.end - bottom);
        } else {
            break;
        }
        fill(rows, &mut placed, frame, count);
    }
    trim(&mut placed, frame);
    placed
}

/// Adds rows below and above until they reach the margin past the view or
/// the list's ends.
fn fill(rows: &mut impl Rows, placed: &mut Placed, frame: &Frame, count: usize) {
    while placed.bottom() < frame.view + frame.margin && placed.last() + 1 < count {
        let top = placed.bottom() + frame.gap;
        let height = rows.height(placed.last() + 1);
        placed.tops.push(top);
        placed.heights.push(height);
    }
    // Rows go in at the front in one splice, not one at a time.
    let mut above = Vec::new();
    let mut top = placed.top();
    let mut index = placed.first;
    while top > -frame.margin && index > 0 {
        index -= 1;
        let height = rows.height(index);
        top -= frame.gap + height;
        above.push((top, height));
    }
    if !above.is_empty() {
        placed.first = index;
        above.reverse();
        placed.tops.splice(0..0, above.iter().map(|(top, _)| *top));
        placed
            .heights
            .splice(0..0, above.iter().map(|(_, height)| *height));
    }
}

/// Drops the rows wholly past the margin, keeping at least one.
fn trim(placed: &mut Placed, frame: &Frame) {
    let past =
        |top: f32, height: f32| top + height < -frame.margin || top > frame.view + frame.margin;
    let keep_from = (0..placed.tops.len())
        .find(|at| !past(placed.tops[*at], placed.heights[*at]))
        .unwrap_or(placed.tops.len().saturating_sub(1));
    let keep_to = (keep_from..placed.tops.len())
        .rev()
        .find(|at| !past(placed.tops[*at], placed.heights[*at]))
        .unwrap_or(keep_from);
    placed.tops.truncate(keep_to + 1);
    placed.heights.truncate(keep_to + 1);
    placed.tops.drain(..keep_from);
    placed.heights.drain(..keep_from);
    placed.first += keep_from;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows of known heights that count how many were measured.
    struct Fixed {
        heights: Vec<f32>,
        measured: usize,
    }

    impl Rows for Fixed {
        fn count(&self) -> usize {
            self.heights.len()
        }
        fn height(&mut self, index: usize) -> f32 {
            self.measured += 1;
            self.heights[index]
        }
    }

    fn rows(heights: Vec<f32>) -> Fixed {
        Fixed {
            heights,
            measured: 0,
        }
    }

    fn frame(end_aligned: bool) -> Frame {
        Frame {
            view: 100.0,
            start: 0.0,
            end: 100.0,
            margin: 20.0,
            gap: 0.0,
            end_aligned,
        }
    }

    #[test]
    fn builds_only_what_covers_the_view_and_its_margin() {
        let mut rows = rows(vec![10.0; 5000]);
        let placed = place(&mut rows, &frame(false), Seed::Start);
        assert_eq!(placed.first, 0);
        assert_eq!(placed.tops.len(), 12, "100 px of view and 20 of margin");
        assert!(rows.measured <= 13);
    }

    #[test]
    fn lays_out_from_the_end() {
        let mut rows = rows(vec![30.0; 50]);
        let placed = place(&mut rows, &frame(true), Seed::End);
        assert_eq!(placed.last(), 49);
        assert_eq!(placed.bottom(), 100.0);
        assert!(placed.top() <= -20.0 + 30.0);
    }

    #[test]
    fn a_short_list_sits_at_its_start_or_its_end() {
        let mut short = rows(vec![20.0, 30.0]);
        let placed = place(&mut short, &frame(false), Seed::End);
        assert_eq!((placed.top(), placed.bottom()), (0.0, 50.0));
        let placed = place(&mut short, &frame(true), Seed::Start);
        assert_eq!((placed.top(), placed.bottom()), (50.0, 100.0));
    }

    #[test]
    fn holds_a_row_where_it_is_and_closes_gaps_at_the_ends() {
        let mut rows = rows(vec![25.0; 40]);
        let placed = place(
            &mut rows,
            &frame(false),
            Seed::Top {
                index: 20,
                top: 33.0,
            },
        );
        assert_eq!(placed.row(20), Some((33.0, 25.0)));
        // A row near the start can't come down past it.
        let placed = place(
            &mut rows,
            &frame(false),
            Seed::Top {
                index: 1,
                top: 60.0,
            },
        );
        assert_eq!(placed.row(0), Some((0.0, 25.0)));
        // Nor one near the end up past that.
        let placed = place(
            &mut rows,
            &frame(false),
            Seed::Bottom {
                index: 39,
                bottom: 50.0,
            },
        );
        assert_eq!(placed.bottom(), 100.0);
    }

    #[test]
    fn a_row_taller_than_the_view_scrolls_within_itself() {
        let mut tall = rows(vec![1000.0]);
        let placed = place(
            &mut tall,
            &frame(false),
            Seed::Top {
                index: 0,
                top: -400.0,
            },
        );
        assert_eq!(placed.top(), -400.0);
        let placed = place(
            &mut tall,
            &frame(false),
            Seed::Top {
                index: 0,
                top: -950.0,
            },
        );
        assert_eq!(placed.bottom(), 100.0, "held at its end");
    }

    #[test]
    fn spaces_rows_by_the_gap_and_keeps_the_padding() {
        let mut rows = rows(vec![10.0; 3]);
        let padded = Frame {
            start: 8.0,
            end: 92.0,
            gap: 2.0,
            ..frame(false)
        };
        let placed = place(&mut rows, &padded, Seed::Start);
        assert_eq!(placed.tops, vec![8.0, 20.0, 32.0]);
        let placed = place(
            &mut rows,
            &Frame {
                end_aligned: true,
                ..padded
            },
            Seed::Start,
        );
        assert_eq!(placed.bottom(), 92.0);
    }
}
