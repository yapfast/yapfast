//! The keys a menu a screen lays out itself moves by, as
//! `anomi_widgets::menu::keyed` reads them for it: the view-model keeps
//! where the keys are, so the rows it lays out light where they say.

/// A key a menu laid out by hand answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    /// Into a row's submenu.
    Right,
    /// Out of a submenu.
    Left,
    /// Enter or Space: choose.
    Enter,
    /// Out of a submenu; with none open, the menu's layer closes it.
    Escape,
}
