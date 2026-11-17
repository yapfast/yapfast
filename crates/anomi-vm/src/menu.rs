//! The context menu open over the window, with its rows: what it shows,
//! where it opened, and what it asks for. The Slint side: the `MenuRow`
//! struct and `ContextMenu` (`ui/controls.slint:1257`, `:1332`), the window's
//! `context-menu` popup (`ui/yapfast.slint:2311`), and the rows' shapes in
//! `menu` (`src/ui/menus.rs:110`).
//!
//! A row carries the intent choosing it sends, so a menu needs no table of
//! ids: a row of a server's menu sends what the server's own button would.
//! The menus whose choices `src/ui/menus.rs` reads by id (`choose_menu`) send
//! [`Intent::Choose`] with that id. A [`Row`] is generic over the intent so
//! an area's own menu (the message menu, a picker's) can carry its area's.
//!
//! [`Spot`] says where something opens over the window: at the pointer, or
//! from a button, preferring a side and flipping to the other when it
//! doesn't fit. Menus, popovers and cards share it; `anomi_widgets::menu`
//! places them.

use iced::{Point, Rectangle, Size};

use crate::icon::Icon;

/// The context menu open over the window. The window holds it as
/// `vm.menu`; `anomi_widgets::menu::layer` draws it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {
    /// Which menu this is: the app counts up for each one it opens, so a
    /// new menu arrives anew even where the last one was.
    pub serial: u64,
    /// It shows; `false` while it plays its way out, with its rows kept
    /// until it asks [`Intent::Gone`].
    pub open: bool,
    /// Where it opened: the pointer for a right click, or a button.
    pub at: Spot,
    pub rows: Vec<Row>,
}

impl Menu {
    /// A menu open at a spot.
    pub fn new(serial: u64, at: Spot, rows: Vec<Row>) -> Self {
        Self {
            serial,
            open: true,
            at,
            rows,
        }
    }

    /// Whether there is anything to draw: open, or on its way out.
    pub fn shown(&self) -> bool {
        !self.rows.is_empty()
    }
}

/// A choice in a menu: `MenuRow` in `ui/controls.slint:1257`.
#[derive(Clone, Debug, PartialEq)]
pub struct Row<I = crate::Intent> {
    pub text: String,
    /// A second line under the text, such as what its submenu has chosen.
    pub subtitle: String,
    /// Its icon, drawn 16 px wide before its text.
    pub icon: Option<Icon>,
    /// What choosing it sends. A row with a submenu and no intent opens its
    /// submenu when clicked; with one, as a mute whose submenu says for how
    /// long, a click sends it and the submenu opens on hover alone.
    pub intent: Option<I>,
    /// A destructive choice, drawn in red.
    pub danger: bool,
    /// It starts a group, under a line.
    pub separated: bool,
    /// It turns something on or off, with a box ticked while it's on;
    /// choosing it leaves the menu open.
    pub checkbox: bool,
    /// Ticked, for a checkbox; otherwise the current choice, marked with a
    /// check in place of its icon and in bold.
    pub checked: bool,
    /// Greyed, and not to be chosen.
    pub disabled: bool,
    /// No room kept for an icon when it has none, as Discord's server and
    /// person menus have none on most rows.
    pub plain: bool,
    /// The second line wraps rather than being cut, and the row grows to
    /// hold it, for a subtitle that says what a choice costs.
    pub wraps: bool,
    /// The rows of its submenu; none for a row without one.
    pub submenu: Vec<Row<I>>,
}

impl<I> Default for Row<I> {
    fn default() -> Self {
        Self {
            text: String::new(),
            subtitle: String::new(),
            icon: None,
            intent: None,
            danger: false,
            separated: false,
            checkbox: false,
            checked: false,
            disabled: false,
            plain: false,
            wraps: false,
            submenu: Vec::new(),
        }
    }
}

impl<I> Row<I> {
    /// A row that sends `intent` when chosen.
    pub fn new(text: impl Into<String>, intent: I) -> Self {
        Self {
            text: text.into(),
            intent: Some(intent),
            ..Self::default()
        }
    }

    /// A row whose click opens its submenu.
    pub fn opens(text: impl Into<String>, submenu: Vec<Row<I>>) -> Self {
        Self {
            text: text.into(),
            submenu,
            ..Self::default()
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn separated(mut self) -> Self {
        self.separated = true;
        self
    }

    /// A box to tick, ticked while `on`.
    pub fn checkbox(mut self, on: bool) -> Self {
        self.checkbox = true;
        self.checked = on;
        self
    }

    /// The current choice, or not.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn plain(mut self) -> Self {
        self.plain = true;
        self
    }

    pub fn wraps(mut self) -> Self {
        self.wraps = true;
        self
    }

    pub fn submenu(mut self, submenu: Vec<Row<I>>) -> Self {
        self.submenu = submenu;
        self
    }

    /// The current choice, drawn with a check and in bold: checked and not
    /// a box to tick.
    pub fn current(&self) -> bool {
        self.checked && !self.checkbox
    }

    /// Whether choosing it does anything.
    pub fn choosable(&self) -> bool {
        !self.disabled && (self.intent.is_some() || !self.submenu.is_empty())
    }
}

/// Where a menu, a popover or a card opens over the window, in the window's
/// logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    /// What it opens from: a button's bounds, or the pointer as a rectangle
    /// of no size.
    pub anchor: Rectangle,
    /// The side it prefers; it flips to the other when it doesn't fit there.
    pub side: Side,
    /// How it lines up along that side.
    pub align: Align,
    /// The space between it and what it opens from.
    pub gap: f32,
}

