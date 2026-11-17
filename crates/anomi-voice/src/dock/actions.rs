//! The wide buttons under the status (`DockButton`, `ui/voice.slint:326`,
//! used at `:546`): the soundboard, the camera and the screen, as Discord's
//! panel has them, lit while what they start is on. Each is 32 px tall with
//! 8 px corners and a wash over the panel; they share the row's width.
//!
//! The wash is `IconButton`'s fill: 5% of the overlay at rest, the live
//! colour's 22% when on.

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

/// What pressing a wide button does: a plain intent, or a menu opened at it.
enum Press {
    Intent(Intent),
    Menu(fn(Spot) -> Intent),
}

/// A wide button: `on` lights it, `starting` tints its picture with the
/// accent (the camera on its way).
fn button<'a>(
    glyph: Icon,
    label: &'a str,
    (on, starting, enabled): (bool, bool, bool),
    press: Press,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut face = icon_button(glyph, label, motion)
        .radius(RADIUS)
        .enabled(enabled)
        .fill(if on { lit } else { rest });
    if starting {
        face = face.tint(|palette| palette.accent);
    } else if on {
        face = face.tint(|palette| palette.text);
    }
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
            (false, false, dock.ready),
            Press::Menu(Intent::OpenSoundboard),
            motion,
        ));
    }
    if dock.can_film {
        let label = if dock.camera_starting {
            tr!("Starting camera...")
        } else if dock.camera_on {
            tr!("Turn off camera")
        } else {
            tr!("Turn on camera")
        };
        let glyph = if dock.camera_on {
            Icon::Video
        } else {
            Icon::VideoOff
        };
        buttons = buttons.push(button(
            glyph,
            label,
            (dock.camera_on, dock.camera_starting, dock.ready),
            Press::Intent(Intent::ToggleCamera),
            motion,
        ));
    }
    // Stopping is beside "Your screen"; while the system's dialog first
    // picks what to share, there's nothing to switch yet.
    if dock.can_share || dock.sharing {
        let label = if dock.sharing {
            tr!("Switch screen or window")
        } else {
            tr!("Share your screen")
        };
        buttons = buttons.push(button(
            Icon::ScreenShare,
            label,
            (
                dock.sharing,
                false,
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
