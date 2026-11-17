//! The voice dock, on top of the account card while a call goes or failed.
//! The Slint side is `VoiceDock` (`ui/voice.slint:393`) with `DockButton`
//! (`:326`) and `DockNotice` (`:362`), over `show_voice`
//! (`src/ui/refresh.rs:4592`).
//!
//! From the top: the status (and, in a call, the noise and hang-up buttons),
//! "Your screen" while sharing, the wide buttons for the soundboard, the
//! camera and the screen, then what to watch for or what went wrong, and
//! with no call left, "Dismiss". The dock draws its own ground (the card's
//! panel, its top corners rounded) so the account card goes on from it.
//!
//! It never ticks: the elapsed time is text the app hands it, once a second
//! while the window is in front.
//!
//! Known gaps, from the pinned iced: the camera button doesn't pulse while
//! the camera starts (no layer opacity; it is tinted with the accent and its
//! tooltip says "Starting camera..."), the status line's underline doesn't
//! follow the keyboard focus, and the wide buttons' lit and hovered washes
//! are `IconButton`'s blend of its fill with the overlay.

mod actions;
mod header;
mod notice;
mod share;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{column, container};
use iced::{Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics};
use anomi_vm::voice::dock::{Dock, Intent};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

#[cfg(feature = "shots")]
pub use shots::shots;

/// The column of icons at the left, as wide as the account card's avatar
/// column (`gutter`, `ui/voice.slint:437`).
pub(crate) const GUTTER: f32 = 56.0;
/// `padding-top: 10px; padding-bottom: 8px; padding-right: 8px; spacing: 6px`.
const PAD_TOP: f32 = 10.0;
const PAD_BOTTOM: f32 = 8.0;
const PAD_RIGHT: f32 = 8.0;
const SPACING: f32 = 6.0;
/// Under the gutter's left edge the buttons and notices start 8 px in.
pub(crate) const INSET: f32 = 8.0;

pub fn view(dock: &Dock, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts = column![header::view(dock, motion)].spacing(SPACING);
    if dock.active && dock.sharing {
        parts = parts.push(share::view(dock, motion));
    }
    if dock.active && actions::wanted(dock) {
        parts = parts.push(actions::view(dock, motion));
    }
    if !dock.warning.is_empty() && dock.error.is_empty() {
        parts = parts.push(notice::view(
            &dock.warning,
            notice::Tone::Warning,
            false,
            motion,
        ));
    }
    if !dock.error.is_empty() {
        parts = parts.push(notice::view(
            &dock.error,
            notice::Tone::Error,
            dock.active,
            motion,
        ));
    }
    if !dock.active {
        // During a call the error has its own way out; without one,
        // "Dismiss" closes the dock.
        parts = parts.push(
            container(
                action_button(tr!("Dismiss"), Kind::Ghost, motion)
                    .small()
                    .on_press(Intent::DismissError),
            )
            .padding(Padding::ZERO.left(GUTTER - 10.0))
            .boxed(),
        );
    }
    container(parts)
        .width(Fill)
        .padding(Padding {
            top: PAD_TOP,
            right: PAD_RIGHT,
            bottom: PAD_BOTTOM,
            left: 0.0,
        })
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().panel.into()),
            border: Border::default().rounded(
                iced::border::Radius::default()
                    .top_left(metrics::RADIUS_CARD)
                    .top_right(metrics::RADIUS_CARD),
            ),
            ..catalog::container::transparent(theme)
        })
}
