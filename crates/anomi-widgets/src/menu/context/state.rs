//! What a context menu keeps between views: its panels laid out, measured
//! again only when their rows change, where the pointer and the keys are,
//! the submenu fading in, the boxes filling.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use iced::animation::Animation;
use iced::{Point, Rectangle, Size};

use anomi_theme::Motion;
use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_vm::menu::Row;

use super::super::keys::Nav;
use super::super::panel::{PADDING, Panel};
use super::super::place::MARGIN;
use super::super::row;
use super::GAP;

/// A row of the menu or of the open submenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Main(usize),
    Sub(usize),
}

pub(super) struct State {
    pub main: Panel,
    /// Each row's submenu, laid out; empty for a row without one.
    pub subs: Vec<Panel>,
    /// What the panels were measured for (see [`key`]).
    measured: Option<u64>,
    /// Submenus open on the menu's left.
    pub flip: bool,
    pub window: Size,
    pub nav: Nav,
    /// The row under the pointer, and the one a press began on.
    pub pointer: Option<Target>,
    pub pressed: Option<Target>,
    /// The submenu fading in.
    pub fade: Animation<bool>,
    /// The boxes filling as they're ticked: the menu's rows, then the open
    /// submenu's, of the row they were kept for.
    pub ticks: Vec<Animation<bool>>,
    pub ticks_for: Option<usize>,
    pub now: Instant,
}

impl State {
    pub fn new() -> Self {
        Self {
            main: Panel::default(),
            subs: Vec::new(),
            measured: None,
            flip: false,
            window: Size::ZERO,
            nav: Nav::default(),
            pointer: None,
            pressed: None,
            fade: Animation::new(false),
            ticks: Vec::new(),
            ticks_for: None,
            now: Instant::now(),
        }
    }

    /// Lays the menu's and its submenus' rows out again if they changed,
    /// keeping how far each scrolled, and fits each to `tallest`.
    pub fn measure<M>(&mut self, rows: &[Row<M>], narrowest: f32, tallest: f32) {
        let key = key(rows, narrowest);
        if self.measured != Some(key) {
            self.measured = Some(key);
            self.main = Panel::new(rows, narrowest).keep(&self.main);
            let old = std::mem::take(&mut self.subs);
            self.subs = rows
                .iter()
                .enumerate()
                .map(
                    |(index, row)| match (row.submenu.as_slice(), old.get(index)) {
                        ([], _) => Panel::default(),
                        (rows, Some(old)) => Panel::new(rows, narrowest).keep(old),
                        (rows, None) => Panel::new(rows, narrowest),
                    },
                )
                .collect();
        }
        self.main.fit(tallest);
        for sub in &mut self.subs {
            sub.fit(tallest);
        }
    }

    pub fn target(&self, point: Point) -> Option<Target> {
        if let Some(open) = self.nav.open
            && let Some(row) = self.subs.get(open).and_then(|sub| sub.hit(point))
        {
            return Some(Target::Sub(row));
        }
        self.main.hit(point).map(Target::Main)
    }

    pub fn over(&self, point: Point) -> bool {
        self.main.bounds.contains(point) || self.sub().is_some_and(|sub| sub.bounds.contains(point))
    }

    pub fn sub(&self) -> Option<&Panel> {
        self.nav.open.and_then(|open| self.subs.get(open))
    }

    /// Lines the open submenu's first row up with its row, inside the window.
    pub fn place_sub(&mut self) {
        let Some(open) = self.nav.open else { return };
        let main = &self.main;
        let top = main.bounds.y + (main.shown_top(open) - PADDING).max(0.0);
        let x_right = main.bounds.x + main.bounds.width + GAP;
        let Some(sub) = self.subs.get_mut(open) else {
            return;
        };
        let size = sub.bounds.size();
        let y = top
            .min(self.window.height - MARGIN - size.height)
            .max(MARGIN);
        let x = if self.flip {
            main.bounds.x - GAP - size.width
        } else {
            x_right
        };
        sub.bounds = Rectangle::new(Point::new(x, y), size);
    }

    /// A submenu shows; it fades in unless one already showed.
    pub fn opened(&mut self, motion: Motion, fresh: bool) {
        if fresh {
            self.fade = if motion.still() {
                Animation::new(true)
            } else {
                Animation::new(false)
                    .duration(QUICK)
                    .easing(ARRIVE)
                    .go(true, Instant::now())
            };
        }
        self.place_sub();
    }
}

/// What laying out the menu depends on, hashed: each row's measure, its
/// submenu's rows, the least width.
fn key<M>(rows: &[Row<M>], narrowest: f32) -> u64 {
    fn walk<M>(rows: &[Row<M>], hasher: &mut DefaultHasher) {
        rows.len().hash(hasher);
        for row in rows {
            row::hash(row, hasher);
            walk(&row.submenu, hasher);
        }
    }
    let mut hasher = DefaultHasher::new();
    narrowest.to_bits().hash(&mut hasher);
    walk(rows, &mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use iced::mouse::ScrollDelta;

    use super::super::super::panel::NARROWEST;
    use super::*;

    fn rows(last: &str) -> Vec<Row<()>> {
        (0..30)
            .map(|n| Row::new(format!("Row {n}"), ()))
            .chain([Row::new(last, ())])
            .collect()
    }

    #[test]
    fn a_resize_refits_the_panels_and_only_new_rows_are_measured_again() {
        let mut state = State::new();
        state.measure(&rows("Last"), NARROWEST, 300.0);
        let measured = state.measured;
        state.main.wheel(ScrollDelta::Lines { x: 0.0, y: -100.0 });
        assert!(state.main.scroll > 500.0, "scrolled to the end");

        // Taller: the same rows, fitted, their scroll kept within reach.
        state.measure(&rows("Last"), NARROWEST, 1000.0);
        assert_eq!(state.measured, measured);
        assert_eq!(state.main.bounds.height, 1000.0);
        assert!(state.main.scroll < 100.0, "{}", state.main.scroll);

        let long = "A row whose text asks for more than the least width";
        state.measure(&rows(long), NARROWEST, 1000.0);
        assert_ne!(state.measured, measured);
        assert!(state.main.bounds.width > NARROWEST);
    }
}
