//! From a walk's parts to what each does: the rows they make up, each read
//! top to bottom and then left to right; the place under the pointer; what
//! each part selects between two places; and a row as copied from what its
//! parts selected (`parts`, `row_text`, `covered_range` and `drag_to` in
//! `yapfast_selection.rs`).

use std::cmp::Ordering;

use iced::advanced::text::Target;
use iced::{Point, Rectangle};

use super::act::{Act, Tally};
use super::marks::Kind;
use super::model::{At, Position};
use super::pass::{Met, Orders, Part, Scroller};

/// How far the pointer moves on a whole text before a drag on it alone
/// covers it, so a click on a name selects nothing.
const MOVED: f32 = 2.0;

/// A row's parts in reading order, by their index in the walk.
#[derive(Clone, Debug)]
pub struct Seen {
    pub index: usize,
    pub key: u64,
    pub bounds: Rectangle,
    pub order: Vec<usize>,
}

/// Where the rows are, by the first and the last one's index and their
/// first part (content without rows by its first part): what moves as the
/// list scrolls or a row grows.
pub type Span = [Option<(usize, Rectangle)>; 2];

/// The parts a walk found, the rows they make up, in the list's order, and
/// what scrolls them.
pub struct Walked {
    pub parts: Vec<Part>,
    pub rows: Vec<Seen>,
    pub scrollers: Vec<Scroller>,
    /// The content has no rows: it's one.
    pub rowless: bool,
    /// How many row marks were met, a row met twice counted twice.
    pub met: usize,
}

impl Walked {
    /// Groups `parts` into the rows `marks` met; content without rows is one
    /// row, and parts outside every row of content with rows are no part.
    pub fn new(
        parts: Vec<Part>,
        marks: &[Met],
        scrollers: Vec<Scroller>,
        whole: Rectangle,
    ) -> Self {
        let reading = |a: &usize, b: &usize| {
            let (a, b) = (parts[*a].bounds, parts[*b].bounds);
            (a.y, a.x)
                .partial_cmp(&(b.y, b.x))
                .unwrap_or(Ordering::Equal)
        };
        let mut rows: Vec<Seen> = if marks.is_empty() {
            let mut order: Vec<usize> = (0..parts.len()).collect();
            order.sort_by(reading);
            vec![Seen {
                index: 0,
                key: 0,
                bounds: whole,
                order,
            }]
        } else {
            marks
                .iter()
                .enumerate()
                .map(|(slot, met)| {
                    let mut order: Vec<usize> = (0..parts.len())
                        .filter(|at| parts[*at].mark == Some(slot))
                        .collect();
                    order.sort_by(reading);
                    Seen {
                        index: met.index,
                        key: met.key,
                        bounds: met.bounds,
                        order,
                    }
                })
                .collect()
        };
        rows.sort_by_key(|row| row.index);
        // A row met twice (a dragged copy of it) counts once.
        rows.dedup_by_key(|row| row.index);
        Self {
            parts,
            rows,
            scrollers,
            rowless: marks.is_empty(),
            met: marks.len(),
        }
    }

    /// Where the rows are now.
    pub fn span(&self) -> Span {
        let at = |row: &Seen| {
            let first = row.order.first().map(|at| self.parts[*at].bounds);
            (row.index, first.unwrap_or(row.bounds))
        };
        [self.rows.first().map(at), self.rows.last().map(at)]
    }

