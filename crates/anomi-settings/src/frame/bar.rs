//! The frame's bar over the page: its title over the page's column and the
//! close button in the card's corner, a line under it. The Slint side: the
//! `discord-layout` bar of `SettingsFrame` (`ui/settings.slint:497`).

use iced::widget::{container, row, rule, stack, text};
use iced::{Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_widgets::button::icon_button;

/// The bar's height; the page's column, centred, at most this wide (1040 for
/// a table) and at least this far from the sides (`column-max`, `column-x:
/// 32`).
pub const HEIGHT: f32 = 48.0;
pub const COLUMN: f32 = 696.0;
pub const WIDE: f32 = 1040.0;
pub const SIDES: f32 = 32.0;
/// The title and the quieter detail after it stand this far apart, the
/// detail's size (`ui/settings.slint:511`).
const DETAIL_GAP: f32 = 8.0;
const DETAIL: f32 = typography::size::SMALL;
/// The close button's side and glyph, and its inset from the corner.
const CLOSE: f32 = 32.0;
const GLYPH: f32 = 18.0;
const CORNER: f32 = 8.0;

/// The bar over a page titled `title`, with the quieter `detail` after it
/// (a server's boost level) unless empty; the close button's tooltip says
/// `close_tip` and a press on it sends `on_close`. `column` is the page's
/// widest column.
pub fn bar<'a, M: Clone + 'a>(
    title: &'a str,
    detail: &'a str,
    close_tip: &'a str,
    on_close: M,
    column_max: f32,
    motion: Motion,
) -> Element<'a, M> {
    let words = text(title)
        .size(typography::size::BODY)
        .font(typography::BOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    let mut line: Vec<Element<'a, M>> = vec![words.boxed()];
    if !detail.is_empty() {
        line.push(
            text(detail)
                .size(DETAIL)
                .font(typography::SEMIBOLD)
                .wrapping(text::Wrapping::None)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    let line = row(line)
        .spacing(DETAIL_GAP)
        .align_y(alignment::Vertical::Center);
    let beside = container(container(line).width(Length::Fill.max(column_max)))
        .padding(Padding::from([0.0, SIDES]))
        .center_x(Length::Fill)
        .align_y(alignment::Vertical::Center)
        .height(Length::Fill);
    let close = container(
        icon_button(Icon::X, close_tip, motion)
            .size(CLOSE)
            .glyph(GLYPH)
            .on_press(on_close)
            .view(),
    )
    .align_right(Length::Fill)
    .align_y(alignment::Vertical::Center)
    .padding(Padding::ZERO.right(CORNER))
    .height(Length::Fill);
    let rule = container(rule::horizontal(1).style(catalog::rule::line))
        .height(Length::Fill)
        .align_y(alignment::Vertical::Bottom);
    container(stack![beside, close, rule])
        .height(HEIGHT)
        .style(catalog::container::panel)
        .boxed()
}
