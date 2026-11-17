//! The interface's buttons and toggles, one module each: [`action`] (the
//! labelled button), [`ping`] (the reply bar's "@ On"), [`icon_button`] (an
//! icon with a tooltip), [`device`] (a toggle with its device menu's caret),
//! [`call`] (the round call button with its caption) and [`header`] (a
//! labelled toggle of a conversation's header). The Slint side is
//! `ui/controls.slint`; each module names its component and line.
//!
//! All six are one widget, [`Face`] (`face.rs`): a leaf that draws its
//! fill, icons and text itself, so the colours that follow the pointer
//! animate in one place. The modules choose its look, its size and its
//! tooltip. What every one has, as the Slint originals do:
//!
//! - **Hover and pressed**: the fill and ink ease over `QUICK` and the face
//!   gives to 0.97 while pressed, on `ARRIVE`; with a still [`Motion`]
//!   they land at once and nothing asks for a frame.
//! - **Disabled**: no hover, no press, no focus; each look says how it fades.
//! - **Focus**: an accent ring of 2 px. It is reached by Tab (the face is a
//!   `Focusable`; F17 sends Tab), and Space or Enter then presses it. A click
//!   does not focus it, as in the Slint app.
//! - **Tooltip**: a card above, right of or below the face, the window's
//!   one tooltip ([`crate::tip`]) laid over the face by [`Face::tipped`]
//!   (`tipped.rs`), hidden by a click until the pointer leaves and returns.
//!
//! Text the person reads is the caller's, already translated, except the
//! ping toggle's own "@ On" and "@ Off".
//!
//! [`Motion`]: anomi_theme::Motion

pub mod action;
pub mod call;
mod caret;
pub mod device;
pub mod face;
pub mod header;
pub mod icon_button;
mod look;
mod paint;
mod paint_face;
pub mod ping;
mod state;
mod tipped;
mod widget;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_face;
#[cfg(test)]
mod tests_tip;

pub use crate::tip::Side;
pub use action::{Kind, action_button};
pub use call::call_button;
pub use caret::chevron;
pub use device::{DeviceToggle, device_toggle};
pub use face::Face;
pub use header::{HeaderToggle, header_toggle};
pub use icon_button::{IconButton, icon_button};
pub use look::Colours;
pub use ping::ping_toggle;
