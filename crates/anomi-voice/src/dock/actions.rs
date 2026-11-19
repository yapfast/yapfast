//! The wide buttons under the status (`DockButton`, `ui/voice.slint:326`,
//! used at `:546`): the soundboard, the camera and the screen, as Discord's
//! panel has them, lit while what they start is on. Each is 32 px tall with
//! 8 px corners and a wash over the panel; they share the row's width.
//!
//! The wash is `IconButton`'s fill: 5% of the overlay at rest, the live
//! colour's 22% when on. With no camera to film with, the camera's button
//! looks unavailable, as Discord's does: half the wash and a faint picture
//! that stays faint under the pointer, "Camera unavailable" in its tooltip.

use iced::widget::row;
use iced::{Color, Fill, Padding};

use anomi_theme::Palette;
use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_vm::voice::dock::{Dock, Intent};
use anomi_widgets::button::icon_button;
use anomi_widgets::menu::opener;
use anomi_widgets::tip::Side;
use yapfast_lang::tr;

const HEIGHT: f32 = 32.0;
const RADIUS: f32 = 8.0;
const SPACING: f32 = 8.0;
/// A wash over the dock's panel: a step off it in every theme.
const WASH: f32 = 0.05;
const WASH_ON: f32 = 0.22;
/// The wash of a button with nothing behind it to use.
const WASH_UNAVAILABLE: f32 = 0.025;

/// Anything for the row to hold.
pub fn wanted(dock: &Dock) -> bool {
    dock.can_film || dock.can_share || dock.sharing || dock.can_soundboard
}

fn rest(palette: &Palette) -> Color {
    palette.overlay.scale_alpha(WASH)
}

fn lit(palette: &Palette) -> Color {
    palette.live.scale_alpha(WASH_ON)
}

fn unavailable(palette: &Palette) -> Color {
    palette.overlay.scale_alpha(WASH_UNAVAILABLE)
}

/// How a wide button is lit.
#[derive(Clone, Copy, PartialEq)]
enum Lit {
    Rest,
    /// What it starts is on.
    On,
    /// The camera on its way: lit, its picture in the accent.
    Starting,
    /// Nothing behind it to use: no camera.
    Unavailable,
}

/// What pressing a wide button does: a plain intent, or a menu opened at it.
enum Press {
    Intent(Intent),
    Menu(fn(Spot) -> Intent),
}

/// A wide button, lit as `look` says; it presses while `enabled`.
fn button<'a>(
    glyph: Icon,
    label: &'a str,
    (look, enabled): (Lit, bool),
    press: Press,
    motion: Motion,
) -> Element<'a, Intent> {
    let face = icon_button(glyph, label, motion)
        .radius(RADIUS)
        .enabled(enabled);
    let mut face = match look {
        Lit::Rest => face.fill(rest),
        Lit::On => face.fill(lit).tint(|palette| palette.text),
        Lit::Starting => face.fill(lit).tint(|palette| palette.accent),
        Lit::Unavailable => face.fill(unavailable).unavailable(true),
    };
    let menu = match press {
        Press::Intent(intent) => {
            face = face.on_press(intent);
            None
        }
        Press::Menu(open) => Some(open),
    };
    let view = face
        .face()
        .width(Fill)
        .height(HEIGHT)
        .tipped(label, Side::Above);
    match menu {
        // A disabled button opens nothing.
        Some(open) if enabled => opener(view)
            .on_press(move |bounds| open(Spot::above(bounds)))
            .boxed(),
        _ => view.boxed(),
    }
}

pub fn view<'a>(dock: &'a Dock, motion: Motion) -> Element<'a, Intent> {
    let mut buttons = row![].spacing(SPACING);
    if dock.can_soundboard {
        buttons = buttons.push(button(
            Icon::Soundboard,
            tr!("Open Soundboard"),
            (Lit::Rest, dock.ready),
            Press::Menu(Intent::OpenSoundboard),
            motion,
        ));
    }
    if dock.can_film {
        let (label, look) = if dock.camera_starting {
            (tr!("Starting camera..."), Lit::Starting)
        } else if dock.camera_on {
            (tr!("Turn off camera"), Lit::On)
        } else if dock.camera_unavailable {
            (tr!("Camera unavailable"), Lit::Unavailable)
        } else {
            (tr!("Turn on camera"), Lit::Rest)
        };
        let glyph = if dock.camera_on {
            Icon::Video
        } else {
            Icon::VideoOff
        };
        buttons = buttons.push(button(
            glyph,
            label,
            (look, dock.ready),
            Press::Intent(Intent::ToggleCamera),
            motion,
        ));
    }
    // While sharing it opens the stream's menu once the stream is live; the
    // gear and the stop beside "Your screen" are there from the start,
    // while the system's dialog first picks what to share.
    if dock.can_share || dock.sharing {
        let label = if dock.sharing {
            tr!("Stream settings")
        } else {
            tr!("Share your screen")
        };
        buttons = buttons.push(button(
            Icon::ScreenShare,
            label,
            (
                if dock.sharing { Lit::On } else { Lit::Rest },
                dock.ready && (!dock.sharing || dock.share_live),
            ),
            Press::Menu(Intent::OpenShareMenu),
            motion,
        ));
    }
    iced::widget::container(buttons)
        .padding(Padding::ZERO.left(super::INSET))
        .width(Fill)
        .boxed()
}
