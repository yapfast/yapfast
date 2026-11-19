//! The share menu (`ShareMenu`, `ui/voice.slint:1414`; the popup around it,
//! `ui/yapfast.slint:2147`): going live is choosing what to share (the
//! screens as cards with their pictures, then windows, the system's dialog
//! and cameras), its sound, and the resolution, frame rate and quality to
//! share it at; while live it switches what's shared, and a choice of
//! sound, resolution, frame rate or encoding changes the stream at once
//! (Slint's menu hid the quality while live; the owner asked to change it
//! on the fly). "Go Live" (or "Switch") stays under the choices, which
//! scroll in a window too short for them.
//!
//! The shell draws it for `Layer::Share` from the button that opened it; it
//! grows from there (the layer's look). The picks live in [`Share`], so a
//! click asks and the app projects the menu again; the view ticks nothing
//! and the card pictures are handles the app made once.

mod card;
mod parts;
mod rows;
#[cfg(feature = "shots")]
mod shots;

use iced::advanced::widget::Id;
use iced::widget::{column, container};
use iced::{Color, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme};
use anomi_vm::icon::Icon;
use anomi_vm::voice::share::{Intent, Share};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

pub use rows::{built, resolution};
#[cfg(feature = "shots")]
pub use shots::shots;

/// The menu's width, and with the screens' pictures the wider one
/// (`ui/yapfast.slint:2154`).
pub const WIDTH: f32 = 280.0;
pub const WIDE: f32 = 336.0;
/// Inside the card, around the choices (`padding: 6px`, `:1445`).
const PADDING: f32 = 6.0;
/// The button's id, for focus and tests.
pub const GO: &str = "share-go";
/// The card's shadow (`drop-shadow-*`, `ui/voice.slint:1456`).
const SHADOW_ALPHA: f32 = 0.5;
const SHADOW_OFFSET: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;
/// Around the button (`padding: 4px; padding-top: 10px`, `:1565`).
const START: [f32; 4] = [10.0, 4.0, 4.0, 4.0];

pub fn view(share: &Share, motion: Motion) -> impl View<Intent> + '_ {
    let width = if share.wide() { WIDE } else { WIDTH };
    let choices = scroll_view(parts::choices(share, width - 2.0 * PADDING, motion), motion)
        .height(Length::Fit.stack(Length::Fill.max(0.0)));
    let label = if share.switching {
        tr!("Switch")
    } else {
        tr!("Go Live")
    };
    let mut go = action_button(label, Kind::Primary, motion)
        .leading(Icon::ScreenShare)
        .id(Id::new(GO))
        .width(Length::Fill)
        .enabled(share.can_go());
    if let Some(intent) = share.go() {
        go = go.on_press(intent);
    }
    let start = container(go).padding(iced::Padding {
        top: START[0],
        right: START[1],
        bottom: START[2],
        left: START[3],
    });
    container(column![choices, start])
        .padding(PADDING)
        .width(width)
        .style(menu)
}

/// The raised card, with its shadow.
fn menu(theme: &Theme) -> container::Style {
    container::Style {
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
            offset: Vector::new(0.0, SHADOW_OFFSET),
            blur_radius: SHADOW_BLUR,
        },
        ..anomi_theme::catalog::container::raised(theme)
    }
}
