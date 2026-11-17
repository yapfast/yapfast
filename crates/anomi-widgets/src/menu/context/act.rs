//! What a context menu does with the pointer and the keys: lighting rows,
//! opening submenus, choosing (`hovered`, `clicked`, `pick` and
//! `key-pressed` of `ContextMenu`, `ui/controls.slint:1332`), and filling
//! the boxes.

use std::time::Instant;

use iced::advanced::Shell;
use iced::animation::Animation;

use anomi_theme::motion::QUICK;
use anomi_vm::menu::Row;

use super::super::keys::{Move, Outcome, Rows};
use super::ContextMenu;
use super::state::{State, Target};

impl<M: Clone> ContextMenu<'_, M> {
    pub(super) fn hover(&self, state: &mut State, target: Option<Target>) {
        match target {
            Some(Target::Main(row)) => {
                let had = state.nav.open.is_some();
                if state.nav.hover(row, !self.submenu(row).is_empty()) {
                    state.opened(self.motion, !had);
                }
            }
            Some(Target::Sub(row)) => state.nav.sub = Some(row),
            None => {}
        }
    }

    pub(super) fn act(&self, state: &mut State, target: Target, shell: &mut Shell<'_, M>) {
        match target {
            Target::Main(index) => {
                let row = &self.rows[index];
                if !row.submenu.is_empty() && row.intent.is_none() {
                    let fresh = state.nav.open.is_none();
                    state.nav.current = Some(index);
                    state.nav.open = Some(index);
                    state.nav.sub = None;
                    state.opened(self.motion, fresh);
                    shell.request_redraw();
                } else {
                    self.pick(row, shell);
                }
            }
            Target::Sub(index) => {
                if let Some(row) = state
                    .nav
                    .open
                    .and_then(|open| self.submenu(open).get(index))
                {
                    self.pick(row, shell);
                }
            }
        }
    }

    /// `pick` (`:1394`): a disabled row does nothing; a box sends its
    /// intent and stays; any other sends its intent and closes.
    fn pick(&self, row: &Row<M>, shell: &mut Shell<'_, M>) {
        if row.disabled {
            return;
        }
        if let Some(intent) = &row.intent {
            shell.publish(intent.clone());
        }
        if !row.checkbox
            && let Some(close) = &self.on_close
        {
            shell.publish(close.clone());
        }
    }

    pub(super) fn key(&self, state: &mut State, key: Move, shell: &mut Shell<'_, M>) {
        let submenus = |row: usize| self.submenu(row).len();
        let rows = Rows {
            len: self.rows.len(),
            submenus: &submenus,
        };
        let had = state.nav.open;
        match state.nav.press(key, &rows) {
            Outcome::Moved => {
                if let (Some(open), Some(sub)) = (state.nav.open, state.nav.sub) {
                    state.subs[open].reveal(sub);
                } else if let Some(current) = state.nav.current {
                    state.main.reveal(current);
                    state.place_sub();
                }
            }
            Outcome::Pick(row) => self.pick(&self.rows[row], shell),
            Outcome::PickSub(row) => {
                if let Some(row) = state.nav.open.and_then(|open| self.submenu(open).get(row)) {
                    self.pick(row, shell);
                }
            }
            Outcome::Opened(row) => {
                state.opened(self.motion, had != Some(row));
                state.subs[row].reveal(0);
            }
            Outcome::Closed => {}
            Outcome::Close => match &self.on_close {
                Some(close) => shell.publish(close.clone()),
                // The layer closes it.
                None => return,
            },
            Outcome::Ignored => return,
        }
        shell.request_redraw();
        shell.capture_event();
    }

    /// Fills the boxes towards their rows' state; whether any still moves.
    pub(super) fn tick(&self, state: &mut State, now: Instant) -> bool {
        let open = state.nav.open.map_or(&[][..], |open| self.submenu(open));
        if state.ticks_for != state.nav.open {
            state.ticks_for = state.nav.open;
            state.ticks.truncate(self.rows.len());
        }
        let rows = self.rows.iter().chain(open);
        let count = self.rows.len() + open.len();
        if state.ticks.len() > count {
            state.ticks.truncate(count);
        }
        let mut moving = false;
        for (index, row) in rows.enumerate() {
            if index == state.ticks.len() {
                state.ticks.push(Animation::new(row.checked));
            }
            let tick = &mut state.ticks[index];
            if tick.value() != row.checked {
                *tick = if self.motion.still() {
                    Animation::new(row.checked)
                } else {
                    Animation::new(!row.checked)
                        .duration(QUICK)
                        .go(row.checked, now)
                };
            }
            moving |= tick.is_animating(now);
        }
        moving
    }
}
