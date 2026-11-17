//! A card's row: its picture, two or three lines, and at its end a stack of
//! faces. A voice row opens its room on a click. The Slint side:
//! `ActiveRow` and `RowIcon` (`ui/active_now.slint:108`, `:88`).

use iced::border::Radius;
use iced::widget::text::{Ellipsis, LineHeight, Wrapping};
use iced::widget::{button, container, row, text};
use iced::{Alignment, Fill, Length, Pixels};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::people::active_now::{Intent, Kind, Row};
use anomi_widgets::avatar::{Shape, portrait};

use super::lit::lit;
use super::{faces, time};

/// The row's least height, and its padding above and below (`:122`).
const LEAST: f32 = 52.0;
const PAD: f32 = 8.0;
/// The picture's side, and a game's corner (`:98`).
const ICON: f32 = 32.0;
const ICON_RADIUS: f32 = 8.0;
/// The lines' sizes and heights: the title 14 px (19 high, as Slint rounds up), the others 13.
const TITLE: f32 = 14.0;
const TITLE_LINE: f32 = 19.0;
const LINE: f32 = 17.0;

/// One line of a row, cut with an ellipsis rather than wrapped.
pub fn line<'a>(
    content: impl text::IntoFragment<'a>,
    size: f32,
    height: f32,
) -> iced::widget::Text<'a, Theme> {
    text(content)
        .size(size)
        .line_height(LineHeight::Absolute(Pixels(height)))
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End)
}

/// The row of `item`, `last` when it ends its card (so its fill keeps the
/// card's rounded foot), with `now` the clock its time counts from.
pub fn view<'a>(item: &'a Row, now: i64, last: bool, motion: Motion) -> Element<'a, Intent> {
    let timed = item.kind == Kind::Detail && item.start > 0;
    let lines = 1 + usize::from(!item.detail.is_empty()) + usize::from(timed);
    let height = LEAST.max(2.0 * PAD + TITLE_LINE + (lines - 1) as f32 * LINE);
    let shape = match item.kind {
        Kind::Voice => Shape::Circle,
        _ => Shape::Rounded(ICON_RADIUS),
    };
    let mut texts = iced::widget::column![
        line(item.title.as_str(), TITLE, TITLE_LINE)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .boxed()
    ];
    if !item.detail.is_empty() {
        texts = texts.push(
            line(item.detail.as_str(), typography::size::LABEL, LINE)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    if timed {
        texts = texts.push(
            line(
                time::elapsed(now - item.start),
                typography::size::LABEL,
                LINE,
            )
            .style(catalog::text::muted)
            .boxed(),
        );
    }
    let mut content = row![
        // Slint's `RowIcon` is a still: no motion for a GIF's art.
        portrait(&item.icon, ICON).shape(shape).view(),
        texts.width(Fill)
    ]
    .spacing(metrics::SPACE_M)
    .align_y(Alignment::Center);
    if !item.faces.is_empty() || item.more > 0 {
        content = content.push(faces::view(&item.faces, item.more).boxed());
    }
    let content = container(content)
        .padding([0.0, metrics::SPACE_L])
        .width(Fill)
        .height(Length::Fixed(height))
        .center_y(height);
    if item.room.is_empty() {
        return content.boxed();
    }
    let foot = if last { metrics::RADIUS_CARD } else { 0.0 };
    let corners = Radius {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: foot,
        bottom_left: foot,
    };
    button(lit(content, corners, motion))
        .padding(0)
        .style(catalog::button::bare)
        .on_press_with(move || Intent::OpenRoom(item.room.clone()))
        .boxed()
}