/// Between a button and what it opens: `y + height + 6px` where the window
/// opens a menu from its server header and bell (`ui/yapfast.slint:2840`,
/// `:3357`) and its popovers (`:4893`).
pub const GAP: f32 = 6.0;

/// Between a row or a mention and the card beside it: `x + width + 8px`,
/// else `x - card - 8px`, in the window's `beside` (`ui/yapfast.slint:2237`),
/// which places the member card, the role card and the server card.
pub const GAP_BESIDE: f32 = 8.0;

impl Default for Spot {
    fn default() -> Self {
        Self::pointer(Point::ORIGIN)
    }
}

impl Spot {
    /// A right click's: its top left corner at the pointer, or its bottom
    /// left when it doesn't fit below.
    pub fn pointer(at: Point) -> Self {
        Self {
            anchor: Rectangle::new(at, Size::ZERO),
            side: Side::Below,
            align: Align::Start,
            gap: 0.0,
        }
    }

    /// Under a button, its left edge on the button's, as the server
    /// header's menu and the bell's open; above it when there's no room.
    pub fn below(anchor: Rectangle) -> Self {
        Self {
            anchor,
            side: Side::Below,
            align: Align::Start,
            gap: GAP,
        }
    }

    /// Over a button, as the composer's pickers and the status menu open.
    pub fn above(anchor: Rectangle) -> Self {
        Self {
            side: Side::Above,
            ..Self::below(anchor)
        }
    }

    /// Beside what opened it, [`GAP_BESIDE`] away on its right or else its
    /// left, its top on the anchor's: a member's card by the member list.
    pub fn beside(anchor: Rectangle) -> Self {
        Self {
            side: Side::Right,
            gap: GAP_BESIDE,
            ..Self::below(anchor)
        }
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

/// The side of its anchor a popup prefers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    #[default]
    Below,
    Above,
    Right,
    Left,
}

/// How a popup lines up along its anchor's side: its start (left or top)
/// on the anchor's, its centre on the anchor's, or its end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Centre,
    End,
}

/// A choice of a menu the app reads by id, as the Slint window's
/// `choose-menu(target, person, id)` does (`src/ui/menus.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// What the menu is about: a server's, a channel's or a person's id,
    /// with the prefixes of `src/ui/menus.rs` (`header:`, `notifications:`...).
    pub target: String,
    /// The menu is a person's.
    pub person: bool,
    /// The row's id.
    pub id: String,
}

impl Choice {
    pub fn new(target: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            person: false,
            id: id.into(),
        }
    }

    /// A choice in a person's menu.
    pub fn person(target: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            person: true,
            ..Self::new(target, id)
        }
    }
}

/// What the context menu asks for. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Escape, a click outside it, or a choice made: it closes, playing its
    /// way out, and keeps its rows until [`Intent::Gone`].
    Close,
    /// Its way out ended: the app may let its rows go.
    Gone,
    /// A row of a menu the app reads by id.
    Choose(Choice),
}

impl From<Choice> for Intent {
    fn from(choice: Choice) -> Self {
        Self::Choose(choice)
    }
}

impl From<Choice> for crate::Intent {
    fn from(choice: Choice) -> Self {
        Self::Menu(Intent::Choose(choice))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticked_box_is_not_the_current_choice() {
        let level: Row<()> = Row::new("All messages", ()).checked(true);
        assert!(level.current());
        let switch: Row<()> = Row::new("Hide muted channels", ()).checkbox(true);
        assert!(switch.checked && !switch.current());
    }

    #[test]
    fn a_row_without_intent_or_submenu_does_nothing() {
        assert!(!Row::<()>::default().choosable());
        assert!(Row::<()>::opens("Apps", vec![Row::new("Quote", ())]).choosable());
        assert!(!Row::new("Kick", ()).disabled(true).choosable());
    }

    #[test]
    fn a_button_spot_keeps_the_window_gap() {
        let button = Rectangle::new(Point::new(80.0, 8.0), Size::new(248.0, 32.0));
        let spot = Spot::below(button);
        assert_eq!(
            (spot.side, spot.align, spot.gap),
            (Side::Below, Align::Start, GAP)
        );
        assert_eq!(Spot::above(button).side, Side::Above);
        let beside = Spot::beside(button);
        assert_eq!((beside.side, beside.gap), (Side::Right, GAP_BESIDE));
        assert_eq!(
            Spot::pointer(Point::new(3.0, 4.0)).anchor.size(),
            Size::ZERO
        );
    }

    #[test]
    fn a_choice_lifts_into_the_window_intent() {
        let intent = crate::Intent::from(Choice::new("header:s1", "invite"));
        assert_eq!(
            intent,
            crate::Intent::Menu(Intent::Choose(Choice::new("header:s1", "invite")))
        );
    }
}
