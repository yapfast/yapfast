//! A popover's frame, under the header button that opened it: a heading
//! with its icon and a close button, then what it holds. Task N07's
//! (docs/iced/tasks/shell.md). The Slint side: `Popover`
//! (`ui/header.slint:243`).

use iced::widget::{column, container, row, rule, text};
use iced::{Alignment, Background, Border, Fill, Padding, Shadow, Vector, border};

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
/// The shadow under the card: `drop-shadow-color: #00000066`, blur 24 px,
/// 8 px down (`ui/header.slint:250`).
const SHADOW_ALPHA: f32 = 0x66 as f32 / 255.0;
const SHADOW_BLUR: f32 = 24.0;
const SHADOW_DROP: f32 = 8.0;
/// The card's border (`border-width: 1px`), which its content sits inside.
const BORDER: f32 = 1.0;
/// The heading's insides (`ui/header.slint:258`-`:265`): its padding, the
/// space between its parts and under its title, the icon, the detail line's
/// type and the close button with its glyph.
const PAD_LEFT: f32 = 16.0;
const PAD_RIGHT: f32 = 10.0;
const SPACING: f32 = 10.0;
const SPACING_TITLE: f32 = 2.0;
const ICON: f32 = 20.0;
const DETAIL_SIZE: f32 = 13.0;
const CLOSE: f32 = 28.0;
const CLOSE_GLYPH: f32 = 18.0;
/// A body's padding of 16 px, less the border that here takes room from the
/// card's content, where Slint paints it over the children.
pub(super) const INSET: f32 = metrics::SPACE_L - BORDER;

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
            .size(DETAIL_SIZE)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed()
    }))
    .spacing(SPACING_TITLE)
    .width(Fill);
    let close = icon_button(Icon::X, tr!("Close"), motion)
        .size(CLOSE)
        .glyph(CLOSE_GLYPH)
        .tip_side(Side::Below)
        .on_press(on_close)
        .view();
    let top = container(
        row![
            icon(glyph, ICON).style(|theme: &Theme, _status| iced::widget::svg::Style {
                color: Some(theme.palette().secondary),
            }),
            words,
            close,
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center),
    )
    // The card's border is painted over the heading in Slint, and here the
    // card's padding keeps the content inside it: the heading is a border
    // narrower on each side, above and under, so that its line is the last
    // row of its 48 (or 64) px.
    .padding(Padding::from([0.0, PAD_RIGHT - BORDER]).left(PAD_LEFT - BORDER))
    .center_y(
        if detail.is_some() {
            HEADING_DETAIL
        } else {
            HEADING
        } - 2.0 * BORDER,
    )
    .style(|theme: &Theme| container::Style {
        background: Some(Background::Color(theme.palette().panel)),
        // Inside the card's border, whose corners are a pixel tighter.
        border: Border::default().rounded(border::top(RADIUS - BORDER)),
        ..container::Style::default()
    });
    container(column![
        top,
        rule::horizontal(1).style(|theme: &Theme| iced::widget::rule::Style {
            color: theme.palette().edge,
            ..catalog::rule::line(theme)
        }),
        body
    ])
    .padding(BORDER)
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
                color: iced::Color::from_rgba8(0, 0, 0, SHADOW_ALPHA),
                offset: Vector::new(0.0, SHADOW_DROP),
                blur_radius: SHADOW_BLUR,
            },
            ..container::Style::default()
        }
    })
    .boxed()
}
