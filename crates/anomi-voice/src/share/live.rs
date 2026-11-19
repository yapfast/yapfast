//! The stream's own menu, which the share button, the gear on "Your
//! screen" in the dock and the gear on this account's stream's tile open
//! while live, as Discord's client has it: "Stream Quality" and "Sound",
//! whose submenus show beside the card ([`menu::beside`]) with radio marks,
//! then under a line "Change Windows" and "Stop Streaming" in the danger
//! colour. A choice in a submenu changes the stream at once and the menu
//! stays open; the keys move through the rows and into the submenus
//! ([`menu::keyed`]), as the device menus' do.

use iced::widget::{column, container, space};
use iced::{Background, Border, Color, Length, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_vm::voice::share::{Intent, Live, Part};
use anomi_widgets::menu;

/// The card's width: the menus' narrowest.
pub const WIDTH: f32 = menu::NARROWEST;
/// The card's corners, padding and the gap between rows, as the device
/// menus' and the submenus' beside them (`MenuRows`, `ui/controls.slint:1287`).
const RADIUS: f32 = 8.0;
const PADDING: f32 = 6.0;
const SPACING: f32 = 2.0;
/// Above and below the line before "Change Windows".
const AROUND_LINE: f32 = 4.0;
/// The card's shadow (`drop-shadow-blur: 24px; offset-y: 8px; #00000080`).
const SHADOW_DROP: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);

pub fn view(live: &Live, motion: Motion) -> Element<'_, Intent> {
    let card = column![
        head(live, Part::Quality, motion),
        head(live, Part::Sound, motion),
        line(),
        choice(live, Part::Change, motion),
        choice(live, Part::Stop, motion),
    ]
    .spacing(SPACING);
    let card = container(card).padding(PADDING).width(WIDTH).style(face);
    menu::keyed(card, Intent::Key)
        .escape(live.open.is_some())
        .boxed()
}

/// A row whose submenu shows beside the card while it's open.
fn head(live: &Live, part: Part, motion: Motion) -> Element<'_, Intent> {
    let row = live.row(part);
    let open = live.open == Some(part);
    let mut item = menu::item(row, motion)
        .arrow(true)
        .on_hover(Intent::Hover(Some(part)));
    if live.lit(part) {
        item = item.lit(true);
    }
    menu::beside(item, &row.submenu, open, motion)
        .reach(PADDING)
        .lit(live.sub.filter(|_| open))
        .boxed()
}

/// A row without a submenu: the pointer on it closes the one open.
fn choice(live: &Live, part: Part, motion: Motion) -> Element<'_, Intent> {
    let mut item = menu::item(live.row(part), motion).on_hover(Intent::Hover(Some(part)));
    if live.lit(part) {
        item = item.lit(true);
    }
    item.boxed()
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

/// The line between what changes the stream and what ends it.
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