    /// Every part read in order, with its place.
    fn places(&self) -> impl Iterator<Item = (Position, &Part)> + '_ {
        self.rows.iter().flat_map(move |row| {
            row.order.iter().enumerate().map(move |(part, at)| {
                let place = Position {
                    row: row.index,
                    key: row.key,
                    part,
                    at: At::Start,
                };
                (place, &self.parts[*at])
            })
        })
    }

    /// Where a press at `point` starts a selection: in a text that selects
    /// itself, or a whole text.
    pub fn pressed(&self, point: Point) -> Option<Position> {
        self.places()
            .filter(|(_, part)| part.pressable && shows(part, point))
            .last()
            .map(|(place, part)| Position {
                at: At::Point(local(part, point)),
                ..place
            })
    }

    /// Where the pointer at `point` takes a selection: the part under it, or
    /// the end of the last part before it in reading order, above it or
    /// level with it and to its left, or the start of the first part shown.
    pub fn focus(&self, point: Point) -> Option<Position> {
        let mut under = None;
        let mut before = None;
        let mut first = None;
        for (place, part) in self.places().filter(|(_, part)| part.shown.is_some()) {
            first.get_or_insert(place);
            if shows(part, point) {
                under = Some(Position {
                    at: At::Point(local(part, point)),
                    ..place
                });
            }
            let bounds = part.bounds;
            let above = bounds.y + bounds.height <= point.y;
            let left = bounds.y <= point.y && bounds.x + bounds.width <= point.x;
            if above || left {
                before = Some(Position {
                    at: At::End,
                    ..place
                });
            }
        }
        under.or(before).or(first)
    }

    /// What scrolls the part at `place`.
    pub fn scroller(&self, place: Position) -> Option<&Scroller> {
        let at = self.row(place.row)?.order.get(place.part)?;
        self.scrollers.get(self.parts[*at].scroller?)
    }

    /// The row with list index `index`, if it was walked.
    pub fn row(&self, index: usize) -> Option<&Seen> {
        self.rows.iter().find(|row| row.index == index)
    }

    /// What each part does for a selection begun at `from` and taken to
    /// `to`: the parts between select all, those at either end from or to
    /// their place, the rest nothing. Only row `only`'s parts act when it's
    /// given; the others keep what they show.
    pub fn acts(
        &self,
        from: Position,
        to: Position,
        unit: Target,
        only: Option<usize>,
    ) -> Vec<Act> {
        let (first, last) = if to.order() < from.order() {
            (to, from)
        } else {
            (from, to)
        };
        let same = first.order() == last.order();
        let mut acts = vec![Act::Keep; self.parts.len()];
        for row in &self.rows {
            if only.is_some_and(|only| only != row.index) {
                continue;
            }
            for (part, at) in row.order.iter().enumerate() {
                let place = (row.index, part);
                let atom = self.parts[*at].kind != Kind::Text;
                acts[*at] = if place < first.order() || place > last.order() {
                    if atom { Act::Cover(false) } else { Act::Clear }
                } else if same {
                    if atom {
                        Act::Cover(covers(from.at, to.at, unit))
                    } else {
                        Act::Between(from.at, to.at)
                    }
                } else if place == first.order() {
                    if atom {
                        Act::Cover(first.at != At::End)
                    } else {
                        Act::Between(first.at, At::End)
                    }
                } else if place == last.order() {
                    if atom {
                        Act::Cover(last.at != At::Start)
                    } else {
                        Act::Between(At::Start, last.at)
                    }
                } else if atom {
                    Act::Cover(true)
                } else {
                    Act::All
                };
            }
        }
        acts
    }

    /// What an apply does with `acts`, copying the parts `copy` marks, by
    /// `unit`: with each row's plan, by the order the marks were met.
    pub fn orders<'a>(&self, acts: &'a [Act], copy: &'a [bool], unit: Target) -> Orders<'a> {
        let mut tallies = vec![Tally::new(unit); self.met];
        for (at, part) in self.parts.iter().enumerate() {
            if let Some(tally) = part.mark.and_then(|slot| tallies.get_mut(slot)) {
                let act = acts.get(at).copied().unwrap_or(Act::Keep);
                tally.add(act, copy.get(at).copied().unwrap_or(false));
            }
        }
        Orders {
            acts,
            copy,
            unit,
            plans: tallies.iter().map(Tally::plan).collect(),
        }
    }

    /// Which parts are in the rows `rows`, for copying what they select.
    pub fn copying(&self, rows: &[usize]) -> Vec<bool> {
        let mut copy = vec![false; self.parts.len()];
        for row in self.rows.iter().filter(|row| rows.contains(&row.index)) {
            for at in &row.order {
                copy[*at] = true;
            }
        }
        copy
    }

    /// Row `index` as copied: what each of its parts selected, texts level
    /// with each other joined by a space, the rest, and pictures, by line
    /// breaks. `pieces` are what the texts copied; `acts` say which whole
    /// texts and pictures are covered.
    pub fn copied(&self, index: usize, acts: &[Act], pieces: &[Option<String>]) -> String {
        let Some(row) = self.row(index) else {
            return String::new();
        };
        let picture = yapfast_lang::tr!("[Picture]");
        let mut text = String::new();
        let mut last: Option<&Part> = None;
        for at in &row.order {
            let part = &self.parts[*at];
            let piece = match part.kind {
                Kind::Text => pieces.get(*at).and_then(Option::as_deref),
                Kind::Whole => covered(acts, *at).then_some(part.words.as_str()),
                Kind::Picture => covered(acts, *at).then_some(picture),
            };
            let Some(piece) = piece.filter(|piece| !piece.is_empty()) else {
                continue;
            };
            if let Some(last) = last {
                let level = part.kind != Kind::Picture
                    && last.kind != Kind::Picture
                    && part.bounds.y < last.bounds.y + last.bounds.height
                    && last.bounds.y < part.bounds.y + part.bounds.height;
                text.push(if level { ' ' } else { '\n' });
            }
            text.push_str(piece);
            last = Some(part);
        }
        text
    }
}

/// Whether a whole text or a picture is covered by its act.
fn covered(acts: &[Act], at: usize) -> bool {
    matches!(acts.get(at), Some(Act::Cover(true)))
}

/// Whether a drag from `from` to `to` on one whole text covers it: unless
/// both are at one edge, or a click by characters barely moved.
fn covers(from: At, to: At, unit: Target) -> bool {
    match (from, to) {
        (At::Start, At::Start) | (At::End, At::End) => false,
        (At::Point(from), At::Point(to)) => unit != Target::Character || from.distance(to) > MOVED,
        _ => true,
    }
}

/// Whether `point` is on what shows of `part`.
fn shows(part: &Part, point: Point) -> bool {
    part.shown.is_some_and(|shown| shown.contains(point))
}

/// `point` in `part`'s own coordinates.
fn local(part: &Part, point: Point) -> Point {
    Point::new(point.x - part.bounds.x, point.y - part.bounds.y)
}
