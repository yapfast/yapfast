//! The call controls: the pill that floats at the stage's foot. The Slint
//! side is `CallControls` (`ui/voice.slint:1121`), placed under the stage at
//! `ui/yapfast.slint:3537`.
//!
//! One dock with a raised ground and a line around it: this account's
//! sound (mute, deafen), a divider, what the call offers (the soundboard,
//! the camera, sharing), a divider, and leaving. Each button is 44 px with
//! 12 px corners (the dock's 18 less its 6 px padding for the corners to
//! follow); an alert (muted, deafened) is red on a wash of red, a live one
//! (camera, sharing) in the live colour on a wash of it. The soundboard and
//! the share menu open at their buttons, the way the dock's do; sharing
//! stops with a click on the button that says "Stop sharing".
//!
//! Everything but leaving waits for the connection (`ready`); leaving is
//! faded while it goes. It never ticks.
//!
//! Known gap, from the pinned iced and the widgets: the hang-up is iced's
//! `button` in the `danger` style (danger, `danger_hover` under the pointer,
//! fill and ink halved while leaving, `:1195-1204`), not the face, so its
//! hover is not eased over QUICK. The camera button doesn't pulse
//! while it starts (no layer opacity): like Slint's at rest it follows
//! `camera-on` alone (`ui/voice.slint:1176`), and its tooltip says "Starting
//! camera...". The task's "menu arrow" on each button is not in Slint or in
//! the golden shots (the arrows there are the sidebar's device toggles, N06),
//! so the pill has none.

#[cfg(feature = "shots")]
mod shots;

use iced::widget::{button as iced_button, center, column, container, row, space};
use iced::{Alignment, Border, Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, catalog};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Spot;
use anomi_vm::voice::controls::{Controls, Intent};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon;
use anomi_widgets::menu::opener;
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::{text_in, tr};

#[cfg(feature = "shots")]
pub use shots::shots;

/// A button's side, its picture and its corners (`:1146`).
pub const BUTTON: f32 = 44.0;
const GLYPH: f32 = 20.0;
const RADIUS: f32 = 12.0;
/// The hang-up's width (`:1197`).
pub const LEAVE: f32 = 60.0;
/// The hang-up's opacity while leaving (`:1199`).
const LEAVING_ALPHA: f32 = 0.5;
/// The dock's corners, padding and the space between buttons (`:1141`).
const DOCK_RADIUS: f32 = 18.0;
pub const PAD: f32 = 6.0;
pub const SPACING: f32 = 6.0;
/// The dividers: a hairline, 22 px tall in the button's row (`:1166`).
pub const DIVIDER: f32 = 1.0;
const DIVIDER_HEIGHT: f32 = 22.0;
/// The room round the dock: 16 px, 8 on top (`:1139`).
const OUTER: f32 = 16.0;
const OUTER_TOP: f32 = 8.0;
/// The wash behind an alert or a live button.
const ALERT_WASH: f32 = 0.18;
const LIVE_WASH: f32 = 0.2;

fn alert_wash(palette: &Palette) -> Color {
    palette.danger.scale_alpha(ALERT_WASH)
}

fn live_wash(palette: &Palette) -> Color {
    palette.live.scale_alpha(LIVE_WASH)
}

/// What a button looks like: its picture, its name for the tooltip and how
/// it is lit.
#[derive(Clone, Copy, PartialEq)]
enum Lit {
    Off,
    /// Red: a state worth warning about.
    Alert,
    /// The live colour.
    Live,
}

/// A 44 px button of the pill.
fn button<'a>(
    glyph: Icon,
    label: &'a str,
    lit: Lit,
    enabled: bool,
    motion: Motion,
) -> anomi_widgets::button::IconButton<'a, Intent> {
    let face = icon_button(glyph, label, motion)
        .size(BUTTON)
        .glyph(GLYPH)
        .radius(RADIUS)
        .enabled(enabled)
        .tint(|palette| palette.text);
    match lit {
        Lit::Off => face,
        Lit::Alert => face.alert(true).fill(alert_wash),
        Lit::Live => face.tint(|palette| palette.live).fill(live_wash),
    }
}

/// A button that opens a menu at itself. It is an `Opener` around the tipped
/// face in every state, so iced keeps the face's state across them; `open`
/// is `None` while nothing may open (not ready, or the face has its own press).
fn opening<'a>(
    face: anomi_widgets::button::IconButton<'a, Intent>,
    label: &'a str,
    open: Option<fn(Spot) -> Intent>,
) -> Element<'a, Intent> {
    let view = opener(face.face().tipped(label, Side::Above));
    match open {
        Some(open) => view
            .on_press(move |bounds| open(Spot::above(bounds)))
            .boxed(),
        None => view.boxed(),
    }
}

/// A hairline between groups.
fn divider<'a>() -> Element<'a, Intent> {
    container(
        container(space())
            .width(DIVIDER)
            .height(DIVIDER_HEIGHT)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().line.into()),
                ..catalog::container::transparent(theme)
            }),
    )
    .center_y(BUTTON)
    .boxed()
}

