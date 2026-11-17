//! A screen to share, as a picture with its name under it: `ScreenCard`
//! (`ui/voice.slint:1363`). Lit under the pointer, ringed when picked.

use iced::widget::{button, column, container, space, stack, svg, text};
use iced::{Alignment, Background, Border, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::voice::share::{Intent, Screen};
use anomi_widgets::icon::icon;
use anomi_widgets::picture::{Fit, picture};

/// The picture's corners (`:1376`), and the card's own.
const PICTURE_RADIUS: f32 = 4.0;
/// Padding and spacing inside the card (`:1389`).
const INSET: f32 = 4.0;
/// The name under the picture, and the room it takes with its spacing.
const NAME_SIZE: f32 = typography::size::LABEL;
/// Before its picture comes (`:1380`).
const PLACEHOLDER: f32 = 24.0;
/// The ring's width, and when the screen is picked (`:1384`).
const RING: f32 = 1.0;
const RING_PICKED: f32 = 2.0;
/// The card's height: its width at 16:9 and the name's room (`card-height`, `:1475`).
const NAME_ROOM: f32 = 30.0;

/// One card, `width` wide, sending `Intent::Source(index)`.
pub fn card(screen: &Screen, index: usize, checked: bool, width: f32) -> impl View<Intent> + '_ {
    let inner = width - 2.0 * INSET;
    let ring = if checked { RING_PICKED } else { RING };
    // Whole pixels, so the ring lies on pixel rows and stays crisp.
    let picture_height = (inner * 9.0 / 16.0).round();
    let shown: Element<'_, Intent> = match &screen.picture {
        Some(handle) => picture(handle.clone())
            .width(Length::Fixed(inner))
            .height(Length::Fixed(picture_height))
            .fit(Fit::Contain)
            .radius(PICTURE_RADIUS)
            .boxed(),
        None => icon(Icon::ScreenShare, PLACEHOLDER)
            .style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().faint),
            })
            .boxed(),
    };
    let well = container(shown)
        .width(inner)
        .height(picture_height)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .clip(true)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().sunken)),
            border: Border::default().rounded(PICTURE_RADIUS),
            ..container::Style::default()
        });
    // The ring lies over the picture (`:1382`): a later stack child draws in
    // its own layer, above the picture's.
    let over = container(space())
        .width(inner)
        .height(picture_height)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                border: Border::default()
                    .rounded(PICTURE_RADIUS)
                    .width(ring)
                    .color(if checked {
                        palette.accent
                    } else {
                        palette.line
                    }),
                ..container::Style::default()
            }
        });
    let well = stack![well, over];
    let name = text(screen.title.as_str())
        .size(NAME_SIZE)
        .font(if checked {
            typography::BOLD
        } else {
            typography::UI
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .align_x(Alignment::Center)
        .width(Fill);
    let content = container(column![well, name].spacing(INSET).width(Fill))
        .align_y(Alignment::Start)
        .height(Fill);
    button(content)
        .padding(INSET)
        .width(width)
        .height(width * 9.0 / 16.0 + NAME_ROOM)
        .on_press(Intent::Source(index))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: lit.then_some(Background::Color(palette.active)),
                text_color: if checked || lit {
                    palette.text
                } else {
                    palette.secondary
                },
                border: Border::default().rounded(metrics::RADIUS_ROW),
                ..button::Style::default()
            }
        })
}
