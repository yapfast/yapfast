//! What a selection is: where the press began and where the pointer took
//! it, each a place in a row kept by the row's key, so rows scrolled away
//! and back, or rows going in above, leave it where it was; and what's
//! copied of the rows at either end, as they were shown (`Position`,
//! `Selection` and `Drag` in `yapfast_selection.rs`).

use std::cmp::Ordering;
use std::sync::atomic::{AtomicU64, Ordering as Atomic};
use std::time::Instant;

use iced::Point;
use iced::advanced::mouse;
use iced::advanced::text::Target;
use iced::advanced::widget::Id;

use super::acts::Span;
use super::pass::Scroller;

/// The last version given out: each is new to every selection, so no row
/// shows one it didn't take from its own.
static VERSIONS: AtomicU64 = AtomicU64::new(0);

/// Where in a part a place is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum At {
    /// Before its first character.
    Start,
    /// A point in the part's own coordinates.
    Point(Point),
    /// After its last character.
    End,
}

/// A place in the rows: a row, a part of it and where in that part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    /// The row's index in the list, as last found, and its key, which finds
    /// it again.
    pub row: usize,
    pub key: u64,
    /// The part, in the row's reading order.
    pub part: usize,
    pub at: At,
}

impl Position {
    /// Its order among places: by row, then by part.
    pub fn order(&self) -> (usize, usize) {
        (self.row, self.part)
    }
}

/// The selection, from where the press began to where the pointer took it.
#[derive(Clone, Debug)]
pub struct Held {
    pub anchor: Position,
    pub focus: Position,
    /// What a press selects by: characters, or the words or lines of a
    /// double or triple press.
    pub unit: Target,
    /// What's copied of the rows at either end: the first from its start
    /// in the selection to its end, the last from its start to its end in
    /// the selection. When both are one row, `first` is all of it.
    pub first: String,
    pub last: String,
    /// Whether it selects anything.
    pub selected: bool,
}

impl Held {
    /// A selection beginning at `anchor`, selecting nothing yet.
    pub fn at(anchor: Position, unit: Target) -> Self {
        Self {
            anchor,
            focus: anchor,
            unit,
            first: String::new(),
            last: String::new(),
            selected: false,
        }
    }

    /// Its ends in reading order.
    pub fn ends(&self) -> (Position, Position) {
        if self.focus.order() < self.anchor.order() {
            (self.focus, self.anchor)
        } else {
            (self.anchor, self.focus)
        }
    }

    /// It as copied: the rows at either end as shown, and each row between
    /// whole from `rows`.
    pub fn copied(&self, rows: Option<&Rows<'_>>) -> Option<String> {
        if !self.selected {
            return None;
        }
        let (first, last) = self.ends();
        if first.row == last.row {
            return (!self.first.is_empty()).then(|| self.first.clone());
        }
        let mut text = String::new();
        let mut add = |piece: &str| {
            if !piece.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(piece);
            }
        };
        add(&self.first);
        if let Some(rows) = rows {
            for index in first.row + 1..last.row.min(rows.count) {
                if let Some(row) = (rows.text)(index) {
                    add(&row);
                }
            }
        }
        add(&self.last);
        (!text.is_empty()).then_some(text)
    }
}

/// A press selecting, while the button is down.
#[derive(Clone, Debug)]
pub struct Drag {
    /// The pointer, in the window's coordinates.
    pub pointer: Point,
    /// The press's row as copied before the press and after it, for when it
    /// scrolls away.
    pub before: String,
    pub after: String,
    /// What scrolls the press's row, which a drag at its edge scrolls.
    pub scroller: Option<Scroller>,
    /// The last frame that scrolled it at an edge.
    pub frame: Option<Instant>,
    /// Where the pointer and the rows were when it last selected: a frame
    /// finding both where they were selects nothing anew.
    pub last: Option<(Point, Span)>,
    /// Which side of the press's row the pointer was on then, whose copy of
    /// that row `held` holds.
    pub side: Ordering,
}

/// The selection's state in the widget tree.
#[derive(Debug, Default)]
pub struct State {
    /// The selection's id: another starts afresh, as for another room.
    pub id: Option<Id>,
    pub held: Option<Held>,
    pub drag: Option<Drag>,
    /// The last left press, and how many came in a row before it.
    pub click: Option<mouse::Click>,
    pub clicks: u8,
    /// [`super::clear`] asked for it to go.
    pub clear: bool,
    /// Which `held` this is, new each time it changes: a row keeps the one
    /// it shows, so a layout shows it again only in rows that don't.
    pub version: u64,
    /// The version content without rows shows.
    pub shown: u64,
    /// The rows the last layout showed the selection in again.
    #[cfg(test)]
    pub reshown: usize,
}