/// The pill, and the room round it.
pub fn view(controls: &Controls, motion: Motion) -> impl View<Intent> + '_ {
    let ready = controls.ready;
    let mut pill = row![
        button(
            if controls.muted {
                Icon::MicOff
            } else {
                Icon::Mic
            },
            if controls.muted {
                text_in("CallControls", "Unmute")
            } else {
                text_in("CallControls", "Mute")
            },
            if controls.muted { Lit::Alert } else { Lit::Off },
            ready,
            motion,
        )
        .on_press(Intent::ToggleMute)
        .view(),
        button(
            if controls.deafened {
                Icon::HeadphoneOff
            } else {
                Icon::Headphones
            },
            if controls.deafened {
                tr!("Undeafen")
            } else {
                tr!("Deafen")
            },
            if controls.deafened {
                Lit::Alert
            } else {
                Lit::Off
            },
            ready,
            motion,
        )
        .on_press(Intent::ToggleDeafen)
        .view(),
        divider(),
    ]
    .spacing(SPACING);
    if controls.can_soundboard {
        let label = tr!("Open Soundboard");
        let face = button(Icon::Soundboard, label, Lit::Off, ready, motion);
        let open = ready.then_some(Intent::OpenSoundboard as fn(Spot) -> Intent);
        pill = pill.push(opening(face, label, open));
    }
    if controls.can_film {
        let label = if controls.camera_starting {
            tr!("Starting camera...")
        } else if controls.camera_on {
            tr!("Turn off camera")
        } else {
            tr!("Turn on camera")
        };
        let (glyph, lit) = if controls.camera_on {
            (Icon::Video, Lit::Live)
        } else {
            (Icon::VideoOff, Lit::Off)
        };
        pill = pill.push(
            button(glyph, label, lit, ready, motion)
                .on_press(Intent::ToggleCamera)
                .view()
                .boxed(),
        );
    }
    if controls.can_share || controls.sharing {
        let (glyph, label, lit) = if controls.sharing {
            (Icon::ScreenShareOff, tr!("Stop sharing"), Lit::Live)
        } else {
            (Icon::ScreenShare, tr!("Share your screen"), Lit::Off)
        };
        let mut face = button(glyph, label, lit, ready, motion);
        let open = if controls.sharing {
            face = face.on_press(Intent::StopSharing);
            None
        } else {
            ready.then_some(Intent::OpenShareMenu as fn(Spot) -> Intent)
        };
        pill = pill.push(opening(face, label, open));
    }
    pill = pill.push(divider());
    pill = pill.push(leave(controls.leaving, motion));

    let dock = container(pill.align_y(Alignment::Center))
        .padding(PAD)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.raised.into()),
                border: Border {
                    color: palette.line,
                    width: 1.0,
                    radius: DOCK_RADIUS.into(),
                },
                ..catalog::container::transparent(theme)
            }
        });
    column![container(dock).width(Fill).align_x(Alignment::Center)].padding(Padding {
        top: OUTER_TOP,
        right: OUTER,
        bottom: OUTER,
        left: OUTER,
    })
}

/// `ink` at `LEAVING_ALPHA` over `ground`, as one opaque colour: an svg keeps
/// only the rgb of its colour, so alpha cannot fade a glyph.
fn faded(ink: Color, ground: Color) -> Color {
    let blend = |a: f32, b: f32| a * LEAVING_ALPHA + b * (1.0 - LEAVING_ALPHA);
    Color::from_rgb(
        blend(ink.r, ground.r),
        blend(ink.g, ground.g),
        blend(ink.b, ground.b),
    )
}

/// The hang-up: the danger colour, 60 by 44. Disabled while leaving, where
/// the button style halves the fill and the glyph is on_danger at half over
/// the dock (Slint fades the rectangle and its glyph as one layer,
/// `ui/voice.slint:1199`, so the glyph hides the danger beneath it).
fn leave<'a>(leaving: bool, motion: Motion) -> Element<'a, Intent> {
    let label = tr!("Leave voice");
    let glyph =
        icon::icon(Icon::PhoneOff, GLYPH).style(move |theme: &Theme, _| iced::widget::svg::Style {
            color: Some(if leaving {
                // Slint draws the rectangle and its glyph into one layer: the
                // opaque glyph replaces the danger under it, and the layer goes
                // at half over the dock. So the glyph is on_danger at half
                // over `raised` alone.
                let palette = theme.palette();
                faded(palette.on_danger, palette.raised)
            } else {
                theme.palette().on_danger
            }),
        });
    let face = iced_button(center(glyph))
        .width(LEAVE)
        .height(BUTTON)
        .padding(0)
        .style(|theme: &Theme, status| iced::widget::button::Style {
            border: Border::default().rounded(RADIUS),
            ..catalog::button::danger(theme, status)
        })
        .on_press_maybe((!leaving).then_some(Intent::Leave));
    tip(face, label, Side::Above, motion).boxed()
}
