//! Every style of the fake's `--member-looks` and the plates of its member
//! list, drawn to PNGs for a reviewer to hold beside the reference shots,
//! as widgets have no screen of their own:
//!
//! ```sh
//! ANOMI_GALLERY=/tmp/names cargo test -p anomi-widgets name::tests::gallery -- --ignored
//! ```
//!
//! The simulator snapshots at twice the pixels, so the gallery has faded
//! names drawn at that scale, as the app has them at the window's; alone,
//! as the scale is the process's.

use iced::widget::{column, container, row, space, text};
use iced::{Fill, Size};
use iced_test::Simulator;

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, fonts};

use super::super::{Ink, Style, styled_name};
use super::support;
use crate::nameplate::{Place, extent, nameplate, sample};

/// `nameStyleSample` in `testing/fake_discord.ts`.
fn sample_style(n: usize) -> (String, Option<Style>) {
    let effects = [
        "solid", "gradient", "neon", "toon", "pop", "glow", "prism", "gummy",
    ];
    let fonts = [3, 4, 6, 8, 12, 13, 14, 15, 16, 11];
    let palettes: [&[u32]; 8] = [
        &[0xff7eb3],
        &[0xff7eb3, 0x7a5cff],
        &[0x00ff99],
        &[0xffaa33],
        &[0x3388ff],
        &[0xff5555, 0xffdd55],
        &[0x66ccff, 0xff66cc, 0xffee66],
        &[0x101010, 0x55aa77],
    ];
    let effect = n % effects.len();
    let name = format!("{} name {n}", effects[effect]);
    let style = Style::new(fonts[n % fonts.len()], effect as u8 + 1, palettes[effect]);
    (name, style)
}

/// How a sample member row stands.
#[derive(Clone, Copy, PartialEq)]
enum Stand {
    Rest,
    Selected,
    /// Faded to half, name and plate, as an offline member's.
    Offline,
}

fn member<'a>(name: &'a str, style: Option<Style>, plate: &str, stand: Stand) -> Element<'a, ()> {
    let front = Motion {
        front: true,
        ..Motion::default()
    };
    let opacity = if stand == Stand::Offline { 0.5 } else { 1.0 };
    let label = styled_name(name, style)
        .ink(Ink::Token(|palette| palette.secondary))
        .opacity(opacity);
    let lines = extent(column![label, text("status line").size(14)]);
    let content = row![space::horizontal().width(32), lines]
        .spacing(12)
        .padding([0, 8]);
    nameplate(
        container(content).center_y(42).width(Fill),
        Place::Member,
        front,
    )
    .picture(Some(sample::strip(plate)))
    .palette(sample::palette(plate))
    .selected(stand == Stand::Selected)
    .opacity(opacity)
    .radius(8.0)
    .boxed()
}

#[test]
#[ignore = "writes PNGs for review"]
fn gallery() {
    let Some(dir) = std::env::var_os("ANOMI_GALLERY") else {
        return;
    };
    fonts::install();
    crate::picture::set_scale(2.0);
    let samples: Vec<(String, Option<Style>)> = (0..16).map(sample_style).collect();
    let carol = Style::new(3, 2, &[0xff7eb3, 0x7a5cff]);
    let bob = Style::new(14, 3, &[0x00ff99]);
    for id in ["bonfire", "catppuccin-latte"] {
        let theme = Theme::built_in(Some(id));
        let names = samples
            .iter()
            .map(|(name, style)| styled_name(name, *style).boxed());
        let toon = Style::new(16, 4, &[0xffaa33]);
        let plates = column![
            member("Alice", None, "bubble_gum", Stand::Rest),
            member("Bob", bob, "sky", Stand::Selected),
            member("Carol", carol, "violet", Stand::Rest),
            member("Dave", carol.map(Style::font_only), "teal", Stand::Rest),
            member("Erin", bob, "forest", Stand::Offline),
            member("Frank", toon, "sunset", Stand::Offline),
        ]
        .spacing(2)
        .width(248);
        let view = row![column(names).spacing(6), plates]
            .spacing(24)
            .padding(12);
        let mut ui: Simulator<'_, (), Theme, Renderer> =
            Simulator::with_size(support::settings(), Size::new(560.0, 440.0), view);
        let shot = ui.snapshot(&theme).expect("a snapshot");
        shot.matches_image(std::path::Path::new(&dir).join(id))
            .expect("saved");
    }
    crate::picture::set_scale(1.0);
}
