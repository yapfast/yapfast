//! A screen to share, as a picture with its name under it: `ScreenCard`
//! (`ui/voice.slint:1363`). Lit under the pointer, ringed when picked.

use iced::widget::{button, column, container, svg, text};
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

/// One card, `width` wide, sending `Intent::Source(index)`.
pub fn card(screen: &Screen, index: usize, checked: bool, width: f32) -> impl View<Intent> + '_ {
    let inner = width - 2.0 * INSET;
    let ring = if checked { 2.0 } else { 1.0 };
    let picture_height = inner * 9.0 / 16.0;
    let shown: Element<'_, Intent> = match &screen.picture {
        Some(handle) => picture(handle.clone())
            .width(Length::Fixed(inner - 2.0 * ring))
            .height(Length::Fixed(picture_height - 2.0 * ring))
            .fit(Fit::Contain)
            .radius(PICTURE_RADIUS - ring)
            .boxed(),
        None => icon(Icon::ScreenShare, PLACEHOLDER)
            .style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().faint),
            })
            .boxed(),
    };
    // The ring is the well's own border: a quad draws under pictures, so the
    // picture sits inside it rather than the ring over the picture.
    let well = container(shown)
        .width(inner)
        .height(picture_height)
        .padding(ring)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(palette.sunken)),
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
    let name = text(screen.title.as_str())
        .size(NAME_SIZE)
        .font(if checked {
            typography::BOLD
        } else {
            typography::UI
        })
        .wrapping(text::Wrapping::None)
        .align_x(Alignment::Center)
        .width(Fill);
    button(column![well, name].spacing(INSET).width(Fill))
        .padding(INSET)
        .width(width)
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
