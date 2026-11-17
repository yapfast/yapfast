//! What a part does for the selection, and what a row keeps of it: a
//! fingerprint of the acts its parts were last given. Each text an apply
//! acts on takes its paragraph into a new allocation (`Arc::make_mut`),
//! which the renderer then counts as changed and draws anew; so an apply,
//! and a clear, act only on the rows whose fingerprint changes, and on the
//! rows they copy.

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::advanced::text::Target;
use iced::advanced::widget::operation::Text;
use iced::{Point, Rectangle, Vector};

use super::model::At;

/// What a part does for the selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    Keep,
    Clear,
    All,
    /// From one place to another, the first where a press would be.
    Between(At, At),
    /// A mark covered, or not.
    Cover(bool),
}

/// The fingerprint of a row that shows nothing of a selection, as one built
/// anew does.
pub const CLEAR: u64 = 1;
/// The fingerprint of a row whose texts may show anything: another row's
/// state, taken over.
pub const UNKNOWN: u64 = 0;

#[cfg(test)]
thread_local! {
    /// How many rows the applies and clears acted on, for tests.
    pub static ACTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What an apply does to a row it meets, by the order the walk met them.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    /// Its parts' acts' fingerprint; `None` when they keep what it shows.
    pub print: Option<u64>,
    /// Its parts are copied: they're acted on even when it shows them.
    pub copy: bool,
}

impl Plan {
    /// A clear's, for every row.
    pub const CLEAR: Self = Self {
        print: Some(CLEAR),
        copy: false,
    };

    /// Whether it acts on a row whose fingerprint is `shown`, which it then
    /// leaves at its own.
    pub fn take(&self, shown: &mut u64) -> bool {
        let changes = self.print.is_some_and(|print| print != *shown);
        if !(changes || self.copy) {
            return false;
        }
        if let Some(print) = self.print {
            *shown = print;
        }
        acted();
        true
    }
}

/// Counts a row acted on, for tests.
pub fn acted() {
    #[cfg(test)]
    ACTED.set(ACTED.get() + 1);
}

/// A row's plan, from its parts' acts in the order the walk found them.
#[derive(Clone)]
pub struct Tally {
    hasher: DefaultHasher,
    clear: bool,
    keep: bool,
    copy: bool,
}

impl Tally {
    /// A row's, its texts selecting by `unit`.
    pub fn new(unit: Target) -> Self {
        let mut hasher = DefaultHasher::new();
        let unit: u8 = match unit {
            Target::Character => 0,
            Target::Word => 1,
            Target::Line => 2,
        };
        unit.hash(&mut hasher);
        Self {
            hasher,
            clear: true,
            keep: true,
            copy: false,
        }
    }

    /// A part doing `act`, copied or not.
    pub fn add(&mut self, act: Act, copy: bool) {
        self.keep &= act == Act::Keep;
        self.clear &= matches!(act, Act::Clear | Act::Cover(false));
        self.copy |= copy;
        let at = |at: At, hasher: &mut DefaultHasher| match at {
            At::Start => 0u8.hash(hasher),
            At::End => 1u8.hash(hasher),
            At::Point(point) => {
                2u8.hash(hasher);
                point.x.to_bits().hash(hasher);
                point.y.to_bits().hash(hasher);
            }
        };
        let hasher = &mut self.hasher;
        match act {
            Act::Keep => 0u8.hash(hasher),
            Act::Clear => 1u8.hash(hasher),
            Act::All => 2u8.hash(hasher),
            Act::Between(from, to) => {
                3u8.hash(hasher);
                at(from, hasher);
                at(to, hasher);
            }
            Act::Cover(on) => {
                4u8.hash(hasher);
                on.hash(hasher);
            }
        }
    }

    pub fn plan(&self) -> Plan {
        let print = if self.clear {
            Some(CLEAR)
        } else if self.keep {
            None
        } else {
            // Never one of the fixed fingerprints.
            Some(self.hasher.finish().max(CLEAR + 1))
        };
        Plan {
            print,
            copy: self.copy,
        }
    }
}

/// How far past a part's edges [`At::Start`] and [`At::End`] point: the
/// text under them lays out to its first and last character.
const FAR: f32 = 1.0e6;

/// `at` in the coordinates of the part at `bounds`.
pub fn point(at: At, bounds: Rectangle) -> Point {
    let local = match at {
        At::Start => Vector::new(-FAR, -FAR),
        At::End => Vector::new(FAR, FAR),
        At::Point(point) => Vector::new(point.x, point.y),
    };
    bounds.position() + local
}

/// Sets a text's selection as `act` says, the text laid out at `bounds`.
pub fn act(state: &mut dyn Text, act: Option<Act>, bounds: Rectangle, unit: Target) {
    match act {
        Some(Act::Clear) => state.deselect(),
        Some(Act::All) => state.select_all(),
        Some(Act::Between(from, to)) => {
            let (from, to) = (point(from, bounds), point(to, bounds));
            // A text keeps where its last selection began and selects from
            // there: begin it here first.
            state.select(from, from, unit);
            if from != to {
                state.select(from, to, unit);
            }
        }
        _ => {}
    }
}
