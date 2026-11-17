//! A server, a folder or the DM home on the rail: a tile with a pill at the
//! rail's edge that grows as the item is hovered and selected, and at its
//! corners a mention count, a call's speaker and Discord's mark. The Slint
//! side: `RailItem` and `FolderItem` in `ui/rail.slint`.
//!
//! It is the reference for a widget of our own: its hover and its animations
//! live in its tree state, it asks for frames only while one plays, and with
//! a still [`Motion`] it lands at once.
//!
//! - A tile shows a [`Face`]: initials, a picture, a symbol or, for a
//!   folder, its colour with its servers in it. A picture's corners are
//!   cleared by [`Player`], since tiny-skia ignores an image's radius.
//! - Tab reaches it, Space or Enter presses it, and the ring shows while it
//!   has the focus; a click doesn't focus it, as in the Slint app. A click
//!   anywhere on its row sends, on release. The menu from a right click is
//!   the caller's (wrap it in `menu::opener`); the Menu key and Shift+F10
//!   send [`RailItem::on_menu`].
//! - [`RailItem::fade`] dims the whole item (a server in an outage).

mod paint;
#[cfg(test)]
mod tests;
mod widget;

use iced::widget::image;
use iced::{Color, Point, Rectangle, Size};

use anomi_theme::{Motion, metrics};

/// A folder tile's corners (`ClientStyle.folder-radius`).
const FOLDER_RADIUS: f32 = 16.0;

/// Where the menu opens from the keyboard, from the item's origin
/// (`ui/rail.slint`).
const MENU_AT: (f32, f32) = (60.0, 24.0);

/// A closed folder's server in its tile.
#[derive(Clone, Debug)]
pub struct Preview<'a> {
    pub initial: &'a str,
    pub picture: Option<image::Handle>,
}

/// What fills the tile.
#[derive(Clone, Debug)]
pub enum Face<'a> {
    /// A server's initials on the raised ground, or on the accent when lit.
    Initials(&'a str),
    /// A server's icon or the app's logo, filling the tile.
    Picture(image::Handle),
    /// A symbol, as the "add a server" item's plus.
    Icon(iced::widget::svg::Handle),
    /// A folder: a tile of its colour holding its first four servers while
    /// closed, a folder in its colour while open.
    Folder {
        open: bool,
        tint: Color,
        previews: Vec<Preview<'a>>,
    },
}

pub struct RailItem<'a, Message> {
    pub(super) face: Face<'a>,
    pub(super) selected: bool,
    pub(super) unread: bool,
    pub(super) badge: &'a str,
    pub(super) call: bool,
    pub(super) discord: bool,
    pub(super) accent_icon: bool,
    pub(super) fade: f32,
    pub(super) on_press: Option<Message>,
    pub(super) on_menu: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    pub(super) motion: Motion,
}

/// Builds a rail item; see [`RailItem`]'s setters.
pub fn rail_item<'a, Message>(face: Face<'a>, motion: Motion) -> RailItem<'a, Message> {
    RailItem {
        face,
        selected: false,
        unread: false,
        badge: "",
        call: false,
        discord: false,
        accent_icon: false,
        fade: 1.0,
        on_press: None,
        on_menu: None,
        motion,
    }
}

impl<'a, Message> RailItem<'a, Message> {
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn unread(mut self, unread: bool) -> Self {
        self.unread = unread;
        self
    }

    /// Mentions waiting, as "3" or "99+"; empty for none.
    pub fn badge(mut self, badge: &'a str) -> Self {
        self.badge = badge;
        self
    }

    /// In a call here: the speaker at the top right.
    pub fn call(mut self, call: bool) -> Self {
        self.call = call;
        self
    }

    /// Discord's mark at the top left.
    pub fn discord(mut self, discord: bool) -> Self {
        self.discord = discord;
        self
    }

    /// A symbol face drawn in the accent while the item isn't lit.
    pub fn accent_icon(mut self, accent: bool) -> Self {
        self.accent_icon = accent;
        self
    }

    /// How opaque the whole item is, 0 to 1.
    pub fn fade(mut self, fade: f32) -> Self {
        self.fade = fade;
        self
    }

    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }

    /// What the Menu key and Shift+F10 send while it has the focus: the
    /// point in the window the menu opens at.
    pub fn on_menu(mut self, on_menu: impl Fn(Point) -> Message + 'a) -> Self {
        self.on_menu = Some(Box::new(on_menu));
        self
    }

    pub(super) fn folder(&self) -> bool {
        matches!(self.face, Face::Folder { .. })
    }

    /// The tile's side, which is the item's height.
    pub(super) fn side(&self) -> f32 {
        if self.folder() {
            metrics::FOLDER_TILE
        } else {
            metrics::RAIL_TILE
        }
    }

    /// The tile's corners: a folder's are rounder.
    pub(super) fn radius(&self) -> f32 {
        if self.folder() {
            FOLDER_RADIUS
        } else {
            metrics::RADIUS_TILE
        }
    }

    /// How tall the pill wants to be: a folder's goes while it is open.
    pub(super) fn pill_target(&self, hovered: bool) -> f32 {
        if matches!(self.face, Face::Folder { open: true, .. }) {
            0.0
        } else if self.selected {
            metrics::RAIL_PILL_SELECTED
        } else if hovered {
            metrics::RAIL_PILL_HOVERED
        } else if self.unread {
            metrics::RAIL_PILL_UNREAD
        } else {
            0.0
        }
    }

    /// Whether it shows lit: a folder only under the pointer, since its
    /// selection is its server's.
    pub(super) fn lit(&self, hovered: bool) -> bool {
        hovered || (self.selected && !self.folder())
    }

    /// How many pictures it plays.
    pub(super) fn pictures(&self) -> usize {
        match &self.face {
            Face::Picture(_) => 1,
            Face::Folder { previews, .. } => previews.len().min(4),
            _ => 0,
        }
    }

    pub(super) fn tile(&self, bounds: Rectangle) -> Rectangle {
        let side = self.side();
        Rectangle::new(
            Point::new(bounds.x + (bounds.width - side) / 2.0, bounds.y),
            Size::new(side, side),
        )
    }
}
