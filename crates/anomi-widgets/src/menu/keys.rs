//! Where the pointer and the keys are in a menu and its submenu, and how the
//! keys move them: the `ContextMenu`'s `current`, `sub-open`, `sub-row` and
//! `sub-current`, and its `key-pressed` (`ui/controls.slint:1410`).
//!
//! Up and Down (or Tab) move through the choices, wrapping; Home and End go
//! to the ends; Enter or Space choose; Right opens a row's submenu at its
//! first row and Left leaves it (the other way round when submenus open on
//! the menu's left); Escape closes the submenu, then the menu.

use iced::keyboard::{Key, Modifiers, key::Named};

/// Where the menu stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Nav {
    /// The menu's row lit by the pointer or the keys.
    pub current: Option<usize>,
    /// The row whose submenu shows.
    pub open: Option<usize>,
    /// The submenu's row the keys or the pointer are on; none while they
    /// are in the menu.
    pub sub: Option<usize>,
}

/// A key as the menu reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    Next,
    Previous,
    First,
    Last,
    Choose,
    /// Towards the submenus: Right, or Left when they open on the left.
    Into,
    /// Away from them.
    Out,
    Escape,
}

impl Move {
    /// The move a key makes, if any; `flip` when submenus open on the left.
    pub fn of(key: &Key, modifiers: Modifiers, flip: bool) -> Option<Self> {
        let Key::Named(named) = key else {
            return (key.as_ref() == Key::Character(" ")).then_some(Self::Choose);
        };
        Some(match named {
            Named::ArrowDown => Self::Next,
            Named::ArrowUp => Self::Previous,
            Named::Tab if modifiers.shift() => Self::Previous,
            Named::Tab => Self::Next,
            Named::Home => Self::First,
            Named::End => Self::Last,
            Named::Enter | Named::Space => Self::Choose,
            Named::ArrowRight if flip => Self::Out,
            Named::ArrowRight => Self::Into,
            Named::ArrowLeft if flip => Self::Into,
            Named::ArrowLeft => Self::Out,
            Named::Escape => Self::Escape,
            _ => return None,
        })
    }
}

/// What a key asks the menu for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The lit row moved; the menu shows it.
    Moved,
    /// Choose the menu's row.
    Pick(usize),
    /// Choose the submenu's row.
    PickSub(usize),
    /// The submenu of the menu's row opened.
    Opened(usize),
    /// The submenu closed.
    Closed,
    /// Close the menu.
    Close,
    /// Not the menu's key.
    Ignored,
}

/// What the menu knows of its rows for the keys.
pub(crate) struct Rows<'a> {
    pub len: usize,
    /// The length of each row's submenu, by the row.
    pub submenus: &'a dyn Fn(usize) -> usize,
}

impl Nav {
    /// Answers a key.
    pub fn press(&mut self, key: Move, rows: &Rows<'_>) -> Outcome {
        let sub_len = self.open.map_or(0, |row| (rows.submenus)(row));
        match key {
            Move::Next | Move::Previous => {
                let by = if key == Move::Next { 1 } else { -1 };
                match self.sub {
                    Some(at) if sub_len > 0 => self.sub = Some(wrap(at, by, sub_len)),
                    _ if rows.len == 0 => return Outcome::Ignored,
                    _ => {
                        self.current = Some(match self.current {
                            None if by > 0 => 0,
                            None => rows.len - 1,
                            Some(at) => wrap(at, by, rows.len),
                        });
                    }
                }
                Outcome::Moved
            }
            Move::First | Move::Last => {
                let last = key == Move::Last;
                match self.sub {
                    Some(_) => self.sub = Some(if last { sub_len - 1 } else { 0 }),
                    None if rows.len == 0 => return Outcome::Ignored,
                    None => self.current = Some(if last { rows.len - 1 } else { 0 }),
                }
                Outcome::Moved
            }
            Move::Choose if self.sub.is_some() => Outcome::PickSub(self.sub.unwrap_or(0)),
            // Already in the submenu: there's nothing further in.
            Move::Into if self.sub.is_some() => Outcome::Ignored,
            Move::Choose | Move::Into => match self.current {
                Some(row) if (rows.submenus)(row) > 0 => {
                    self.open = Some(row);
                    self.sub = Some(0);
                    Outcome::Opened(row)
                }
                Some(row) if key == Move::Choose => Outcome::Pick(row),
                _ => Outcome::Ignored,
            },
            Move::Out | Move::Escape if self.open.is_some() => {
                self.open = None;
                self.sub = None;
                Outcome::Closed
            }
            Move::Escape => Outcome::Close,
            Move::Out => Outcome::Ignored,
        }
    }

