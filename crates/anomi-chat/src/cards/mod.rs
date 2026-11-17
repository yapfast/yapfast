//! The card a custom emoji or a sticker opens: the picture, its name and what
//! it is in 88 px, and below it the server it is from, which opens on a click
//! (`EmojiCardView`, `MessageStickerCardView`, `ui/messages.slint:1079`,
//! `:957`). 288 px wide. Task C08 (docs/iced/tasks/chat.md).
//!
//! `Layer::MessageCard` calls [`view`]: it draws the card that is open and
//! nothing when both are closed; the shell gives the backdrop and Escape.

mod origin;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{Column, Row, container, space, text};
use iced::{Alignment, Background, Border, Color, Fill, Length, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::cards::{Cards, Emoji, Intent, Origin, Sticker, WIDTH};
use anomi_vm::kind::Expected;
use anomi_widgets::picture::{Fit, picture};
use origin::Look;
use yapfast_lang::tr;

#[cfg(feature = "shots")]
pub use shots::shots;

/// The picture's side, the padding round the body, the space between the
/// picture and the words, and the picture's top inset.
const SIDE: f32 = 48.0;
const PAD: f32 = metrics::SPACE_L;
const GAP: f32 = 10.0;
const INSET: f32 = 4.0;
/// The name's line (`height: 20px`) and the description's two lines.
const NAME_LINE: f32 = 20.0;
const DESCRIPTION_LINES: f32 = 2.0;
/// The glyph a Unicode emoji shows without a picture.
const GLYPH: f32 = 40.0;
/// The card's drop shadow (`drop-shadow-blur: 24px; drop-shadow-offset-y: 8px`).
const SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 128.0 / 255.0),
    offset: Vector::new(0.0, 8.0),
    blur_radius: 24.0,
};

/// What `Layer::MessageCard` shows: the emoji's card, else the sticker's;
/// nothing while both are closed.
pub fn view(cards: &Cards, motion: Motion) -> impl View<Intent> + '_ {
    let card: Element<'_, Intent> = if let Some(emoji) = &cards.emoji {
        emoji_card(emoji, motion)
    } else if let Some(sticker) = &cards.sticker {
        sticker_card(sticker, motion)
    } else {
        space().boxed()
    };
    card
}

fn emoji_card(card: &Emoji, motion: Motion) -> Element<'_, Intent> {
    let still: Element<'_, Intent> = match &card.picture {
        Expected::Ready(handle) => art(picture(handle.clone()), motion),
        // Twemoji's glyph runs a little past 48 px at 40: centred, it spills
        // evenly both sides, as the Slint card's does.
        _ => container(
            text(card.glyph.as_str())
                .size(GLYPH)
                .width(SIDE)
                .center()
                .wrapping(text::Wrapping::None),
        )
        .center(SIDE)
        .boxed(),
    };
    surface(
        card.name.as_str(),
        still,
        &card.description,
        card.server.as_ref(),
        tr!("This emoji is from"),
        Look::Emoji,
        motion,
    )
}

fn sticker_card(card: &Sticker, motion: Motion) -> Element<'_, Intent> {
    let still: Element<'_, Intent> = match &card.picture {
        Some(handle) => art(picture(handle.clone()), motion),
        None => space().width(SIDE).height(SIDE).boxed(),
    };
    surface(
        card.name.as_str(),
        still,
        &card.description,
        card.server.as_ref(),
        tr!("This sticker is from"),
        Look::Sticker,
        motion,
    )
}

fn art(picture: anomi_widgets::picture::Picture, motion: Motion) -> Element<'static, Intent> {
    picture.side(SIDE).fit(Fit::Contain).motion(motion).boxed()
}

/// The picture, its name and description, and the server under them.
fn surface<'a>(
    name: &'a str,
    art: Element<'a, Intent>,
    description: &'a str,
    server: Option<&'a Origin>,
    heading: &'a str,
    look: Look,
    motion: Motion,
) -> Element<'a, Intent> {
    let words = Column::<Element<'a, Intent>>::new()
        .push(
            container(
                text(name)
                    .size(typography::size::TITLE)
                    .font(typography::SEMIBOLD)
                    .style(catalog::text::primary)
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End),
            )
            .height(NAME_LINE)
            .align_y(Alignment::Center)
            .boxed(),
        )
        .push(
            // Two lines, cut where a third would start rather than elided.
            container(
                text(description)
                    .size(typography::size::SMALL)
                    .style(catalog::text::secondary),
            )
            .width(Fill)
            .clip(true)
            .height(
                Length::Shrink.max(
                    DESCRIPTION_LINES
                        * text::LineHeight::default()
                            .to_absolute(iced::Pixels(typography::size::SMALL))
                            .0,
                ),
            )
            .boxed(),
        )
        .width(Fill);
    let head = Row::<Element<'a, Intent>>::new()
        .push(container(art).padding(Padding::ZERO.top(INSET)).boxed())
        .push(words.boxed())
        .spacing(GAP)
        .padding(PAD);
    let mut body = Column::<Element<'a, Intent>>::new().push(head.boxed());
    if let Some(origin) = server {
        body = body.push(origin::view(origin, heading, look, motion));
    }
    container(body)
        .width(WIDTH)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().raised)),
            border: Border {
                color: theme.palette().edge,
                width: 1.0,
                radius: metrics::RADIUS_CARD.into(),
            },
            shadow: SHADOW,
            ..container::Style::default()
        })
        .boxed()
}
