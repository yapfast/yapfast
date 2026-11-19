//! Menus and what opens from a spot in the window. The Slint side:
//! `MenuItem`, `MenuRows` and `ContextMenu` (`ui/controls.slint:1156`,
//! `:1287`, `:1332`), and the window's placing of its popups by hand
//! (`ui/yapfast.slint:2311` and each popover's `x` and `y`).
//!
//! - [`ContextMenu`]: a menu of [`Row`]s with submenus, placed at a
//!   [`Spot`], flipping and staying in the window, as wide as its widest
//!   row, taking the keyboard. [`layer`] puts the window's (`vm.menu`) in a
//!   layer of [`crate::layer`].
//! - [`Item`]: one row, for menus a screen lays out itself; [`Beside`]
//!   shows such a row's submenu beside the card, and [`Keyed`] hands such a
//!   menu the keys.
//! - [`Opener`]: what a menu opens from, telling where it is in the window.
//! - [`place`]: where a popup goes against its anchor; the layer stack
//!   places popovers and cards with it.
//!
//! Rows are `anomi_vm::menu::Row`s: their text, icon, and the intent each
//! sends.
//!
//! [`Row`]: anomi_vm::menu::Row
//! [`Spot`]: anomi_vm::menu::Spot

mod beside;
mod context;
mod item;
mod keyed;
mod keys;
mod mark;
mod opener;
mod panel;
pub mod place;
mod row;
#[cfg(test)]
mod tests;
pub mod text;

pub use beside::{Beside, beside};
pub use context::{ContextMenu, context_menu};
pub use item::{Item, item};
pub use keyed::{Keyed, keyed};
pub use opener::{Opener, opener};
pub use panel::{NARROWEST, size, width};

use anomi_theme::Motion;
use anomi_vm::Intent;
use anomi_vm::menu::{self, Menu};

use crate::layer::{self, Backdrop, Layer, Place};

/// The window's context menu as a layer over the window, while it shows:
/// open, or playing its way out. Escape, a press outside it or a choice
/// sends `menu::Intent::Close`; once gone, `menu::Intent::Gone`.
pub fn layer(menu: &Menu, motion: Motion) -> Option<Layer<'_, Intent>> {
    if !menu.shown() {
        return None;
    }
    let close = Intent::Menu(menu::Intent::Close);
    let content = context_menu(&menu.rows, menu.at, motion)
        .narrowest(menu.narrowest.unwrap_or(NARROWEST))
        .headed(menu.headed)
        .submenu_shown(menu.submenu)
        .on_close(close.clone());
    Some(
        layer::layer(("menu", menu.serial), content)
            .open(menu.open)
            .place(Place::Fill)
            .backdrop(Backdrop::Clear)
            .grow_from(place::near(menu.at))
            .on_close(close)
            .on_gone(Intent::Menu(menu::Intent::Gone)),
    )
}
