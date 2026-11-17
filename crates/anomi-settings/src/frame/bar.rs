//! The frame's bar over the page: its title over the page's column and the
//! close button in the card's corner, a line under it. The Slint side: the
//! `discord-layout` bar of `SettingsFrame` (`ui/settings.slint:497`).

use iced::widget::{column, container, rule, stack, text};
use iced::{Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::frame::Intent;
use anomi_widgets::button::icon_button;
use yapfast_lang::tr;

/// The bar's height; the page's column, centred, at most this wide and at
/// least this far from the sides (`column-max`, `column-x: 32`).
pub const HEIGHT: f32 = 48.0;
pub const COLUMN: f32 = 696.0;
pub const SIDES: f32 = 32.0;
/// The close button's side and glyph, and its inset from the corner.
const CLOSE: f32 = 32.0;
const GLYPH: f32 = 18.0;
const CORNER: f32 = 8.0;

/// The bar over a page titled `title`.
pub fn bar<'a>(title: &'a str, motion: Motion) -> Element<'a, Intent> {
    let words = text(title)
        .size(typography::size::BODY)
        .font(typography::BOLD)
        .wrapping(text::Wrapping::None);
    let beside = container(container(words).width(Length::Fill.max(COLUMN)))
        .padding(Padding::from([0.0, SIDES]))
        .center_x(Length::Fill)
        .align_y(alignment::Vertical::Center)
        .height(Length::Fill);
    let close = container(
        icon_button(Icon::X, tr!("Close"), motion)
            .size(CLOSE)
            .glyph(GLYPH)
            .on_press(Intent::Close)
            .view(),
    )
    .align_right(Length::Fill)
    .align_y(alignment::Vertical::Center)
    .padding(Padding::ZERO.right(CORNER))
    .height(Length::Fill);
    column![
        container(stack![beside, close])
            .height(HEIGHT - 1.0)
            .style(catalog::container::panel),
        rule::horizontal(1).style(catalog::rule::line)
    ]
    .boxed()
}