/// A version no selection had yet.
pub fn next_version() -> u64 {
    VERSIONS.fetch_add(1, Atomic::Relaxed) + 1
}

impl State {
    /// `held` changed, or is about to: its new version.
    pub fn changed(&mut self) -> u64 {
        self.version = next_version();
        self.version
    }

    /// The selection goes, and with it any drag.
    pub fn drop_held(&mut self) {
        self.held = None;
        self.drag = None;
        self.changed();
    }

    /// What the press in a row of presses selects by: characters, words,
    /// lines, then characters again (`click_count % 3` in `press`).
    pub fn unit(&mut self, at: Point) -> Target {
        let click = mouse::Click::new(at, mouse::Button::Left, self.click);
        self.clicks = match click.kind() {
            mouse::click::Kind::Single => 0,
            _ => self.clicks.wrapping_add(1),
        };
        self.click = Some(click);
        match self.clicks % 3 {
            0 => Target::Character,
            1 => Target::Word,
            _ => Target::Line,
        }
    }
}

/// The rows as the list holds them; see [`super::Selection::rows`].
pub struct Rows<'a> {
    pub count: usize,
    pub key: Box<dyn Fn(usize) -> u64 + 'a>,
    pub text: Box<dyn Fn(usize) -> Option<String> + 'a>,
}

impl Rows<'_> {
    /// Where the row keyed `key` is now, looked for outward from `hint`,
    /// where it was: rows that went in above or below move it.
    pub fn index_of(&self, key: u64, hint: usize) -> Option<usize> {
        let count = self.count;
        let hint = hint.min(count.checked_sub(1)?);
        (0..count).find_map(|step| {
            let below = hint + step;
            if below < count && (self.key)(below) == key {
                return Some(below);
            }
            let above = hint.checked_sub(step)?;
            ((self.key)(above) == key).then_some(above)
        })
    }

    /// `position` with its row's index as it is now.
    pub fn found(&self, mut position: Position) -> Position {
        if let Some(index) = self.index_of(position.key, position.row) {
            position.row = index;
        }
        position
    }

    /// Finds `held`'s ends again where rows went in or out. Returns whether
    /// either is still among the rows: when neither is, these are other
    /// rows (another room's), and the selection is gone.
    pub fn find(&self, held: &mut Held) -> bool {
        let anchor = self.index_of(held.anchor.key, held.anchor.row);
        let focus = self.index_of(held.focus.key, held.focus.row);
        if let Some(index) = anchor {
            held.anchor.row = index;
        }
        if let Some(index) = focus {
            held.focus.row = index;
        }
        anchor.is_some() || focus.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(keys: &[u64]) -> Rows<'_> {
        Rows {
            count: keys.len(),
            key: Box::new(|at| keys[at]),
            text: Box::new(|at| Some(format!("row {at}"))),
        }
    }

    fn place(row: usize, key: u64) -> Position {
        Position {
            row,
            key,
            part: 0,
            at: At::Start,
        }
    }

    #[test]
    fn a_row_is_found_by_its_key_wherever_it_went() {
        let keys = [10, 11, 12, 13, 14];
        let rows = rows(&keys);
        assert_eq!(rows.index_of(13, 1), Some(3), "found below the hint");
        assert_eq!(rows.index_of(10, 4), Some(0), "found above it");
        assert_eq!(rows.index_of(99, 2), None);
        assert_eq!(rows.found(place(0, 12)).row, 2);
        assert_eq!(rows.found(place(3, 99)).row, 3, "a row gone stays put");
    }

    #[test]
    fn rows_between_the_ends_copy_whole() {
        let keys = [0, 1, 2, 3];
        let rows = rows(&keys);
        let held = Held {
            anchor: place(3, 3),
            focus: place(0, 0),
            unit: Target::Character,
            first: "first".into(),
            last: "last".into(),
            selected: true,
        };
        assert_eq!(
            held.copied(Some(&rows)).as_deref(),
            Some("first\nrow 1\nrow 2\nlast")
        );
        let nothing = Held {
            selected: false,
            ..held
        };
        assert_eq!(nothing.copied(Some(&rows)), None);
    }

    #[test]
    fn presses_in_a_row_cycle_through_their_units() {
        let mut state = State::default();
        let at = Point::new(10.0, 10.0);
        let units: Vec<Target> = (0..4).map(|_| state.unit(at)).collect();
        assert_eq!(
            units,
            [
                Target::Character,
                Target::Word,
                Target::Line,
                Target::Character
            ]
        );
        assert_eq!(state.unit(Point::new(200.0, 10.0)), Target::Character);
    }
}
