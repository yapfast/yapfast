//! One walk over the rows built, as an operation: to find the parts (texts
//! that select themselves, and the marks' whole texts and pictures), each
//! with its row, where it shows in the window, what of it shows past the
//! scrollables around it, and whether a press there may select (`place`,
//! `parts` and `rows_in` in `yapfast_selection.rs`); or to set each part's
//! selection, copying what the rows at the selection's ends show. Both
//! walks count the parts alike, so the second finds each part the first
//! numbered: the tree doesn't change between them. Each row keeps the
//! version of the selection it shows, which the apply sets; a pass may go
//! only into the rows that show another, as a layout does. Each row also
//! keeps its acts' fingerprint (`act`): an apply acts only on the rows
//! whose fingerprint it changes, or whose parts it copies.
//!
//! A scrollable gives its content unscrolled, with how far it's scrolled
//! (its `translation`), as iced's own selection reads it
//! (`operation::text::select`): the walk keeps where parts show, taking
//! the scrollables' translations off, and the apply hands each text its
//! own layout's coordinates.

use std::any::Any;

use iced::advanced::text::Target;
use iced::advanced::widget::Id;
use iced::advanced::widget::operation::{Focusable, Operation, Scrollable, Text, TextInput};
use iced::{Point, Rectangle, Size, Vector};

use super::act::{self, Act, Plan, UNKNOWN};
use super::marks::{Cover, Kind, RowMark};

/// What scrolls a part: a scrollable's id and where it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Scroller {
    pub id: Option<Id>,
    pub bounds: Rectangle,
}

/// A part of a row.
#[derive(Clone, Debug)]
pub struct Part {
    pub kind: Kind,
    /// Where it shows in the window, and what of it shows there.
    pub bounds: Rectangle,
    pub shown: Option<Rectangle>,
    /// The row mark it's in, by the order they were met.
    pub mark: Option<usize>,
    /// A press there may start a selection: a whole text, or a text found
    /// to select itself under the walk's probe.
    pub pressable: bool,
    /// A whole text's words.
    pub words: String,
    /// What scrolls it, among the walk's scrollers.
    pub scroller: Option<usize>,
}

/// A row mark met: the row's index in the list, its key, where it shows.
#[derive(Clone, Copy, Debug)]
pub struct Met {
    pub index: usize,
    pub key: u64,
    pub bounds: Rectangle,
}

/// What an apply does: each part's act, which parts it copies, by what
/// its texts select, and each row's plan, by the order the walk met them.
pub struct Orders<'a> {
    pub acts: &'a [Act],
    pub copy: &'a [bool],
    pub unit: Target,
    pub plans: Vec<Plan>,
}

pub enum Mode<'a> {
    /// Finds the parts, trying whether those under `probe` select.
    Walk {
        probe: Option<Point>,
        parts: Vec<Part>,
        marks: Vec<Met>,
        scrollers: Vec<Scroller>,
    },
    /// Does each part's act, copying what those marked `copy` select.
    Apply {
        orders: Orders<'a>,
        pieces: Vec<Option<String>>,
    },
    /// Clears every part.
    Clear,
}

/// Which rows a pass goes into, and the version of the selection the rows
/// an apply acts on then show.
#[derive(Clone, Copy, Debug)]
pub struct Showing {
    pub version: u64,
    /// Only rows that show another: a row built anew shows none.
    pub behind: bool,
}

impl Showing {
    /// Every row.
    pub fn all(version: u64) -> Self {
        Self {
            version,
            behind: false,
        }
    }

    /// The rows that don't show `version`.
    pub fn behind(version: u64) -> Self {
        Self {
            version,
            behind: true,
        }
    }
}

/// What a widget said of what's inside it, taken up when it traverses.
#[derive(Default)]
struct Pending {
    clip: Option<Rectangle>,
    translation: Vector,
    scroller: Option<usize>,
    mark: Option<usize>,
    within: Option<(usize, Kind)>,
    /// A row the pass goes through without acting on it, or does act on.
    quiet: Option<bool>,
    /// A row the pass doesn't go into.
    skip: bool,
}

/// Where the pass is: what shows, what scrolls, which row, which mark.
#[derive(Clone, Copy)]
struct Around {
    clip: Rectangle,
    /// How far the scrollables around have scrolled their content.
    translation: Vector,
    scroller: Option<usize>,
    mark: Option<usize>,
    /// The whole text or picture being walked, whose texts are its own.
    within: Option<(usize, Kind)>,
    /// In a row the pass leaves as it is.
    quiet: bool,
}

pub struct Pass<'a> {
    pub mode: Mode<'a>,
    showing: Showing,
    /// The parts counted so far, and the rows met.
    count: usize,
    met: usize,
    around: Around,
    pending: Pending,
}

impl<'a> Pass<'a> {
    /// A walk over what shows inside `clip`, into the rows `showing` says.
    pub fn new(mode: Mode<'a>, clip: Rectangle, showing: Showing) -> Self {
        Self {
            mode,
            showing,
            count: 0,
            met: 0,
            around: Around {
                clip,
                translation: Vector::ZERO,
                scroller: None,
                mark: None,
                within: None,
                quiet: false,
            },
            pending: Pending::default(),
        }
    }

    /// Where something laid out at `bounds` shows in the window.
    fn on_screen(&self, bounds: Rectangle) -> Rectangle {
        bounds - self.around.translation
    }

    fn shown(&self, bounds: Rectangle) -> Option<Rectangle> {
        self.on_screen(bounds).intersection(&self.around.clip)
    }

