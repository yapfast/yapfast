//! The device menus and the input level, task V09. The Slint side is
//! `QuickDeviceMenu` (`ui/controls.slint:1610`) and `InputLevel`
//! (`ui/voice.slint:1331`), placed by the window's two popups
//! (`ui/yapfast.slint:2073`, `:2101`).
//!
//! One card for both, which the shell opens for `Layer::Devices` above the
//! caret of the microphone's or the headphones' toggle. From the top: the
//! device in use, which opens the list of them in place; the device's volume;
//! for the microphone its live level and, in a Discord call, noise
//! suppression; a way into Voice settings. The level is the app's to feed
//! while the menu shows; the view asks for no frame for it.

mod choices;
mod list;
mod meter;
mod reveal;

use iced::widget::{column, container, space, text};
use iced::{Background, Border, Color, Length, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::voice::devices::{Devices, Intent};
use anomi_widgets::{menu, slider};
use yapfast_lang::tr;

pub use choices::name as noise_name;

/// The menu's width, and the input menu's when it has noise suppression to
/// choose: Slint fits it to the options (`noise-measure.preferred-width +
/// 108px`, at least 280), which the Russian ones fill at 304.
pub const WIDTH: f32 = 280.0;
pub const WIDTH_WITH_NOISE: f32 = 304.0;
const RADIUS: f32 = 10.0;
/// The card's padding and the gap between its parts (`padding: 6px;
/// spacing: 4px`).
const PADDING: f32 = 6.0;
const SPACING: f32 = 4.0;
/// The padding of the volume, the level and the choice (`padding-left: 8px;
/// padding-right: 8px; padding-top: 4px; padding-bottom: 4px`).
const INSET: Padding = Padding {
    top: 4.0,
    right: 8.0,
    bottom: 4.0,
    left: 8.0,
};
/// The card's shadow (`drop-shadow-blur: 24px; offset-y: 8px; #00000080`).
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);

pub fn view(devices: &Devices, motion: Motion) -> impl View<Intent> + '_ {
    let wide = devices.input() && devices.noise.is_some();
    let mut card = column![
        menu::item(&devices.head, motion)
            .arrow(true)
            .expanded(devices.listing)
            .boxed(),
        list::list(devices, motion).boxed(),
        line().boxed(),
        volume(devices).boxed(),
    ]
    .spacing(SPACING);
    if devices.input() {
        card = card.push(line().boxed()).push(level(devices));
        if let Some(noise) = devices.noise {
            card = card.push(line().boxed()).push(noise_suppression(noise));
        }
    }
    let card = card
        .push(line().boxed())
        .push(menu::item(&devices.settings, motion).boxed());
    container(card)
        .padding(PADDING)
        .width(if wide { WIDTH_WITH_NOISE } else { WIDTH })
        .style(face)
}

/// The raised card, its edge and its shadow.
fn face(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.raised)),
        border: Border::default()
            .rounded(RADIUS)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: SHADOW,
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        ..container::Style::default()
    }
}

/// A divider, 1 px in the line colour.
fn line<'a>() -> impl View<Intent> + 'a {
    container(space())
        .width(Length::Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        })
}

/// "Input Volume" or "Output Volume" over its slider, 0 to 200% with 100% marked, as Slint's default slider.
fn volume(devices: &Devices) -> Element<'_, Intent> {
    let label = if devices.input() {
        tr!("Input Volume")
    } else {
        tr!("Output Volume")
    };
    container(slider::volume(
        slider::Volume::new(label, devices.volume),
        Intent::Volume,
        Intent::Commit,
    ))
    .padding(INSET)
    .boxed()
}

/// A small heading over a part of the menu: muted, bold, 12 px.
fn heading<'a>(words: &'static str) -> impl View<Intent> + 'a {
    text(words)
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .style(catalog::text::muted)
}

/// "INPUT LEVEL" over the segments, then why there are none to light or that
/// muted, nothing is sent.
fn level(devices: &Devices) -> Element<'_, Intent> {
    let mut part = column![
        heading(tr!("INPUT LEVEL")),
        meter::meter(devices.shown_level(), devices.muted),
    ]
    .spacing(2.0 * SPACING);
    if let Some(note) = devices.note() {
        part = part.push(note_text(note));
    } else if devices.muted {
        part = part.push(note_text(tr!("Muted. Nothing you say is sent.")));
    }
    container(part).padding(INSET).boxed()
}

/// A line of small print under a part (Slint's `Label`: muted, 13 px).
fn note_text<'a>(words: &'a str) -> Element<'a, Intent> {
    text(words)
        .size(typography::size::LABEL)
        .style(catalog::text::muted)
        .boxed()
}

/// "NOISE SUPPRESSION" over its options.
fn noise_suppression<'a>(noise: anomi_vm::voice::devices::Noise) -> Element<'a, Intent> {
    container(column![heading(tr!("NOISE SUPPRESSION")), choices::choices(noise)].spacing(6))
        .padding(Padding { top: 0.0, ..INSET })
        .boxed()
}

/// The menus as the golden shots have them, and their other states.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::voice::devices as samples;

    let shot = |area, name, vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("voice", "mic_device_menu", || {
            samples::window(samples::microphone())
        }),
        shot("voice", "headphones_device_menu", || {
            samples::window(samples::speakers())
        }),
        shot("voice", "mic_device_menu_talking", || {
            samples::window(samples::talking())
        }),
        shot("voice", "mic_device_menu_muted", || {
            samples::window(samples::muted())
        }),
        shot("voice", "mic_device_menu_unreadable", || {
            samples::window(samples::unreadable())
        }),
        shot("voice", "mic_device_menu_without_noise", || {
            samples::window(samples::without_noise())
        }),
        shot("voice", "mic_device_menu_listing", || {
            samples::window(samples::listing())
        }),
        shot("voice", "mic_device_menu_nothing_found", || {
            samples::window(samples::nothing_found())
        }),
    ]
}