    /// The pointer came onto a row of the menu: it lights, and its submenu
    /// opens or the open one closes. Whether a submenu opened.
    pub fn hover(&mut self, row: usize, has_submenu: bool) -> bool {
        self.current = Some(row);
        self.sub = None;
        if has_submenu {
            let opened = self.open != Some(row);
            self.open = Some(row);
            opened
        } else {
            self.open = None;
            false
        }
    }

    /// The menu's rows drawn as lit: the current one while the keys are in
    /// the menu, and the one whose submenu shows.
    pub fn lit(&self, row: usize) -> bool {
        (self.sub.is_none() && self.current == Some(row)) || self.open == Some(row)
    }
}

fn wrap(at: usize, by: isize, len: usize) -> usize {
    (at as isize + by).rem_euclid(len as isize) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(submenus: &dyn Fn(usize) -> usize) -> Rows<'_> {
        Rows { len: 4, submenus }
    }

    #[test]
    fn the_keys_wrap_and_reach_the_ends() {
        let none = |_| 0;
        let rows = rows(&none);
        let mut nav = Nav::default();
        assert_eq!(nav.press(Move::Previous, &rows), Outcome::Moved);
        assert_eq!(nav.current, Some(3));
        nav.press(Move::Next, &rows);
        assert_eq!(nav.current, Some(0));
        nav.press(Move::Last, &rows);
        assert_eq!(nav.current, Some(3));
        nav.press(Move::First, &rows);
        assert_eq!(nav.press(Move::Choose, &rows), Outcome::Pick(0));
        assert_eq!(nav.press(Move::Escape, &rows), Outcome::Close);
    }

    #[test]
    fn right_opens_a_submenu_at_its_first_row_and_escape_leaves_it() {
        let apps = |row| if row == 2 { 3 } else { 0 };
        let rows = rows(&apps);
        let mut nav = Nav {
            current: Some(2),
            ..Nav::default()
        };
        assert_eq!(nav.press(Move::Into, &rows), Outcome::Opened(2));
        assert_eq!(nav.sub, Some(0));
        nav.press(Move::Previous, &rows);
        assert_eq!(nav.sub, Some(2));
        // Right again stays where it is, as Slint rejects it there.
        assert_eq!(nav.press(Move::Into, &rows), Outcome::Ignored);
        assert_eq!(nav.sub, Some(2));
        assert!(nav.lit(2));
        assert_eq!(nav.press(Move::Choose, &rows), Outcome::PickSub(2));
        assert_eq!(nav.press(Move::Escape, &rows), Outcome::Closed);
        assert_eq!((nav.open, nav.sub), (None, None));
        assert_eq!(nav.press(Move::Escape, &rows), Outcome::Close);
    }

    #[test]
    fn hovering_opens_submenus_and_closes_them_on_other_rows() {
        let apps = |row| if row == 1 { 2 } else { 0 };
        let mut nav = Nav::default();
        assert!(nav.hover(1, apps(1) > 0));
        assert!(!nav.hover(1, true));
        assert_eq!(nav.open, Some(1));
        nav.hover(3, false);
        assert_eq!((nav.current, nav.open), (Some(3), None));
    }

    #[test]
    fn keys_read_by_side() {
        let none = Modifiers::default();
        let right = Key::Named(Named::ArrowRight);
        assert_eq!(Move::of(&right, none, false), Some(Move::Into));
        assert_eq!(Move::of(&right, none, true), Some(Move::Out));
        let tab = Key::Named(Named::Tab);
        assert_eq!(
            Move::of(&tab, Modifiers::SHIFT, false),
            Some(Move::Previous)
        );
        assert_eq!(
            Move::of(&Key::Character(" ".into()), none, false),
            Some(Move::Choose)
        );
        assert_eq!(Move::of(&Key::Character("a".into()), none, false), None);
    }
}