    fn part(&mut self, kind: Kind, bounds: Rectangle, pressable: bool) {
        let shown = self.shown(bounds);
        let bounds = self.on_screen(bounds);
        let Around { mark, scroller, .. } = self.around;
        if let Mode::Walk { parts, .. } = &mut self.mode {
            parts.push(Part {
                kind,
                bounds,
                shown,
                mark,
                pressable,
                words: String::new(),
                scroller,
            });
        }
    }

    /// A row met: whether the pass goes into it, and whether it acts there.
    fn row(&mut self, mark: &mut RowMark, bounds: Rectangle) {
        // Both walks pass over the same rows, so they meet them alike.
        let slot = self.met;
        self.met += 1;
        let Showing { version, behind } = self.showing;
        let skip = behind && mark.shown == version;
        self.pending.skip = skip;
        let bounds = self.on_screen(bounds);
        let quiet = match &mut self.mode {
            Mode::Walk { marks, .. } => {
                // A row passed over is still met: what's outside the rows
                // stays outside.
                marks.push(Met {
                    index: mark.index,
                    key: mark.key,
                    bounds,
                });
                self.pending.mark = Some(slot);
                return;
            }
            Mode::Apply { .. } if skip => return,
            Mode::Apply { orders, .. } => {
                mark.shown = version;
                match orders.plans.get(slot) {
                    Some(plan) => !plan.take(&mut mark.acts),
                    // A row the walk didn't meet: acted on, with no
                    // fingerprint to keep.
                    None => {
                        mark.acts = UNKNOWN;
                        act::acted();
                        false
                    }
                }
            }
            Mode::Clear => !Plan::CLEAR.take(&mut mark.acts),
        };
        self.pending.quiet = Some(quiet);
    }
}

impl Operation for Pass<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let saved = self.around;
        let pending = std::mem::take(&mut self.pending);
        if pending.skip {
            return;
        }
        let around = &mut self.around;
        if let Some(clip) = pending.clip {
            around.clip = around
                .clip
                .intersection(&clip)
                .unwrap_or(Rectangle::new(clip.position(), Size::ZERO));
        }
        around.translation += pending.translation;
        around.scroller = pending.scroller.or(around.scroller);
        around.mark = pending.mark.or(around.mark);
        around.within = pending.within.or(around.within);
        around.quiet = pending.quiet.unwrap_or(around.quiet);
        operate(self);
        self.around = saved;
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content: Size,
        translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        let bounds = self.on_screen(bounds);
        self.pending.clip = Some(bounds);
        self.pending.translation = translation;
        if let Mode::Walk { scrollers, .. } = &mut self.mode {
            scrollers.push(Scroller {
                id: id.cloned(),
                bounds,
            });
            self.pending.scroller = Some(scrollers.len() - 1);
        }
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, _state: &mut dyn Focusable) {
        self.pending = Pending::default();
    }

    fn text_input(&mut self, _id: Option<&Id>, _bounds: Rectangle, _state: &mut dyn TextInput) {
        self.pending = Pending::default();
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, state: &mut dyn Text) {
        self.pending = Pending::default();
        match self.around.within {
            Some((at, Kind::Whole)) => {
                if let Mode::Walk { parts, .. } = &mut self.mode
                    && let Some(part) = parts.get_mut(at)
                {
                    let words = state.text();
                    if !words.is_empty() {
                        if !part.words.is_empty() {
                            part.words.push(' ');
                        }
                        part.words.push_str(&words);
                    }
                }
                return;
            }
            Some((_, Kind::Picture)) => return,
            _ => {}
        }
        let index = self.count;
        self.count += 1;
        let shown = self.shown(bounds);
        let quiet = self.around.quiet;
        let mut pressable = false;
        match &mut self.mode {
            Mode::Walk { probe, .. } => {
                if probe.is_some_and(|probe| shown.is_some_and(|shown| shown.contains(probe))) {
                    // Whether it selects at all: iced's texts that don't,
                    // and a name's, copy nothing.
                    state.select_all();
                    pressable = state.copy().is_some_and(|copied| !copied.is_empty());
                    state.deselect();
                }
            }
            Mode::Apply { .. } | Mode::Clear if quiet => return,
            Mode::Apply { orders, pieces } => {
                // In its own layout's coordinates, as it selects.
                act::act(state, orders.acts.get(index).copied(), bounds, orders.unit);
                if orders.copy.get(index).copied().unwrap_or(false)
                    && let Some(piece) = pieces.get_mut(index)
                {
                    *piece = state.copy();
                }
                return;
            }
            Mode::Clear => {
                state.deselect();
                return;
            }
        }
        self.part(Kind::Text, bounds, pressable);
    }

    fn custom(&mut self, _id: Option<&Id>, bounds: Rectangle, state: &mut dyn Any) {
        self.pending = Pending::default();
        if let Some(mark) = state.downcast_mut::<RowMark>() {
            self.row(mark, bounds);
            return;
        }
        let Some(cover) = state.downcast_mut::<Cover>() else {
            return;
        };
        if self.around.within.is_some() {
            // A mark inside another is the outer one's.
            return;
        }
        let index = self.count;
        self.count += 1;
        let kind = cover.kind;
        if matches!(self.mode, Mode::Walk { .. }) {
            self.part(kind, bounds, kind == Kind::Whole);
        }
        match &self.mode {
            Mode::Walk { .. } => {}
            Mode::Apply { .. } | Mode::Clear if self.around.quiet => {}
            Mode::Apply { orders, .. } => match orders.acts.get(index) {
                Some(Act::Cover(on)) => cover.on = *on,
                Some(Act::Clear) => cover.on = false,
                _ => {}
            },
            Mode::Clear => cover.on = false,
        }
        self.pending.within = Some((index, kind));
    }
}
