//! The device menus at the account card's carets, as Discord's: task V09.
//! The Slint side is `QuickDeviceMenu` (`ui/controls.slint:1610`), placed by
//! the window's two popups (`ui/yapfast.slint:2073`, `:2101`).
//!
//! One card for both, which the shell opens for `Layer::Devices` above the
//! caret of the microphone's or the headphones' toggle. From the top: the
//! device in use under its heading, whose submenu lists the devices beside
//! the card ([`menu::beside`]); for the microphone with Discord linked, the
//! input profile the same way; the device's volume, its value over the
//! handle while dragged; a way into Voice settings. The keys move through
//! the rows and into the submenus ([`menu::keyed`]).

use iced::widget::{column, container, mouse_area, space};
use iced::{Background, Border, Color, Length, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_vm::menu::Row;
use anomi_vm::voice::devices::{Devices, Intent, Part};
use anomi_widgets::{menu, slider};
use yapfast_lang::tr;

/// The card's width: the menus' narrowest.
pub const WIDTH: f32 = menu::NARROWEST;
/// The card's corners, padding and the gap between rows, as the menus' and
/// the submenu's beside it (`MenuRows`, `ui/controls.slint:1287`).
const RADIUS: f32 = 8.0;
const PADDING: f32 = 6.0;
const SPACING: f32 = 2.0;
/// Above and below a group's line, which takes 9 px with it.
const AROUND_LINE: f32 = 4.0;
/// The volume's room inside the card: a row's sides, 4 px above and 6 px
/// below.
const INSET: Padding = Padding {
    top: 4.0,
    right: 8.0,
    bottom: 6.0,
    left: 8.0,
};
/// The card's shadow (`drop-shadow-blur: 24px; offset-y: 8px; #00000080`).
const SHADOW_DROP: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);

pub fn view(devices: &Devices, motion: Motion) -> impl View<Intent> + '_ {
    let mut card = column![head(devices, Part::Device, &devices.device, motion)].spacing(SPACING);
    if let Some(profile) = &devices.profile {
        card = card.push(head(devices, Part::Profile, profile, motion));
    }
    let mut settings =
        menu::item(&devices.settings, motion).on_hover(Intent::Hover(Some(Part::Settings)));
    if devices.lit(Part::Settings) {
        settings = settings.lit(true);
    }
    let card = card
        .push(line())
        .push(volume(devices))
        .push(line())
        .push(settings.boxed());
    let card = container(card).padding(PADDING).width(WIDTH).style(face);
    menu::keyed(card, Intent::Key).escape(devices.open.is_some())
}

/// A row whose submenu shows beside the card while it's open: the
/// device's, the profile's.
fn head<'a>(
    devices: &'a Devices,
    part: Part,
    row: &'a Row<Intent>,
    motion: Motion,
) -> Element<'a, Intent> {
    let open = devices.open == Some(part);
    let mut item = menu::item(row, motion)
        .arrow(true)
        .on_hover(Intent::Hover(Some(part)));
    if devices.lit(part) {
        item = item.lit(true);
    }
    menu::beside(item, &row.submenu, open, motion)
        .reach(PADDING)
        .lit(devices.sub.filter(|_| open))
        .boxed()
}

/// The card in the menus' colour, its edge and its shadow.
fn face(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.menu())),
        border: Border::default()
            .rounded(RADIUS)
            .width(1)
            .color(palette.menu_line()),
        shadow: Shadow {
            color: SHADOW,
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// A group's line, 1 px in the line colour with room above and below.
fn line<'a>() -> Element<'a, Intent> {
    let rule = container(space())
        .width(Length::Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        });
    container(rule)
        .padding(Padding::new(0.0).top(AROUND_LINE).bottom(AROUND_LINE))
        .boxed()
}

/// "Input Volume" or "Output Volume" over its slider, 0 to 200% with 100%
/// marked; the pointer on it closes a submenu.
fn volume(devices: &Devices) -> Element<'_, Intent> {
    let label = if devices.input() {
        tr!("Input Volume")
    } else {
        tr!("Output Volume")
    };
    let slider = slider::volume(
        slider::Volume::new(label, devices.volume).tip(),
        Intent::Volume,
        Intent::Commit,
    );
    mouse_area(container(slider).padding(INSET))
        .on_enter(Intent::Hover(None))
        .boxed()
}

/// The menus as the golden shots have them, and with a submenu open.
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
        shot("voice", "headphones_device_menu_listing", || {
            samples::window(samples::listing())
        }),
        shot("voice", "mic_device_menu_profiles", || {
            samples::window(samples::profiles())
        }),
        shot("voice", "mic_device_menu_nothing_found", || {
            samples::window(samples::nothing_found())
        }),
    ]
}
