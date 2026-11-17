//! A card's rows (`CardView`'s `for row in root.card.rows`,
//! `ui/messages.slint:2284`-`:2352`).

use iced::widget::{container, row, space, text};
use iced::{Alignment, Border, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::components::{Components, Intent, Row};
use anomi_widgets::paragraph::paragraph;
use yapfast_lang::tr;

use super::{button, media};

/// Beside a section's text: the space before its thumbnail or button.
const SECTION_GAP: f32 = 12.0;
/// A buttons row's gap.
const BUTTONS_GAP: f32 = 8.0;
/// A large gap is 16 px beyond the layout's spacing either side (the line,
/// when it has one, in its middle).
const LARGE: f32 = 16.0;
const NOTE_PADDING: f32 = 16.0;
const NOTE_RADIUS: f32 = 12.0;

pub fn view<'a>(
    components: &'a Components,
    row_: &'a Row,
    framed: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let key = components.key;
    match row_ {
        Row::Text {
            text,
            thumbnail,
            button,
        } => {
            let paragraph = paragraph(text)
                .size(components.size.px())
                .framed(framed)
                .motion(motion)
                .on_link(move |link| Intent::Link(key, link));
            // What lies beside the text keeps to its top, as Slint's
            // `alignment: start`.
            let mut line = row![container(paragraph).width(Fill)]
                .spacing(SECTION_GAP)
                .align_y(Alignment::Start);
            if let Some(thumbnail) = thumbnail {
                line = line.push(media::thumbnail(key, thumbnail, motion));
            }
            if let Some(button) = button {
                line = line.push(button::card_button(components, button, motion));
            }
            line.boxed()
        }
        // The message's gallery and file cards, C04's.
        Row::Media(media) => crate::attachments::view(media, motion)
            .map(Intent::Media)
            .boxed(),
        Row::Separator { large, divider } => {
            let height = if *large { LARGE } else { 0.0 } + if *divider { 1.0 } else { 0.0 };
            let line = container(space())
                .width(Fill)
                .height(1.0)
                .style(|theme: &Theme| {
                    container::Style::default().background(theme.palette().line)
                });
            let line: Element<'a, Intent> = if *divider {
                line.boxed()
            } else {
                space().boxed()
            };
            container(line)
                .height(height)
                .width(Fill)
                .align_y(Alignment::Center)
                .boxed()
        }
        Row::Buttons(buttons) => {
            let mut line = row![].spacing(BUTTONS_GAP).align_y(Alignment::Center);
            for each in buttons {
                line = line.push(button::card_button(components, each, motion));
            }
            line.boxed()
        }
        Row::Select(select) => button::select_box(components, select),
        // As Discord's client tells of components it doesn't know.
        Row::Unknown => container(
            text(tr!("Yapfast can't show this part of the message yet."))
                .size(typography::size::SMALL)
                .style(catalog::text::secondary),
        )
        .padding(NOTE_PADDING)
        .width(Fill)
        .style(|theme: &Theme| {
            container::Style::default().border(Border {
                color: theme.palette().accent,
                width: 1.0,
                radius: NOTE_RADIUS.into(),
            })
        })
        .boxed(),
    }
}
