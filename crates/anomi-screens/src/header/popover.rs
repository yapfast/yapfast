//! A popover's frame, under the header button that opened it: a heading
//! with its icon and a close button, then what it holds. Task N07's
//! (docs/iced/tasks/shell.md). The Slint side: `Popover`
//! (`ui/header.slint:243`).

use iced::widget::{column, container, row, rule, text};
use iced::{Alignment, Background, Border, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_widgets::button::{Side, icon_button};
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

/// The corners, and the heading's height without and with a detail line
/// (`:251`, `:261`).
const RADIUS: f32 = 10.0;
const HEADING: f32 = 48.0;
const HEADING_DETAIL: f32 = 64.0;

/// `body` in a card headed by `heading` (and `detail` under it), whose
/// close button sends `on_close`.
pub(super) fn popover<'a, M: Clone + 'a>(
    glyph: Icon,
    heading: &'a str,
    detail: Option<&'a str>,
    on_close: M,
    body: impl View<M> + 'a,
    motion: Motion,
) -> Element<'a, M> {
    let words = column![
        text(heading)
            .size(typography::size::TITLE)
            .font(typography::BOLD)
            .style(catalog::text::primary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .extend(detail.map(|detail| {
        text(detail)
            .size(13.0)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed()
    }))
    .spacing(2.0)
    .width(Fill);
    let close = icon_button(Icon::X, tr!("Close"), motion)
        .size(28.0)
        .glyph(18.0)
        .tip_side(Side::Below)
        .on_press(on_close)
        .view();
    let top = container(
        row![
            icon(glyph, 20.0).style(|theme: &Theme, _status| iced::widget::svg::Style {
                color: Some(theme.palette().secondary),
            }),
            words,
            close,
        ]
        .spacing(10.0)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([0.0, 10.0]).left(metrics::SPACE_L))
    .height(if detail.is_some() {
        HEADING_DETAIL
    } else {
        HEADING
    })
    .center_y(if detail.is_some() {
        HEADING_DETAIL
    } else {
        HEADING
    })
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().panel)),
        ..container::Style::default()
    });
    container(column![
        top,
        rule::horizontal(1).style(catalog::rule::line),
        body
    ])
    .clip(true)
    .style(|theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: Some(Background::Color(palette.sidebar)),
            border: Border::default()
                .rounded(RADIUS)
                .width(1)
                .color(palette.edge),
            shadow: Shadow {
                color: iced::Color::from_rgba8(0, 0, 0, 0.4),
                offset: Vector::new(0.0, 8.0),
                blur_radius: 24.0,
            },
            ..container::Style::default()
        }
    })
    .boxed()
}
