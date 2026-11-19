//! While live, the share button's menu is the stream's own, as Discord's
//! client has it: "Stream Quality", whose submenu beside the card has the
//! frame rates and the resolutions with radio marks (and the fork's
//! encoding under them); "Sound", whose submenu has all applications or
//! none; then "Change Windows" and "Stop Streaming". A choice in a submenu
//! changes the stream at once and the menu stays open.
//!
//! The rows are made by `anomi_voice::share::built` (the text translated
//! there), each time the picks change; which submenu shows and where the
//! keys are is the menu's own state, which [`Live::apply`] moves, as
//! `voice::devices` moves the device menus'.

use crate::menu::{Key, Row};

use super::Intent;

/// A row of the stream's menu the pointer and the keys reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// "Stream Quality", its submenu the frame rates, the resolutions and
    /// the encoding.
    Quality,
    /// "Sound", its submenu all applications or none.
    Sound,
    /// "Change Windows": what's shared, picked again.
    Change,
    /// "Stop Streaming".
    Stop,
}

impl Part {
    /// The rows, top to bottom.
    pub const ALL: [Self; 4] = [Self::Quality, Self::Sound, Self::Change, Self::Stop];
}

/// The stream's menu: its rows and where the pointer and the keys are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Live {
    /// "Stream Quality" over the quality streamed ("1080p · 60 fps").
    pub quality: Row<Intent>,
    /// "Sound" over the sound streamed.
    pub sound: Row<Intent>,
    pub change: Row<Intent>,
    pub stop: Row<Intent>,
    /// The stream is live, not choosing or starting: what's shared can be
    /// changed only then, as the core switches nothing before.
    pub changeable: bool,
    /// The row whose submenu shows.
    pub open: Option<Part>,
    /// The row the keys or the pointer last lit.
    pub current: Option<Part>,
    /// The open submenu's row the keys light, while they're in it.
    pub sub: Option<usize>,
}

impl Live {
    /// The menu, `changeable` or not, before its rows are made.
    pub fn new(changeable: bool) -> Self {
        Self {
            changeable,
            ..Self::default()
        }
    }

    pub fn row(&self, part: Part) -> &Row<Intent> {
        match part {
            Part::Quality => &self.quality,
            Part::Sound => &self.sound,
            Part::Change => &self.change,
            Part::Stop => &self.stop,
        }
    }

    /// The submenu of a row; none for the last two.
    pub fn submenu(&self, part: Part) -> &[Row<Intent>] {
        &self.row(part).submenu
    }

    /// A row of the menu drawn lit: the one whose submenu shows, and the
    /// one the keys are on while they're in the menu.
    pub fn lit(&self, part: Part) -> bool {
        self.open == Some(part) || (self.sub.is_none() && self.current == Some(part))
    }

    /// What the pointer or a key did to the menu's own state. A choice the
    /// keys made comes back, for the app to do as the click would.
    pub fn apply(&mut self, intent: &Intent) -> Option<Intent> {
        match intent {
            Intent::Hover(part) => {
                self.current = *part;
                self.sub = None;
                self.open = part.filter(|&part| !self.submenu(part).is_empty());
                None
            }
            Intent::Key(key) => self.press(*key),
            _ => None,
        }
    }

    /// Made again, the keys stay where its rows still are.
    pub fn settle(&mut self) {
        let rows = self.open.map_or(0, |part| self.submenu(part).len());
        self.sub = self.sub.filter(|&at| at < rows);
        if self.current.is_some_and(|part| self.row(part).disabled) {
            self.current = None;
        }
    }

    fn press(&mut self, key: Key) -> Option<Intent> {
        if let (Some(open), Some(at)) = (self.open, self.sub) {
            let rows = self.submenu(open);
            match key {
                Key::Up | Key::Down => self.sub = Some(next(rows, at, key)),
                Key::Enter => {
                    let row = rows.get(at).filter(|row| row.choosable())?;
                    return row.intent.clone();
                }
                Key::Left | Key::Escape => (self.open, self.sub) = (None, None),
                Key::Right => {}
            }
            return None;
        }
        // "Change Windows" is skipped while it can't be chosen.
        let parts: Vec<Part> = (Part::ALL.into_iter())
            .filter(|&part| !self.row(part).disabled)
            .collect();
        let at = (self.current).and_then(|part| parts.iter().position(|&p| p == part));
        match (key, self.current) {
            (Key::Up | Key::Down, _) if !parts.is_empty() => {
                let next = match at {
                    Some(at) => step(at, key, parts.len()),
                    None if key == Key::Down => 0,
                    None => parts.len() - 1,
                };
                self.current = Some(parts[next]);
                if self.open != self.current {
                    self.open = None;
                }
            }
            (Key::Right | Key::Enter, Some(part)) if !self.submenu(part).is_empty() => {
                let rows = self.submenu(part);
                self.sub = rows.iter().position(|row| row.choosable());
                self.open = Some(part);
            }
            (Key::Enter, Some(part)) => {
                let row = self.row(part);
                return row.intent.clone().filter(|_| row.choosable());
            }
            (Key::Left | Key::Escape, _) => self.open = None,
            _ => {}
        }
        None
    }
}

/// The place one up or down from `at` among `len`, wrapping.
fn step(at: usize, key: Key, len: usize) -> usize {
    let by: isize = if key == Key::Up { -1 } else { 1 };
    (at as isize + by).rem_euclid(len.max(1) as isize) as usize
}

/// The next row up or down from `at` that can be chosen, past the groups'
/// titles, wrapping; `at` itself when no other can.
fn next(rows: &[Row<Intent>], at: usize, key: Key) -> usize {
    let mut to = at;
    for _ in 0..rows.len() {
        to = step(to, key, rows.len());
        if rows[to].choosable() {
            return to;
        }
    }
    at
}

#[cfg(test)]
mod tests;
